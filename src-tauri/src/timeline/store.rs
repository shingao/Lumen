//! Snapshots dans la base de Lumen (schéma v4 de `journal.rs`). Une série = un volume (par son
//! GUID ou son numéro de série, jamais par sa lettre) + une racine relative au volume + une
//! méthode d'analyse : seuls les snapshots d'une même série sont comparables.

use rusqlite::{params, Connection, OptionalExtension};

use super::diff::Row;
use super::retention;
use super::snapshot::Taken;

/// Identité stable d'un volume. La lettre n'est qu'un dernier état connu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeIdentity {
    pub key: String,
    pub guid: Option<String>,
    pub serial: u32,
    pub fs: String,
    pub label: String,
    pub letter: String,
}

impl VolumeIdentity {
    /// Clé : `guid:{…}` (`\\?\Volume{GUID}\`), à défaut `serial:XXXXXXXX:FS` (lecteurs sans GUID).
    pub fn new(guid: Option<&str>, serial: u32, fs: &str, label: &str, letter: &str) -> VolumeIdentity {
        let guid = guid.and_then(|g| {
            let start = g.find('{')?;
            let end = g[start..].find('}')? + start;
            Some(g[start..=end].to_ascii_lowercase())
        });
        let key = match &guid {
            Some(g) => format!("guid:{g}"),
            None => format!("serial:{serial:08X}:{}", fs.to_ascii_uppercase()),
        };
        VolumeIdentity { key, guid, serial, fs: fs.to_owned(), label: label.to_owned(), letter: letter.to_ascii_uppercase() }
    }
}

/// Racine d'une analyse : lettre (« C: », vide hors Windows) et chemin relatif au volume
/// (« \ » pour le volume entier).
pub fn split_root(root_path: &str) -> (String, String) {
    let root_path = root_path.strip_prefix("\\\\?\\").unwrap_or(root_path);
    let b = root_path.as_bytes();
    if b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
        let rel = root_path[2..].trim_end_matches('\\');
        return (root_path[..2].to_ascii_uppercase(), if rel.is_empty() { "\\".into() } else { rel.to_owned() });
    }
    (String::new(), root_path.to_owned())
}

/// Chemin `rel` (relatif au volume) dans la racine `root` ?
fn under(rel: &str, root: &str) -> bool {
    if root == "\\" {
        return true;
    }
    let (r, p) = (root.to_lowercase(), rel.to_lowercase());
    p == r || p.strip_prefix(&r).is_some_and(|rest| rest.starts_with('\\'))
}

pub struct NewSnapshot<'a> {
    pub volume: &'a VolumeIdentity,
    /// Numéro de série vu par l'analyse (FILE_ID_INFO), celui des éléments du journal.
    pub scan_serial: u64,
    pub root: &'a str,
    pub method: &'static str,
    pub source: &'static str,
    pub taken_at: i64,
    pub volume_total: Option<u64>,
    pub volume_free: Option<u64>,
    pub taken: &'a Taken,
}

/// Enregistre un snapshot, puis applique la rétention à sa série.
pub fn insert(conn: &mut Connection, s: &NewSnapshot<'_>) -> rusqlite::Result<i64> {
    let tx = conn.transaction()?;
    let v = s.volume;
    tx.execute(
        "INSERT INTO volumes (key, guid, serial, scan_serial, fs, label, letter, seen_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(key) DO UPDATE SET guid = excluded.guid, serial = excluded.serial, scan_serial = excluded.scan_serial,
           fs = excluded.fs, label = excluded.label, letter = excluded.letter, seen_at = excluded.seen_at",
        params![v.key, v.guid, v.serial, s.scan_serial as i64, v.fs, v.label, v.letter, s.taken_at],
    )?;
    let volume_id: i64 = tx.query_row("SELECT id FROM volumes WHERE key = ?1", params![v.key], |r| r.get(0))?;
    tx.execute(
        "INSERT INTO snapshots (volume_id, root, method, source, taken_at, used, files, volume_total, volume_free, letter, min_bytes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            volume_id,
            s.root,
            s.method,
            s.source,
            s.taken_at,
            s.taken.used as i64,
            s.taken.files as i64,
            s.volume_total.map(|x| x as i64),
            s.volume_free.map(|x| x as i64),
            v.letter,
            s.taken.min_bytes as i64,
        ],
    )?;
    let id = tx.last_insert_rowid();
    {
        let mut path = tx.prepare_cached("INSERT INTO snapshot_paths (path) VALUES (?1) ON CONFLICT(path) DO UPDATE SET path = excluded.path RETURNING id")?;
        let mut size = tx.prepare_cached("INSERT INTO snapshot_sizes (snapshot_id, path_id, allocated, files) VALUES (?1, ?2, ?3, ?4)")?;
        for r in &s.taken.rows {
            let pid: i64 = path.query_row(params![r.path], |x| x.get(0))?;
            size.execute(params![id, pid, r.allocated as i64, r.files as i64])?;
        }
    }
    apply_retention(&tx, volume_id, s.root, s.method, s.taken_at)?;
    tx.commit()?;
    Ok(id)
}

/// Rétention d'une série à la date `now`, puis oubli des chemins qui ne servent plus.
pub fn apply_retention(conn: &Connection, volume_id: i64, root: &str, method: &str, now: i64) -> rusqlite::Result<usize> {
    let all: Vec<(i64, i64)> = conn
        .prepare("SELECT id, taken_at FROM snapshots WHERE volume_id = ?1 AND root = ?2 AND method = ?3")?
        .query_map(params![volume_id, root, method], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let gone = retention::to_delete(&all, now);
    let mut del = conn.prepare_cached("DELETE FROM snapshots WHERE id = ?1")?;
    for id in &gone {
        del.execute(params![id])?;
    }
    if !gone.is_empty() {
        conn.execute("DELETE FROM snapshot_paths WHERE NOT EXISTS (SELECT 1 FROM snapshot_sizes s WHERE s.path_id = snapshot_paths.id)", [])?;
    }
    Ok(gone.len())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Series {
    pub volume_id: i64,
    pub key: String,
    pub label: String,
    pub fs: String,
    /// Dernière lettre connue (peut avoir changé).
    pub letter: String,
    pub root: String,
    pub method: String,
    pub count: u64,
    pub first: i64,
    pub last: i64,
    /// Dernier snapshot pris par une analyse de l'utilisateur.
    pub last_scan: Option<i64>,
    /// Identifiant du snapshot le plus récent.
    pub last_id: i64,
}

pub fn series(conn: &Connection) -> rusqlite::Result<Vec<Series>> {
    conn.prepare(
        "SELECT v.id, v.key, v.label, v.fs, v.letter, s.root, s.method, COUNT(*), MIN(s.taken_at), MAX(s.taken_at),
                MAX(CASE WHEN s.source = 'scan' THEN s.taken_at END),
                (SELECT l.id FROM snapshots l WHERE l.volume_id = s.volume_id AND l.root = s.root AND l.method = s.method ORDER BY l.taken_at DESC, l.id DESC LIMIT 1)
         FROM snapshots s JOIN volumes v ON v.id = s.volume_id
         GROUP BY s.volume_id, s.root, s.method ORDER BY MAX(s.taken_at) DESC",
    )?
    .query_map([], |r| {
        Ok(Series {
            volume_id: r.get(0)?,
            key: r.get(1)?,
            label: r.get(2)?,
            fs: r.get(3)?,
            letter: r.get(4)?,
            root: r.get(5)?,
            method: r.get(6)?,
            count: r.get::<_, i64>(7)? as u64,
            first: r.get(8)?,
            last: r.get(9)?,
            last_scan: r.get(10)?,
            last_id: r.get(11)?,
        })
    })?
    .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotInfo {
    pub id: i64,
    pub volume_id: i64,
    pub root: String,
    pub method: String,
    pub source: String,
    pub taken_at: i64,
    pub used: u64,
    pub files: u64,
    pub volume_total: Option<u64>,
    pub volume_free: Option<u64>,
    pub letter: String,
    pub min_bytes: u64,
}

const INFO: &str = "id, volume_id, root, method, source, taken_at, used, files, volume_total, volume_free, letter, min_bytes";

fn info(r: &rusqlite::Row<'_>) -> rusqlite::Result<SnapshotInfo> {
    Ok(SnapshotInfo {
        id: r.get(0)?,
        volume_id: r.get(1)?,
        root: r.get(2)?,
        method: r.get(3)?,
        source: r.get(4)?,
        taken_at: r.get(5)?,
        used: r.get::<_, i64>(6)? as u64,
        files: r.get::<_, i64>(7)? as u64,
        volume_total: r.get::<_, Option<i64>>(8)?.map(|x| x as u64),
        volume_free: r.get::<_, Option<i64>>(9)?.map(|x| x as u64),
        letter: r.get(10)?,
        min_bytes: r.get::<_, i64>(11)? as u64,
    })
}

/// Snapshots d'une série, du plus ancien au plus récent.
pub fn points(conn: &Connection, volume_id: i64, root: &str, method: &str) -> rusqlite::Result<Vec<SnapshotInfo>> {
    conn.prepare(&format!("SELECT {INFO} FROM snapshots WHERE volume_id = ?1 AND root = ?2 AND method = ?3 ORDER BY taken_at, id"))?
        .query_map(params![volume_id, root, method], info)?
        .collect()
}

pub fn snapshot(conn: &Connection, id: i64) -> rusqlite::Result<Option<SnapshotInfo>> {
    conn.query_row(&format!("SELECT {INFO} FROM snapshots WHERE id = ?1"), params![id], info).optional()
}

pub fn rows(conn: &Connection, id: i64) -> rusqlite::Result<Vec<Row>> {
    conn.prepare("SELECT p.path, s.allocated FROM snapshot_sizes s JOIN snapshot_paths p ON p.id = s.path_id WHERE s.snapshot_id = ?1")?
        .query_map(params![id], |r| Ok(Row { path: r.get(0)?, allocated: r.get::<_, i64>(1)? as u64 }))?
        .collect()
}

/// Nettoyage de Lumen à marquer sur la courbe d'une série.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    pub action_id: i64,
    pub at: i64,
    pub bytes: u64,
    pub title: String,
    pub kind: String,
}

/// Nettoyages de Lumen sur le volume de la série, avec ce qu'ils ont retiré de sa racine :
/// éléments supprimés sous la racine ; actions système (hors de tout dossier utilisateur) :
/// gain mesuré, seulement pour la série du volume entier. Les nettoyages d'avant le jalon 7
/// (sans volume retenu) sont retrouvés par le numéro de série de leurs éléments.
pub fn markers(conn: &Connection, volume_id: i64, root: &str) -> rusqlite::Result<Vec<Marker>> {
    let (key, scan_serial): (String, Option<i64>) = conn.query_row("SELECT key, scan_serial FROM volumes WHERE id = ?1", params![volume_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let actions: Vec<(i64, i64, String, String, Option<i64>)> = conn
        .prepare(
            "SELECT a.id, a.started_at, a.title, a.kind, a.bytes_measured FROM actions a
             WHERE a.volume_key = ?1
                OR (a.volume_key IS NULL AND ?2 IS NOT NULL AND EXISTS (SELECT 1 FROM action_items i WHERE i.action_id = a.id AND i.volume_serial = ?2))
             ORDER BY a.started_at",
        )?
        .query_map(params![key, scan_serial], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
        .collect::<Result<_, _>>()?;
    let mut items = conn.prepare_cached("SELECT path, size FROM action_items WHERE action_id = ?1 AND result = 'done'")?;
    let mut out = Vec::new();
    for (id, at, title, kind, measured) in actions {
        let bytes = if kind == "system" {
            if root == "\\" { measured.unwrap_or(0).max(0) as u64 } else { 0 }
        } else {
            items
                .query_map(params![id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
                .filter_map(Result::ok)
                .filter(|(p, _)| under(&split_root(p).1, root))
                .map(|(_, s)| s.max(0) as u64)
                .sum()
        };
        if bytes > 0 {
            out.push(Marker { action_id: id, at, bytes, title, kind });
        }
    }
    Ok(out)
}

/// Variation sur 7 jours : dernier snapshot moins le dernier pris il y a au moins 7 jours.
pub fn trend_7d(points: &[SnapshotInfo], now: i64) -> Option<i64> {
    let last = points.last()?;
    let before = points.iter().rev().find(|p| p.taken_at <= now - 7 * retention::DAY_MS)?;
    Some(last.used as i64 - before.used as i64)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DbStats {
    pub bytes: u64,
    pub snapshots: u64,
    pub series: u64,
}

pub fn stats(conn: &Connection) -> rusqlite::Result<DbStats> {
    let pages: i64 = conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    let size: i64 = conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    let snapshots: i64 = conn.query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))?;
    let series: i64 = conn.query_row("SELECT COUNT(*) FROM (SELECT 1 FROM snapshots GROUP BY volume_id, root, method)", [], |r| r.get(0))?;
    Ok(DbStats { bytes: (pages * size).max(0) as u64, snapshots: snapshots as u64, series: series as u64 })
}

/// Purge : tous les snapshots (le journal des nettoyages est gardé : il sert à restaurer),
/// puis la base est compactée.
pub fn purge(conn: &Connection) -> rusqlite::Result<DbStats> {
    conn.execute_batch("DELETE FROM snapshots; DELETE FROM snapshot_paths;")?;
    conn.execute_batch("VACUUM;")?;
    // En WAL, le fichier ne rétrécit qu'une fois le journal recopié.
    let _ = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    stats(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::Journal;
    use crate::timeline::snapshot::{SnapshotRow, Taken};

    const GIB: u64 = 1 << 30;
    const NOW: i64 = 1_792_584_000_000;
    const DAY: i64 = retention::DAY_MS;

    fn taken(used: u64, rows: &[(&str, u64)]) -> Taken {
        Taken { used, files: 10, rows: rows.iter().map(|(p, s)| SnapshotRow { path: (*p).into(), allocated: *s, files: 1 }).collect(), min_bytes: 1 << 20 }
    }

    fn snap(j: &mut Journal, v: &VolumeIdentity, root: &str, method: &'static str, at: i64, t: &Taken) -> i64 {
        let s = NewSnapshot { volume: v, scan_serial: 0xABCD, root, method, source: "scan", taken_at: at, volume_total: Some(500 * GIB), volume_free: Some(100 * GIB), taken: t };
        insert(j.conn_mut(), &s).unwrap()
    }

    #[test]
    fn identite_des_volumes() {
        let a = VolumeIdentity::new(Some("\\\\?\\Volume{4C1E7A90-2B3D-4E5F-8A6B-1C2D3E4F5A6B}\\"), 0x1234_ABCD, "NTFS", "Data", "e:");
        assert_eq!(a.key, "guid:{4c1e7a90-2b3d-4e5f-8a6b-1c2d3e4f5a6b}");
        assert_eq!(a.letter, "E:");
        let b = VolumeIdentity::new(None, 0x1234_ABCD, "exFAT", "USB", "F:");
        assert_eq!(b.key, "serial:1234ABCD:EXFAT");
        assert_eq!(split_root("C:\\"), ("C:".into(), "\\".into()));
        assert_eq!(split_root("\\\\?\\D:\\Jeux"), ("D:".into(), "\\Jeux".into()));
        assert_eq!(split_root("c:\\Users\\Arthur\\"), ("C:".into(), "\\Users\\Arthur".into()));
        assert!(under("\\Users\\Arthur\\AppData", "\\users\\arthur") && !under("\\Users\\Arthur2", "\\Users\\Arthur"));
    }

    #[test]
    fn changement_de_lettre_de_volume() {
        let mut j = Journal::in_memory().unwrap();
        let guid = "\\\\?\\Volume{11111111-2222-3333-4444-555555555555}\\";
        // Clé USB vue en E:, puis réattribuée en F: : même série.
        let usb_e = VolumeIdentity::new(Some(guid), 0xAA, "exFAT", "Photos", "E:");
        let usb_f = VolumeIdentity::new(Some(guid), 0xAA, "exFAT", "Photos", "F:");
        // Un autre disque prend la lettre E: : autre série.
        let other_e = VolumeIdentity::new(Some("\\\\?\\Volume{99999999-2222-3333-4444-555555555555}\\"), 0xBB, "NTFS", "Backup", "E:");
        snap(&mut j, &usb_e, "\\", "walk", NOW - 3 * DAY, &taken(10 * GIB, &[("DCIM", 9 * GIB)]));
        snap(&mut j, &usb_f, "\\", "walk", NOW - DAY, &taken(12 * GIB, &[("DCIM", 11 * GIB)]));
        snap(&mut j, &other_e, "\\", "walk", NOW, &taken(300 * GIB, &[("Backups", 290 * GIB)]));
        let all = series(j.conn()).unwrap();
        assert_eq!(all.len(), 2);
        let usb = all.iter().find(|s| s.label == "Photos").unwrap();
        assert_eq!((usb.count, usb.letter.as_str()), (2, "F:"));
        let pts = points(j.conn(), usb.volume_id, "\\", "walk").unwrap();
        assert_eq!(pts.iter().map(|p| p.letter.as_str()).collect::<Vec<_>>(), ["E:", "F:"]);
        assert_eq!(pts.iter().map(|p| p.used).collect::<Vec<_>>(), [10 * GIB, 12 * GIB]);
        let backup = all.iter().find(|s| s.label == "Backup").unwrap();
        assert_eq!((backup.count, backup.letter.as_str()), (1, "E:"));
        // Sans GUID (lecteur qui n'en a pas) : identifié par numéro de série + système de fichiers.
        let nas_g = VolumeIdentity::new(None, 0xCC, "NTFS", "", "G:");
        let nas_h = VolumeIdentity::new(None, 0xCC, "NTFS", "", "H:");
        snap(&mut j, &nas_g, "\\", "walk", NOW - DAY, &taken(GIB, &[]));
        snap(&mut j, &nas_h, "\\", "walk", NOW, &taken(GIB, &[]));
        assert_eq!(series(j.conn()).unwrap().len(), 3);
    }

    #[test]
    fn series_par_racine_et_methode() {
        let mut j = Journal::in_memory().unwrap();
        let c = VolumeIdentity::new(Some("{C0000000-0000-0000-0000-000000000000}"), 1, "NTFS", "Windows", "C:");
        snap(&mut j, &c, "\\Users\\Arthur", "walk", NOW - DAY, &taken(GIB, &[]));
        snap(&mut j, &c, "\\Users\\Arthur", "walk", NOW, &taken(GIB, &[]));
        snap(&mut j, &c, "\\Users\\Arthur", "mft", NOW, &taken(GIB, &[]));
        snap(&mut j, &c, "\\", "walk", NOW, &taken(GIB, &[]));
        let all = series(j.conn()).unwrap();
        assert_eq!(all.len(), 3);
        // La racine se compare sans tenir compte de la casse.
        snap(&mut j, &c, "\\users\\arthur", "walk", NOW, &taken(GIB, &[]));
        assert_eq!(points(j.conn(), all[0].volume_id, "\\Users\\Arthur", "walk").unwrap().len(), 3);
    }

    #[test]
    fn retention_appliquee_a_l_ecriture_et_purge() {
        let mut j = Journal::in_memory().unwrap();
        let c = VolumeIdentity::new(Some("{C0000000-0000-0000-0000-000000000000}"), 1, "NTFS", "Windows", "C:");
        // Un an de snapshots, du plus ancien au plus récent, chacun avec ses dossiers.
        for d in (0..365).rev() {
            for h in [2, 14] {
                let rows = [("AppData", (d as u64 + 1) * GIB), (if d % 2 == 0 { "Pair" } else { "Impair" }, GIB)];
                snap(&mut j, &c, "\\", "walk", NOW - d * DAY - h * 3_600_000, &taken(500 * GIB, &rows));
            }
        }
        let st = stats(j.conn()).unwrap();
        assert!(st.snapshots < 90, "{}", st.snapshots);
        assert_eq!(st.series, 1);
        // Les tailles des snapshots supprimés sont parties avec eux.
        let sizes: i64 = j.conn().query_row("SELECT COUNT(*) FROM snapshot_sizes", [], |r| r.get(0)).unwrap();
        assert_eq!(sizes as u64, st.snapshots * 2);
        let after = purge(j.conn()).unwrap();
        assert_eq!((after.snapshots, after.series), (0, 0));
        let paths: i64 = j.conn().query_row("SELECT COUNT(*) FROM snapshot_paths", [], |r| r.get(0)).unwrap();
        assert_eq!(paths, 0);
    }

    fn action(j: &Journal, at: i64, kind: &str, volume_key: Option<&str>, measured: Option<i64>, items: &[(&str, u64, Option<i64>, &str)]) -> i64 {
        j.conn()
            .execute(
                "INSERT INTO actions (started_at, finished_at, title, method, plan_hash, status, bytes_planned, bytes_freed, bytes_measured,
                   items_done, items_partial, items_skipped, items_refused, items_failed, items_cancelled, script, kind, volume_key)
                 VALUES (?1, ?1, 'Nettoyage', 'trash', 'ab', 'ok', 0, 0, ?2, 0, 0, 0, 0, 0, 0, '', ?3, ?4)",
                params![at, measured, kind, volume_key],
            )
            .unwrap();
        let id = j.conn().last_insert_rowid();
        for (path, size, serial, result) in items {
            j.conn()
                .execute(
                    "INSERT INTO action_items (action_id, path, directory, size, method, result, volume_serial) VALUES (?1, ?2, 1, ?3, 'trash', ?4, ?5)",
                    params![id, path, *size as i64, result, serial],
                )
                .unwrap();
        }
        id
    }

    #[test]
    fn reperes_des_nettoyages() {
        let mut j = Journal::in_memory().unwrap();
        let c = VolumeIdentity::new(Some("{C0000000-0000-0000-0000-000000000000}"), 1, "NTFS", "Windows", "C:");
        snap(&mut j, &c, "\\Users\\Arthur", "walk", NOW - 10 * DAY, &taken(GIB, &[]));
        let key = c.key.as_str();
        let a1 = action(&j, NOW - 8 * DAY, "recos", Some(key), None, &[("C:\\Users\\Arthur\\AppData\\npm-cache", 44 * GIB, Some(0xABCD), "done"), ("C:\\Temp\\x", GIB, Some(0xABCD), "done"), ("C:\\Users\\Arthur\\verrou", GIB, Some(0xABCD), "skipped")]);
        // Nettoyage d'avant le jalon 7 : sans volume retenu, retrouvé par le numéro de série.
        let a2 = action(&j, NOW - 6 * DAY, "duplicates", None, None, &[("C:\\Users\\Arthur\\Videos\\a.mp4", 2 * GIB, Some(0xABCD), "done")]);
        let a3 = action(&j, NOW - 4 * DAY, "system", Some(key), Some(10 * GIB as i64), &[]);
        action(&j, NOW - 2 * DAY, "recos", Some("guid:{autre}"), None, &[("D:\\x", 5 * GIB, Some(0x1), "done")]);
        action(&j, NOW - DAY, "recos", None, None, &[("C:\\Users\\Arthur\\y", 5 * GIB, Some(0x1), "done")]);
        let vid = series(j.conn()).unwrap()[0].volume_id;
        let m: Vec<(i64, u64)> = markers(j.conn(), vid, "\\Users\\Arthur").unwrap().iter().map(|m| (m.action_id, m.bytes)).collect();
        assert_eq!(m, [(a1, 44 * GIB), (a2, 2 * GIB)]);
        let m: Vec<(i64, u64)> = markers(j.conn(), vid, "\\").unwrap().iter().map(|m| (m.action_id, m.bytes)).collect();
        assert_eq!(m, [(a1, 45 * GIB), (a2, 2 * GIB), (a3, 10 * GIB)]);
    }

    #[test]
    fn tendance_sur_sept_jours() {
        let p = |at: i64, used: u64| SnapshotInfo { id: 0, volume_id: 1, root: "\\".into(), method: "walk".into(), source: "scan".into(), taken_at: at, used, files: 0, volume_total: None, volume_free: None, letter: "C:".into(), min_bytes: 0 };
        assert_eq!(trend_7d(&[p(NOW - 3 * DAY, 10), p(NOW, 30)], NOW), None);
        assert_eq!(trend_7d(&[p(NOW - 9 * DAY, 5), p(NOW - 8 * DAY, 10), p(NOW - 2 * DAY, 20), p(NOW, 28)], NOW), Some(18));
    }
}
