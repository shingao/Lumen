//! Sauvegardes de jeux : jamais proposées par Doublons, Dormants ni Gros fichiers, même en
//! double, dormantes et grosses (denylist de `safety`).

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use safety::denylist::{DenyList, SystemLayout};
use safety::path::WinPath;
use scan::model::Builder;
use scan::{Kind, Method, NodeId, RawEntry, Scan, Stats};

use super::{dormant, duplicates, largest};

const DAY: i64 = 864_000_000_000;
const NOW: i64 = 133_700_000_000_000_000;
const MB: u64 = 1 << 20;

fn deny() -> DenyList {
    let p = |s: &str| WinPath::parse(s).unwrap();
    DenyList::new(&SystemLayout {
        windows: p("C:\\Windows"),
        program_files: vec![p("C:\\Program Files"), p("C:\\Program Files (x86)")],
        program_data: p("C:\\ProgramData"),
        users: p("C:\\Users"),
        profile: p("C:\\Users\\Arthur"),
        public: p("C:\\Users\\Public"),
        other_profiles: Vec::new(),
    })
}

/// Profil où tout est vieux, gros et en double : deux films (témoins), deux copies d'une partie
/// `.sav`, une sauvegarde Skyrim dans `Saves` et sa copie, une partie Baldur's Gate 3 (`.lsv`).
fn profile() -> Scan {
    let old = NOW - 400 * DAY;
    let mut id = 0u128;
    let mut e = |name: &str, kind: Kind, size: u64| {
        id += 1;
        RawEntry { name: name.into(), kind, attributes: 0, file_id: id, logical: size, allocated: size, modified: old, accessed: old }
    };
    let mut b = Builder::new(&e("Arthur", Kind::Dir, 0));
    let top = b.add_children(0, &[e("Videos", Kind::Dir, 0), e("Documents", Kind::Dir, 0), e("Saved Games", Kind::Dir, 0)]);
    b.add_children(top.start, &[e("film.mp4", Kind::File, 900 * MB), e("film - copie.mp4", Kind::File, 900 * MB)]);
    let docs = b.add_children(top.start + 1, &[e("partie.sav", Kind::File, 600 * MB), e("partie (1).sav", Kind::File, 600 * MB), e("My Games", Kind::Dir, 0), e("BG3.lsv", Kind::File, 700 * MB)]);
    let my = b.add_children(docs.start + 2, &[e("Skyrim", Kind::Dir, 0)]);
    let sky = b.add_children(my.start, &[e("Saves", Kind::Dir, 0)]);
    b.add_children(sky.start, &[e("Save 12.ess", Kind::File, 500 * MB), e("Save 12 - copie.ess", Kind::File, 500 * MB)]);
    b.add_children(top.start + 2, &[e("Hades.dat", Kind::File, 500 * MB)]);
    b.finish("C:\\Users\\Arthur".into(), 1, Method::Walk, true, Stats::default())
}

/// Contenu : les copies sont identiques deux à deux.
struct Same;

impl duplicates::ContentReader for Same {
    fn edges(&self, path: &str, _: &AtomicBool) -> Result<[u8; 32], duplicates::ReadError> {
        self.full(path, &AtomicBool::new(false))
    }

    fn full(&self, path: &str, _: &AtomicBool) -> Result<[u8; 32], duplicates::ReadError> {
        let lower = path.to_lowercase();
        let tag = if lower.contains("film") { 1 } else if lower.contains("partie") { 2 } else if lower.contains("save 12") { 3 } else { 4 };
        Ok([tag; 32])
    }
}

fn names(scan: &Scan, ids: impl IntoIterator<Item = NodeId>) -> Vec<String> {
    ids.into_iter().map(|id| scan.name(id).to_owned()).collect()
}

fn is_save(name: &str) -> bool {
    let n = name.to_lowercase();
    n.ends_with(".sav") || n.ends_with(".lsv") || n.starts_with("save 12") || n == "hades.dat"
}

#[test]
fn sav_en_double_ou_dormant_jamais_propose() {
    let scan = profile();
    let deny = deny();

    let dup = duplicates::find(
        &duplicates::Inputs { scan: &scan, deny: Some(&deny), min_size: MB, reader: &Same, cache: &HashMap::new(), cancel: &AtomicBool::new(false) },
        |_| {},
    )
    .unwrap();
    let members: Vec<String> = dup.groups.iter().flat_map(|g| names(&scan, g.members.iter().copied())).collect();
    assert_eq!(dup.groups.len(), 1, "seuls les films sont des doublons proposés : {members:?}");
    assert!(members.iter().all(|n| n.starts_with("film")), "{members:?}");

    let dormant = dormant::find(&dormant::Inputs {
        scan: &scan,
        deny: Some(&deny),
        now: NOW,
        months: 6,
        last_access_updated: true,
        last_access_raw: Some(0x8000_0002),
        covered: &|_| false,
        display: &|p| p.to_owned(),
    });
    let dormants = names(&scan, dormant.eligible.iter().copied());
    assert!(dormants.contains(&"film.mp4".to_owned()), "{dormants:?}");
    assert!(!dormants.iter().any(|n| is_save(n)), "{dormants:?}");

    let large = largest::find(&scan, Some(&deny), None, &|p| p.to_owned());
    let big = names(&scan, large.eligible.iter().copied());
    assert!(big.contains(&"film.mp4".to_owned()), "{big:?}");
    assert!(!big.iter().any(|n| is_save(n)), "{big:?}");
}
