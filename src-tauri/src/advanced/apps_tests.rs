//! Dossiers d'installation de jeux et d'applications (jalon 13) : jamais proposés par Doublons,
//! Dormants ni Gros fichiers, même anciens, volumineux et en double, qu'ils soient détectés par
//! un launcher (HoYoPlay) ou trouvés par repli (racine d'un volume, exécutable + désinstalleur).

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use safety::denylist::{DenyList, SystemLayout};
use safety::path::WinPath;
use scan::model::Builder;
use scan::{Kind, Method, NodeId, RawEntry, Scan, Stats};

use super::{dormant, duplicates, files, largest};

const DAY: i64 = 864_000_000_000;
const NOW: i64 = 133_700_000_000_000_000;
const MB: u64 = 1 << 20;

fn p(s: &str) -> WinPath {
    WinPath::parse(s).unwrap()
}

/// Denylist de la machine, avec le jeu HoYoPlay détecté (installé sous le profil, là où le repli
/// « racine du volume » ne le couvrirait pas).
fn deny(detected: bool) -> DenyList {
    let base = DenyList::new(&SystemLayout {
        windows: p("C:\\Windows"),
        program_files: vec![p("C:\\Program Files"), p("C:\\Program Files (x86)")],
        program_data: p("C:\\ProgramData"),
        users: p("C:\\Users"),
        profile: p("C:\\Users\\Arthur"),
        public: p("C:\\Users\\Public"),
        other_profiles: Vec::new(),
    });
    if detected {
        base.with_apps([p("C:\\Users\\Arthur\\Jeux\\Genshin Impact")])
    } else {
        base
    }
}

/// Volume C: : League of Legends (`.wad.client` de 900 Mo, ouverts il y a 7 mois, deux identiques),
/// Genshin Impact installé par HoYoPlay sous le profil, un utilitaire avec exécutable et
/// désinstalleur dans Documents, et deux films témoins (vieux, gros, en double).
fn volume() -> Scan {
    let old = NOW - 210 * DAY;
    let mut id = 0u128;
    let mut e = |name: &str, kind: Kind, size: u64| {
        id += 1;
        RawEntry { name: name.into(), kind, attributes: 0, file_id: id, logical: size, allocated: size, modified: old, accessed: old }
    };
    let mut b = Builder::new(&e("C:", Kind::Dir, 0));
    let top = b.add_children(0, &[e("Riot Games", Kind::Dir, 0), e("Users", Kind::Dir, 0)]);
    let lol = b.add_children(top.start, &[e("League of Legends", Kind::Dir, 0)]);
    let game = b.add_children(lol.start, &[e("Game", Kind::Dir, 0), e("LeagueClient.exe", Kind::File, 80 * MB)]);
    let data = b.add_children(game.start, &[e("DATA", Kind::Dir, 0)]);
    let fin = b.add_children(data.start, &[e("FINAL", Kind::Dir, 0)]);
    let champs = b.add_children(fin.start, &[e("Champions", Kind::Dir, 0)]);
    b.add_children(champs.start, &[e("Ahri.wad.client", Kind::File, 900 * MB), e("Ahri.fr_FR.wad.client", Kind::File, 900 * MB)]);
    let me = b.add_children(top.start + 1, &[e("Arthur", Kind::Dir, 0)]);
    let home = b.add_children(me.start, &[e("Jeux", Kind::Dir, 0), e("Videos", Kind::Dir, 0), e("Documents", Kind::Dir, 0)]);
    let jeux = b.add_children(home.start, &[e("Genshin Impact", Kind::Dir, 0)]);
    let gi = b.add_children(jeux.start, &[e("Genshin Impact Game", Kind::Dir, 0)]);
    let gid = b.add_children(gi.start, &[e("GenshinImpact_Data", Kind::Dir, 0), e("GenshinImpact.exe", Kind::File, 120 * MB)]);
    b.add_children(gid.start, &[e("blocks_00.blk", Kind::File, 2000 * MB), e("blocks_01.blk", Kind::File, 2000 * MB)]);
    b.add_children(home.start + 1, &[e("film.mp4", Kind::File, 700 * MB), e("film - copie.mp4", Kind::File, 700 * MB)]);
    let docs = b.add_children(home.start + 2, &[e("OutilPortable", Kind::Dir, 0)]);
    b.add_children(docs.start, &[e("outil.exe", Kind::File, 300 * MB), e("unins000.exe", Kind::File, 3 * MB), e("donnees.pak", Kind::File, 600 * MB)]);
    b.finish("C:\\".into(), 1, Method::Walk, true, Stats::default())
}

/// Contenu : les paires sont identiques (champions, blocs du jeu, films).
struct Same;

impl duplicates::ContentReader for Same {
    fn edges(&self, path: &str, _: &AtomicBool) -> Result<[u8; 32], duplicates::ReadError> {
        self.full(path, &AtomicBool::new(false))
    }

    fn full(&self, path: &str, _: &AtomicBool) -> Result<[u8; 32], duplicates::ReadError> {
        let l = path.to_lowercase();
        let tag = if l.contains("ahri") { 1 } else if l.contains("blocks") { 2 } else if l.contains("film") { 3 } else { 4 };
        Ok([tag; 32])
    }
}

fn names(scan: &Scan, ids: impl IntoIterator<Item = NodeId>) -> Vec<String> {
    ids.into_iter().map(|id| scan.name(id).to_owned()).collect()
}

fn from_app(name: &str) -> bool {
    let n = name.to_lowercase();
    n.ends_with(".wad.client") || n.ends_with(".blk") || n.ends_with(".exe") || n == "donnees.pak"
}

#[test]
fn fichiers_de_jeux_et_d_applis_jamais_proposes() {
    let scan = volume();
    let deny = deny(true);

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
    let d = names(&scan, dormant.eligible.iter().copied());
    assert!(d.contains(&"film.mp4".to_owned()), "témoin proposé : {d:?}");
    assert!(!d.iter().any(|n| from_app(n)), "Dormants : {d:?}");

    let large = largest::find(&scan, Some(&deny), None, &|p| p.to_owned());
    let l = names(&scan, large.eligible.iter().copied());
    assert!(l.contains(&"film.mp4".to_owned()), "{l:?}");
    assert!(!l.iter().any(|n| from_app(n)), "Gros fichiers : {l:?}");

    let dup = duplicates::find(
        &duplicates::Inputs { scan: &scan, deny: Some(&deny), min_size: MB, reader: &Same, cache: &HashMap::new(), cancel: &AtomicBool::new(false) },
        |_| {},
    )
    .unwrap();
    let members: Vec<String> = dup.groups.iter().flat_map(|g| names(&scan, g.members.iter().copied())).collect();
    assert_eq!(dup.groups.len(), 1, "seuls les films : {members:?}");
    assert!(!members.iter().any(|n| from_app(n)), "Doublons : {members:?}");
}

/// Sans denylist (développement hors Windows) ni détection, le repli de l'analyse suffit pour les
/// dossiers à la racine du volume et ceux qui ont un exécutable et un désinstalleur.
#[test]
fn repli_sans_detection() {
    let scan = volume();
    let large = largest::find(&scan, None, None, &|p| p.to_owned());
    let l = names(&scan, large.eligible.iter().copied());
    assert!(!l.iter().any(|n| n.ends_with(".wad.client") || n == "donnees.pak" || n == "outil.exe"), "{l:?}");
    // Un jeu sous le profil sans exécutable de désinstallation : seule la détection le protège.
    assert!(l.iter().any(|n| n.ends_with(".blk")), "{l:?}");
    let with_detection = largest::find(&scan, Some(&deny(true)), None, &|p| p.to_owned());
    assert!(!names(&scan, with_detection.eligible.iter().copied()).iter().any(|n| n.ends_with(".blk")));
    // Dossiers trouvés par repli, transmis à `safety` avec les dossiers détectés.
    let dirs: Vec<String> = files::heuristic_app_dirs(&scan).iter().map(WinPath::display_string).collect();
    assert_eq!(dirs, ["C:\\Users\\Arthur\\Documents\\OutilPortable"]);
    // Sans détection, la denylist de `safety` couvre quand même la racine du volume.
    let riot = deny(false).app_install(&p("C:\\Riot Games\\League of Legends\\Game\\DATA\\FINAL\\Champions\\Ahri.wad.client"));
    assert_eq!(riot, Some(safety::denylist::AppZone::Inside));
}
