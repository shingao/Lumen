//! Dernière analyse de chaque cible (dossier utilisateur ou disque), enregistrée dans la base de
//! Lumen, compressée : l'appli s'ouvre aussitôt sur le dernier résultat (« analysé il y a… ») et
//! la recherche fonctionne sans réanalyser. Tout reste local. Les file ID gardés ne remplacent
//! jamais le recontrôle de `safety` au moment d'agir : un plan se construit toujours sur une
//! analyse, et chaque élément repasse les six étapes avant d'être touché.

use rusqlite::{params, Connection};
use scan::Scan;

/// Analyse enregistrée, relue.
pub struct Saved {
    /// `userProfile` ou la lettre du disque (`C:`).
    pub target: String,
    /// Identité du volume (GUID…) au moment de l'analyse ; un autre disque à la même lettre
    /// ne reprend pas l'analyse d'un ancien.
    pub volume_key: Option<String>,
    pub finished_at: i64,
    pub scan: Scan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreStats {
    pub count: u64,
    /// Octets enregistrés (compressés).
    pub bytes: u64,
}

fn method(m: scan::Method) -> &'static str {
    match m {
        scan::Method::Walk => "walk",
        scan::Method::Mft => "mft",
    }
}

/// Enregistre (ou remplace) la dernière analyse d'une cible.
pub fn save(conn: &Connection, target: &str, volume_key: Option<&str>, finished_at: i64, scan: &Scan) -> rusqlite::Result<()> {
    let raw = scan.encode();
    let data = lz4_flex::compress_prepend_size(&raw);
    conn.execute(
        "INSERT INTO saved_scans (target, volume_key, root, method, finished_at, raw_bytes, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(target) DO UPDATE SET volume_key = excluded.volume_key, root = excluded.root, method = excluded.method,
           finished_at = excluded.finished_at, raw_bytes = excluded.raw_bytes, data = excluded.data",
        params![target, volume_key, scan.root_path, method(scan.method), finished_at, raw.len() as i64, data],
    )?;
    Ok(())
}

/// Ligne brute : cible, clé du volume, fin, taille décompressée, données.
type Row = (String, Option<String>, i64, i64, Vec<u8>);

/// Analyses enregistrées, de la plus récente à la plus ancienne. Une analyse illisible
/// (base abîmée, format d'une autre version) est oubliée, jamais une erreur.
pub fn load_all(conn: &Connection) -> rusqlite::Result<Vec<Saved>> {
    let mut stmt = conn.prepare("SELECT target, volume_key, finished_at, raw_bytes, data FROM saved_scans ORDER BY finished_at DESC")?;
    let rows: Vec<Row> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<Result<_, _>>()?;
    let mut out = Vec::new();
    for (target, volume_key, finished_at, raw_bytes, data) in rows {
        let scan = decompress(&data, raw_bytes).and_then(|raw| Scan::decode(&raw).ok());
        match scan {
            Some(scan) => out.push(Saved { target, volume_key, finished_at, scan }),
            None => {
                conn.execute("DELETE FROM saved_scans WHERE target = ?1", params![target])?;
            }
        }
    }
    Ok(out)
}

/// Taille annoncée vérifiée avant d'allouer (une base abîmée n'alloue pas des gigaoctets).
fn decompress(data: &[u8], raw_bytes: i64) -> Option<Vec<u8>> {
    let announced = data.get(..4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as i64)?;
    if announced != raw_bytes || raw_bytes <= 0 || raw_bytes > 4 << 30 {
        return None;
    }
    lz4_flex::decompress_size_prepended(data).ok()
}

pub fn stats(conn: &Connection) -> rusqlite::Result<StoreStats> {
    let (count, bytes): (i64, i64) = conn.query_row("SELECT COUNT(*), COALESCE(SUM(LENGTH(data)), 0) FROM saved_scans", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(StoreStats { count: count.max(0) as u64, bytes: bytes.max(0) as u64 })
}

/// Oublie toutes les analyses enregistrées (la base est compactée par l'appelant).
pub fn purge(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("DELETE FROM saved_scans;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::Journal;
    use scan::model::Builder;
    use scan::{Kind, Method, RawEntry, Stats};

    fn scan(root: &str, files: usize) -> Scan {
        let e = |name: String, kind: Kind, size: u64, id: u128| RawEntry { name, kind, attributes: 0, file_id: id, logical: size, allocated: size, modified: 1, accessed: 1 };
        let mut b = Builder::new(&e("racine".into(), Kind::Dir, 0, 1));
        let entries: Vec<RawEntry> = (0..files).map(|i| e(format!("fichier {i}.bin"), Kind::File, 1000 + i as u64, 10 + i as u128)).collect();
        b.add_children(0, &entries);
        b.finish(root.into(), 7, Method::Walk, true, Stats::default())
    }

    #[test]
    fn derniere_analyse_enregistree_compressee_et_relue() {
        let j = Journal::in_memory().unwrap();
        let c = j.conn();
        save(c, "C:", Some("guid:C"), 1000, &scan("C:\\", 5000)).unwrap();
        save(c, "userProfile", None, 2000, &scan("C:\\Users\\Arthur", 10)).unwrap();
        // Nouvelle analyse de C: : remplace l'ancienne.
        save(c, "C:", Some("guid:C"), 3000, &scan("C:\\", 6000)).unwrap();
        let all = load_all(c).unwrap();
        assert_eq!(all.iter().map(|s| (s.target.as_str(), s.finished_at)).collect::<Vec<_>>(), [("C:", 3000), ("userProfile", 2000)]);
        assert_eq!(all[0].scan.len(), 6001);
        assert_eq!(all[0].volume_key.as_deref(), Some("guid:C"));
        assert_eq!(all[0].scan.path(all[0].scan.find("fichier 42.bin").unwrap()), "C:\\fichier 42.bin");
        let st = stats(c).unwrap();
        assert_eq!(st.count, 2);
        // Compressée : bien plus petite que l'analyse encodée.
        assert!(st.bytes < scan("C:\\", 6000).encode().len() as u64 / 2, "{}", st.bytes);
        purge(c).unwrap();
        assert_eq!(stats(c).unwrap(), StoreStats { count: 0, bytes: 0 });
    }

    #[test]
    fn enregistrement_abime_oublie() {
        let j = Journal::in_memory().unwrap();
        let c = j.conn();
        save(c, "D:", None, 1, &scan("D:\\", 10)).unwrap();
        c.execute("UPDATE saved_scans SET data = X'0A000000FFFFFFFF'", []).unwrap();
        save(c, "E:", None, 2, &scan("E:\\", 10)).unwrap();
        c.execute("UPDATE saved_scans SET raw_bytes = 999999999999 WHERE target = 'E:'", []).unwrap();
        save(c, "F:", None, 3, &scan("F:\\", 10)).unwrap();
        let all = load_all(c).unwrap();
        assert_eq!(all.iter().map(|s| s.target.as_str()).collect::<Vec<_>>(), ["F:"]);
        assert_eq!(stats(c).unwrap().count, 1, "les enregistrements abîmés sont supprimés");
    }
}
