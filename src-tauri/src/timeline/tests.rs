use super::snapshot::{SnapshotRow, Taken};
use super::*;
use crate::journal::Journal;

const GIB: u64 = 1 << 30;
const NOW: i64 = 1_792_584_000_000;
const DAY: i64 = retention::DAY_MS;

fn c_drive(letter: &str) -> VolumeIdentity {
    VolumeIdentity::new(Some("\\\\?\\Volume{c0000000-0000-0000-0000-000000000000}\\"), 1, "NTFS", "Windows", letter)
}

fn present(letter: &str) -> Vec<Present> {
    vec![Present { identity: c_drive(letter), total: 500 * GIB, free: 80 * GIB }]
}

fn taken(used: u64, rows: &[(&str, u64)]) -> Taken {
    Taken { used, files: 1, rows: rows.iter().map(|(p, s)| SnapshotRow { path: (*p).into(), allocated: *s, files: 1 }).collect(), min_bytes: 1 << 20 }
}

fn put(j: &mut Journal, root: &str, method: &'static str, source: &'static str, at: i64, t: &Taken) -> i64 {
    let v = c_drive("C:");
    let s = NewSnapshot { volume: &v, scan_serial: 7, root, method, source, taken_at: at, volume_total: Some(500 * GIB), volume_free: Some(100 * GIB), taken: t };
    store::insert(j.conn_mut(), &s).unwrap()
}

fn profile() -> (String, String) {
    ("C:".into(), "\\Users\\Arthur".into())
}

#[test]
fn courbe_reperes_et_projection_d_une_serie() {
    let mut j = Journal::in_memory().unwrap();
    // 20 jours à +1 Gio par jour, nettoyage de Lumen de 44 Gio il y a 10 jours.
    let cleaned_at = NOW - 10 * DAY - 3_600_000;
    for d in (0..20).rev() {
        let at = NOW - d * DAY;
        let used = 200 * GIB + (20 - d as u64) * GIB - if at > cleaned_at { 44 * GIB } else { 0 };
        put(&mut j, "\\Users\\Arthur", "walk", "daily", at, &taken(used, &[("AppData", used / 2)]));
    }
    j.conn()
        .execute(
            "INSERT INTO actions (started_at, finished_at, title, method, plan_hash, status, bytes_planned, bytes_freed,
               items_done, items_partial, items_skipped, items_refused, items_failed, items_cancelled, script, kind, volume_key)
             VALUES (?1, ?1, 'Cache npm', 'trash', 'ab', 'ok', 0, 0, 1, 0, 0, 0, 0, 0, '', 'recos', ?2)",
            rusqlite::params![cleaned_at, c_drive("C:").key],
        )
        .unwrap();
    let action = j.conn().last_insert_rowid();
    j.conn()
        .execute(
            "INSERT INTO action_items (action_id, path, directory, size, method, result) VALUES (?1, 'C:\\Users\\Arthur\\AppData\\Local\\npm-cache', 1, ?2, 'trash', 'done')",
            rusqlite::params![action, (44 * GIB) as i64],
        )
        .unwrap();
    let s = &store::series(j.conn()).unwrap()[0];
    let key = SeriesKey { volume_id: s.volume_id, root: s.root.clone(), method: s.method.clone() };
    // La lettre du volume a changé depuis (C: → E:) : la série suit son GUID.
    let t = timeline_of(j.conn(), &key, &present("E:"), Some(&profile()), NOW).unwrap();
    assert_eq!(t.points.len(), 20);
    assert_eq!(t.markers.len(), 1);
    assert_eq!((t.markers[0].bytes, t.markers[0].title.as_str()), (44 * GIB, "Cache npm"));
    assert_eq!(t.projection.status, "growing");
    let per_day = t.projection.bytes_per_day.unwrap_or(0.0) / GIB as f64;
    assert!((per_day - 1.0).abs() < 1e-6, "{per_day}");
    // 100 Gio libres au dernier snapshot, +1 Gio par jour.
    assert!((t.projection.days_to_full.unwrap_or(0.0) - 100.0).abs() < 1e-3);
    assert_eq!(t.free, Some(80 * GIB));
    assert_eq!(t.base.as_deref(), Some("E:\\Users\\Arthur"));
    // Le profil était en C: : en E:, ce n'est plus le profil (pas de cible d'analyse).
    assert_eq!(t.target, None);
    let t = timeline_of(j.conn(), &key, &present("C:"), Some(&profile()), NOW).unwrap();
    assert_eq!(t.target, Some(TargetDto::UserProfile));
    // Volume absent : dernier espace libre connu, pas de chemin.
    let t = timeline_of(j.conn(), &key, &[], Some(&profile()), NOW).unwrap();
    assert_eq!((t.free, t.base), (Some(100 * GIB), None));
    assert_eq!(t.trend7d, Some(7 * GIB as i64));
}

#[test]
fn comparaison_seulement_dans_une_meme_serie() {
    let mut j = Journal::in_memory().unwrap();
    let a = put(&mut j, "\\", "walk", "scan", NOW - 7 * DAY, &taken(300 * GIB, &[("Users", 200 * GIB), ("Users\\Arthur", 190 * GIB), ("Jeux", 50 * GIB)]));
    let b = put(&mut j, "\\", "walk", "daily", NOW, &taken(310 * GIB, &[("Users", 260 * GIB), ("Users\\Arthur", 250 * GIB)]));
    let mft = put(&mut j, "\\", "mft", "scan", NOW, &taken(310 * GIB, &[]));
    let profile_snap = put(&mut j, "\\Users\\Arthur", "walk", "scan", NOW, &taken(250 * GIB, &[]));

    // Dans l'ordre inverse : remis dans l'ordre des dates.
    let c = compare_in(j.conn(), b, a, &present("C:"), Some(&profile())).unwrap();
    assert!(c.comparable);
    assert_eq!((c.from.id, c.to.id), (a, b));
    let d = c.diff.unwrap();
    assert_eq!(d.total, 10 * GIB as i64);
    assert_eq!(d.grown.iter().map(|x| x.path.as_str()).collect::<Vec<_>>(), ["Users\\Arthur"]);
    assert_eq!(d.removed.iter().map(|x| x.path.as_str()).collect::<Vec<_>>(), ["Jeux"]);
    assert_eq!(c.base.as_deref(), Some("C:\\"));
    assert_eq!(c.target, Some(TargetDto::Volume { letter: "C:".into() }));

    // Autre méthode ou autre racine : pas de faux delta.
    let m = compare_in(j.conn(), a, mft, &present("C:"), None).unwrap();
    assert_eq!((m.comparable, m.reason, m.diff.is_none(), m.base.is_none()), (false, Some("method"), true, true));
    let r = compare_in(j.conn(), a, profile_snap, &present("C:"), None).unwrap();
    assert_eq!((r.comparable, r.reason), (false, Some("root")));
    assert!(compare_in(j.conn(), a, 9999, &present("C:"), None).is_err());
}

#[test]
fn snapshot_quotidien_du_bon_dossier() {
    let s = |root: &str, method: &str, last: i64, last_scan: Option<i64>| Series {
        volume_id: 1,
        key: "k".into(),
        label: String::new(),
        fs: "NTFS".into(),
        letter: "C:".into(),
        root: root.into(),
        method: method.into(),
        count: 1,
        first: last,
        last,
        last_scan,
    };
    let all = [
        // Snapshot de moins de 20 h : pas encore dû.
        s("\\Users\\Arthur", "walk", NOW - 3_600_000, Some(NOW - 3_600_000)),
        // Dû, analysé par l'utilisateur il y a 2 jours.
        s("\\", "walk", NOW - 2 * DAY, Some(NOW - 2 * DAY)),
        // Dû, mais analysé il y a plus longtemps.
        s("\\Jeux", "walk", NOW - 3 * DAY, Some(NOW - 5 * DAY)),
        // Plus analysé par l'utilisateur depuis 40 jours : plus de snapshot quotidien.
        s("\\Vieux", "walk", NOW - 40 * DAY, Some(NOW - 40 * DAY)),
        // MFT : jamais d'UAC pour un snapshot quotidien.
        s("\\Mft", "mft", NOW - 2 * DAY, Some(NOW - DAY)),
    ];
    let mounted = |_: &Series| true;
    let (pick, admin) = due(&all, &mounted, NOW, false);
    assert_eq!(pick.map(|p| p.root.as_str()), Some("\\"));
    assert_eq!(admin, 1);
    let (pick, admin) = due(&all, &mounted, NOW, true);
    assert_eq!((pick.map(|p| p.root.as_str()), admin), (Some("\\Mft"), 0));
    // Volume démonté : rien.
    assert_eq!(due(&all, &|_: &Series| false, NOW, true).0, None);
}

#[test]
fn cible_d_analyse_d_une_serie() {
    let p = profile();
    assert_eq!(target_of("\\", Some("D:"), Some(&p)), Some(TargetDto::Volume { letter: "D:".into() }));
    assert_eq!(target_of("\\users\\arthur", Some("C:"), Some(&p)), Some(TargetDto::UserProfile));
    assert_eq!(target_of("\\Users\\Arthur", Some("D:"), Some(&p)), None);
    assert_eq!(target_of("\\", None, Some(&p)), None);
    assert_eq!(base_path("C:", "\\"), "C:\\");
    assert_eq!(base_path("C:", "\\Users\\Arthur"), "C:\\Users\\Arthur");
}
