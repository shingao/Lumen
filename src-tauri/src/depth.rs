//! Analyse en profondeur (jalon 12) : où passent les Go « non analysés » d'un volume, hors du
//! dossier de l'utilisateur, en LECTURE SEULE. Elle s'appuie sur l'analyse du volume entier
//! (parcours sans droits, ou MFT si l'utilisateur accepte l'assistant élevé) ; ce module ne fait
//! que classer ses dossiers en postes et préparer une treemap où la denylist est « Protégée ».
//! Aucune action de suppression : chaque poste qui a un module renvoie vers lui, et les
//! applications se désinstallent depuis les Paramètres de Windows (adresse figée de `safety`).

use serde::Serialize;
use tauri::State;

use crate::error::LumenError;
use crate::scanning::{self, Entry, ScanState, TreeNodeDto, TreeSliceDto};
use scan::{Kind, NodeId, Scan};

/// Éléments listés au plus sous un poste (applications, profils, dossiers à la racine).
const MAX_ENTRIES: usize = 40;

/// Élément d'un poste : une application, un profil (taille totale seulement), un fichier système…
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DepthEntryDto {
    pub name: String,
    pub path: String,
    pub size: u64,
    /// Nœud de l'analyse (zoom dans la treemap) ; absent pour un profil d'un autre compte, dont
    /// le contenu n'est jamais détaillé.
    pub node: Option<i64>,
}

/// Poste : `windows`, `programFiles`, `programData`, `systemFiles`, `restore`, `profiles`,
/// `storeApps`, `rootApps`, `userProfile`, `other`, `inaccessible`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DepthItemDto {
    pub id: &'static str,
    pub size: u64,
    pub files: u64,
    pub entries: Vec<DepthEntryDto>,
    /// Module qui s'en occupe déjà : `sinks` (Gouffres système), `games` (Jeux), `dev` (Dev),
    /// `uninstall` (Paramètres de Windows › Applications installées), `analysis` (Analyse).
    pub link: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DepthReportDto {
    pub scan_id: u32,
    pub volume: String,
    /// `walk` (parcours sans droits : partiel) ou `mft` (complet).
    pub method: &'static str,
    /// Espace occupé sur le volume (total − libre), si le volume est connu.
    pub used: Option<u64>,
    /// Ce que l'analyse a pu mesurer.
    pub measured: u64,
    /// Dossiers refusés pendant le parcours (comptés, jamais bloquants).
    pub access_denied: u64,
    pub items: Vec<DepthItemDto>,
    pub finished_at: i64,
}

fn eq(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Taille comptée d'un nœud (liens physiques une seule fois : la deuxième occurrence ne compte
/// pas, ce qui évite de compter WinSxS deux fois avec System32).
fn size(scan: &Scan, n: NodeId) -> u64 {
    scan.node(n).map_or(0, |x| x.allocated)
}

fn files(scan: &Scan, n: NodeId) -> u64 {
    scan.node(n).map_or(0, |x| u64::from(x.files))
}

fn entry(scan: &Scan, n: NodeId) -> DepthEntryDto {
    DepthEntryDto { name: scan.name(n).to_owned(), path: scan.path(n), size: size(scan, n), node: Some(i64::from(n)) }
}

fn child(scan: &Scan, dir: NodeId, name: &str) -> Option<NodeId> {
    scan.children(dir).find(|&c| eq(scan.name(c), name))
}

#[derive(Default)]
struct Acc {
    size: u64,
    files: u64,
    entries: Vec<DepthEntryDto>,
}

impl Acc {
    fn add(&mut self, scan: &Scan, n: NodeId, listed: Option<DepthEntryDto>) {
        self.size += size(scan, n);
        self.files += files(scan, n);
        if let Some(e) = listed {
            self.entries.push(e);
        }
    }

    fn item(mut self, id: &'static str, link: Option<&'static str>) -> DepthItemDto {
        self.entries.sort_by_key(|e| std::cmp::Reverse(e.size));
        self.entries.truncate(MAX_ENTRIES);
        DepthItemDto { id, size: self.size, files: self.files, entries: self.entries, link }
    }
}

/// Classe la racine d'une analyse de volume en postes (logique pure). `profile` : nom du dossier
/// de l'utilisateur sous `Users` ; `used` : espace occupé du volume (pour « Inaccessible »).
pub fn classify(scan: &Scan, profile: Option<&str>, used: Option<u64>) -> Vec<DepthItemDto> {
    let root = scan.root();
    let mut windows = Acc::default();
    let mut program_files = Acc::default();
    let mut program_data = Acc::default();
    let mut system_files = Acc::default();
    let mut restore = Acc::default();
    let mut profiles = Acc::default();
    let mut store = Acc::default();
    let mut root_apps = Acc::default();
    let mut user = Acc::default();
    let mut other = Acc::default();
    let mut docker = false;

    for c in scan.children(root) {
        let name = scan.name(c);
        let is_dir = scan.node(c).is_some_and(|n| n.kind == Kind::Dir);
        if eq(name, "Windows") && is_dir {
            // WinSxS, System32 (sans ses pilotes), pilotes (DriverStore), le reste.
            let sys32 = child(scan, c, "System32");
            let drivers = sys32.and_then(|s| child(scan, s, "DriverStore"));
            let mut rest = size(scan, c);
            if let Some(w) = child(scan, c, "WinSxS") {
                windows.entries.push(entry(scan, w));
                rest = rest.saturating_sub(size(scan, w));
            }
            if let Some(s) = sys32 {
                let mut e = entry(scan, s);
                e.size = e.size.saturating_sub(drivers.map_or(0, |d| size(scan, d)));
                rest = rest.saturating_sub(size(scan, s));
                windows.entries.push(e);
            }
            if let Some(d) = drivers {
                windows.entries.push(entry(scan, d));
            }
            windows.entries.push(DepthEntryDto { name: String::new(), path: scan.path(c), size: rest, node: Some(i64::from(c)) });
            windows.size += size(scan, c);
            windows.files += files(scan, c);
        } else if (eq(name, "Program Files") || eq(name, "Program Files (x86)")) && is_dir {
            // Applications par taille ; WindowsApps (Store) a son propre poste.
            for app in scan.children(c) {
                if eq(scan.name(app), "WindowsApps") {
                    store.add(scan, app, Some(entry(scan, app)));
                } else {
                    program_files.add(scan, app, Some(entry(scan, app)));
                }
            }
        } else if eq(name, "ProgramData") && is_dir {
            docker = child(scan, c, "DockerDesktop").is_some() || child(scan, c, "Docker").is_some();
            let top: Vec<DepthEntryDto> = scan.children(c).map(|x| entry(scan, x)).collect();
            program_data.add(scan, c, None);
            program_data.entries = top;
        } else if !is_dir && ["pagefile.sys", "hiberfil.sys", "swapfile.sys"].iter().any(|f| eq(name, f)) {
            system_files.add(scan, c, Some(entry(scan, c)));
        } else if eq(name, "System Volume Information") {
            restore.add(scan, c, None);
        } else if eq(name, "Users") && is_dir {
            for p in scan.children(c) {
                let mut e = entry(scan, p);
                if profile.is_some_and(|me| eq(scan.name(p), me)) {
                    user.add(scan, p, Some(e));
                } else {
                    // Autre compte : taille totale seulement, jamais son contenu.
                    e.node = None;
                    profiles.add(scan, p, Some(e));
                }
            }
        } else if eq(name, "XboxGames") && is_dir {
            store.add(scan, c, Some(entry(scan, c)));
        } else if is_dir && !name.starts_with('$') {
            // Jeux et applications installés à la racine (C:\Riot Games, bibliothèques Steam…).
            root_apps.add(scan, c, Some(entry(scan, c)));
        } else {
            other.add(scan, c, Some(entry(scan, c)));
        }
    }
    let measured = size(scan, root);
    let mut items = vec![
        windows.item("windows", Some("sinks")),
        program_files.item("programFiles", Some("uninstall")),
        program_data.item("programData", docker.then_some("dev")),
        system_files.item("systemFiles", Some("sinks")),
        restore.item("restore", Some("sinks")),
        profiles.item("profiles", None),
        store.item("storeApps", Some("games")),
        root_apps.item("rootApps", Some("games")),
        user.item("userProfile", Some("analysis")),
        other.item("other", None),
    ];
    if let Some(used) = used {
        // Ce que même l'analyse n'a pas pu lire : dossiers refusés, métadonnées du volume…
        items.push(DepthItemDto { id: "inaccessible", size: used.saturating_sub(measured), files: 0, entries: Vec::new(), link: None });
    }
    items.retain(|i| i.size > 0 || i.id == "inaccessible" || i.id == "restore");
    items.sort_by_key(|i| std::cmp::Reverse(i.size));
    items
}

fn profile_name() -> Option<String> {
    let p = scanning::app_info().profile?;
    p.trim_end_matches('\\').rsplit('\\').next().map(str::to_owned)
}

fn volume_entry(state: &ScanState, scan_id: u32) -> Result<std::sync::Arc<Entry>, LumenError> {
    let e = scanning::entry_of(state, scan_id)?;
    if !matches!(e.target, scanning::ScanTarget::Volume { .. }) {
        return Err(LumenError::Depth("elle porte sur un volume entier".into()));
    }
    Ok(e)
}

#[tauri::command]
pub fn depth_report(state: State<'_, ScanState>, scan_id: u32) -> Result<DepthReportDto, LumenError> {
    let e = volume_entry(&state, scan_id)?;
    let used = scanning::volumes_list().into_iter().find(|v| eq(&v.letter, &e.volume)).map(|v| v.total.saturating_sub(v.free));
    let profile = profile_name();
    Ok(DepthReportDto {
        scan_id,
        volume: e.volume.clone(),
        method: match e.scan.method {
            scan::Method::Walk => "walk",
            scan::Method::Mft => "mft",
        },
        used,
        measured: size(&e.scan, e.scan.root()),
        access_denied: e.scan.stats.access_denied,
        items: classify(&e.scan, profile.as_deref(), used),
        finished_at: e.finished_at,
    })
}

/// Profil d'un autre compte qui contient `id` (ou `id` lui-même) : son dossier `Users\<nom>`.
fn other_profile(scan: &Scan, id: NodeId, profile: Option<&str>) -> Option<NodeId> {
    let users = child(scan, scan.root(), "Users")?;
    let mut cur = id;
    while cur != scan.root() {
        let parent = scan.node(cur)?.parent;
        if parent == users {
            return (!profile.is_some_and(|me| eq(scan.name(cur), me))).then_some(cur);
        }
        cur = parent;
    }
    None
}

/// Marque la denylist « Protégée » et referme les profils des autres comptes (taille seulement).
fn finish(n: &mut TreeNodeDto, protected: &dyn Fn(&str) -> bool, users_dir: &str, profile: Option<&str>) {
    n.protected = protected(&n.path);
    let name = n.path.get(..users_dir.len()).filter(|h| h.eq_ignore_ascii_case(users_dir)).and_then(|_| n.path.get(users_dir.len()..)).and_then(|r| r.strip_prefix('\\'));
    if let Some(name) = name.filter(|_| n.id >= 0) {
        if !name.is_empty() && !name.contains('\\') && !profile.is_some_and(|me| eq(name, me)) {
            n.children.clear();
            n.has_children = false;
            return;
        }
    }
    for c in &mut n.children {
        finish(c, protected, users_dir, profile);
    }
}

#[tauri::command]
pub fn depth_tree(state: State<'_, ScanState>, scan_id: u32, node: Option<i64>) -> Result<TreeSliceDto, LumenError> {
    let e = volume_entry(&state, scan_id)?;
    let s = &e.scan;
    let profile = profile_name();
    let mut id = match node {
        None => s.root(),
        Some(n) => u32::try_from(n).ok().filter(|&n| (n as usize) < s.len()).ok_or(LumenError::UnknownNode)?,
    };
    // Jamais dans le contenu d'un autre profil : la vue s'ouvre sur `Users`.
    if let Some(p) = other_profile(s, id, profile.as_deref()) {
        id = s.node(p).map_or(s.root(), |n| n.parent);
    }
    let mut dto = scanning::tree_dto(&e, id)?;
    let users_dir = format!("{}\\Users", s.root_path.trim_end_matches('\\'));
    let deny = platform::deny();
    let protected = |path: &str| deny.as_ref().is_some_and(|d| safety::path::WinPath::parse(path).is_ok_and(|p| d.check(&p).is_some()));
    finish(&mut dto.root, &protected, &users_dir, profile.as_deref());
    // Hachures réservées au « sans risque » de l'Analyse : ici, rien n'est à cocher.
    Ok(dto)
}

/// Paramètres de Windows › Applications installées : l'utilisateur y désinstalle lui-même.
#[tauri::command]
pub fn open_apps_settings() -> Result<(), LumenError> {
    platform::open_apps_settings()
}

mod platform {
    use super::*;

    #[cfg(windows)]
    pub fn deny() -> Option<safety::denylist::DenyList> {
        safety::win::system_layout().ok().map(|l| safety::denylist::DenyList::new(&l))
    }

    #[cfg(not(windows))]
    pub fn deny() -> Option<safety::denylist::DenyList> {
        None
    }

    #[cfg(windows)]
    pub fn open_apps_settings() -> Result<(), LumenError> {
        safety::win::open_launcher(&safety::launch::LauncherOpen::AppsSettings).map_err(|e| LumenError::Integration(e.to_string()))
    }

    #[cfg(not(windows))]
    pub fn open_apps_settings() -> Result<(), LumenError> {
        Err(LumenError::Integration("disponible sous Windows uniquement".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scan::model::Builder;
    use scan::{Method, RawEntry, Stats};

    fn e(name: &str, kind: Kind, allocated: u64, file_id: u128) -> RawEntry {
        RawEntry { name: name.into(), kind, attributes: 0, file_id, logical: allocated, allocated, modified: 0, accessed: 0 }
    }

    /// Volume C: : Windows (WinSxS, System32 et ses pilotes), Program Files (une appli et
    /// WindowsApps), ProgramData (Docker), pagefile, Users (moi, un autre compte, Public),
    /// XboxGames, C:\Riot Games, la Corbeille.
    fn volume() -> Scan {
        let mut b = Builder::new(&e("C:", Kind::Dir, 0, 1));
        let r = b.add_children(
            0,
            &[
                e("Windows", Kind::Dir, 0, 2),
                e("Program Files", Kind::Dir, 0, 3),
                e("ProgramData", Kind::Dir, 0, 4),
                e("pagefile.sys", Kind::File, 8000, 5),
                e("Users", Kind::Dir, 0, 6),
                e("XboxGames", Kind::Dir, 0, 7),
                e("Riot Games", Kind::Dir, 0, 8),
                e("$Recycle.Bin", Kind::Dir, 0, 9),
                e("System Volume Information", Kind::Dir, 0, 10),
            ],
        );
        let win = b.add_children(r.start, &[e("WinSxS", Kind::Dir, 0, 20), e("System32", Kind::Dir, 0, 21), e("explorer.exe", Kind::File, 50, 22)]);
        b.add_children(win.start, &[e("amd64_x.dll", Kind::File, 3000, 30)]);
        let s32 = b.add_children(win.start + 1, &[e("DriverStore", Kind::Dir, 0, 31), e("kernel32.dll", Kind::File, 900, 32)]);
        b.add_children(s32.start, &[e("nv.inf", Kind::File, 600, 33)]);
        let pf = b.add_children(r.start + 1, &[e("Adobe", Kind::Dir, 0, 40), e("WindowsApps", Kind::Dir, 0, 41)]);
        b.add_children(pf.start, &[e("ps.exe", Kind::File, 2500, 42)]);
        b.add_children(pf.start + 1, &[e("pkg", Kind::File, 1200, 43)]);
        let pd = b.add_children(r.start + 2, &[e("DockerDesktop", Kind::Dir, 0, 50)]);
        b.add_children(pd.start, &[e("vm.vhdx", Kind::File, 4000, 51)]);
        let users = b.add_children(r.start + 4, &[e("Arthur", Kind::Dir, 0, 60), e("Bob", Kind::Dir, 0, 61), e("Public", Kind::Dir, 0, 62)]);
        b.add_children(users.start, &[e("photo.jpg", Kind::File, 700, 63)]);
        b.add_children(users.start + 1, &[e("secret.docx", Kind::File, 300, 64)]);
        b.add_children(r.start + 5, &[e("Halo", Kind::File, 1500, 70)]);
        b.add_children(r.start + 6, &[e("League of Legends", Kind::File, 2200, 80)]);
        b.add_children(r.start + 7, &[e("old.zip", Kind::File, 100, 90)]);
        b.finish("C:\\".into(), 1, Method::Walk, false, Stats::default())
    }

    fn item<'a>(items: &'a [DepthItemDto], id: &str) -> &'a DepthItemDto {
        items.iter().find(|i| i.id == id).unwrap_or_else(|| panic!("poste {id} absent : {items:?}"))
    }

    #[test]
    fn postes_du_volume_en_lecture_seule() {
        let scan = volume();
        let items = classify(&scan, Some("Arthur"), Some(30_000));
        let windows = item(&items, "windows");
        assert_eq!(windows.size, 4550);
        let names: Vec<(&str, u64)> = windows.entries.iter().map(|e| (e.name.as_str(), e.size)).collect();
        assert_eq!(names, [("WinSxS", 3000), ("System32", 900), ("DriverStore", 600), ("", 50)]);
        let apps = item(&items, "programFiles");
        assert_eq!((apps.size, apps.entries.len(), apps.entries[0].name.as_str(), apps.link), (2500, 1, "Adobe", Some("uninstall")));
        // WindowsApps (sous Program Files) et XboxGames : un poste, compté une fois.
        assert_eq!(item(&items, "storeApps").size, 2700);
        assert_eq!(item(&items, "programData").link, Some("dev"), "Docker : renvoi vers l'onglet Dev");
        assert_eq!((item(&items, "systemFiles").size, item(&items, "systemFiles").link), (8000, Some("sinks")));
        // Autres comptes : taille totale seulement, jamais de nœud à ouvrir.
        let profiles = item(&items, "profiles");
        assert_eq!(profiles.size, 300);
        assert!(profiles.entries.iter().all(|e| e.node.is_none()));
        assert_eq!(item(&items, "userProfile").size, 700);
        assert_eq!((item(&items, "rootApps").entries[0].name.as_str(), item(&items, "rootApps").link), ("Riot Games", Some("games")));
        assert_eq!(item(&items, "other").entries[0].name, "$Recycle.Bin");
        // Ce que l'analyse n'a pas pu lire : occupé − mesuré.
        let measured = size(&scan, scan.root());
        assert_eq!(item(&items, "inaccessible").size, 30_000 - measured);
        // Tout est compté une seule fois.
        let total: u64 = items.iter().filter(|i| i.id != "inaccessible").map(|i| i.size).sum();
        assert_eq!(total, measured);
        // Sans volume connu : pas de poste « Inaccessible ».
        assert!(!classify(&scan, Some("Arthur"), None).iter().any(|i| i.id == "inaccessible"));
    }

    #[test]
    fn profil_d_un_autre_compte_jamais_detaille() {
        let scan = volume();
        let users = scan.find("Users").unwrap();
        let bob = scan.find("Users\\Bob").unwrap();
        let secret = scan.find("Users\\Bob\\secret.docx").unwrap();
        let me = scan.find("Users\\Arthur\\photo.jpg").unwrap();
        assert_eq!(other_profile(&scan, secret, Some("Arthur")), Some(bob));
        assert_eq!(other_profile(&scan, bob, Some("Arthur")), Some(bob));
        assert_eq!(other_profile(&scan, me, Some("Arthur")), None);
        assert_eq!(other_profile(&scan, users, Some("Arthur")), None);

        let entry = Entry::for_tests(scanning::ScanTarget::Volume { letter: "C:".into() }, volume());
        let mut dto = scanning::tree_dto(&entry, users).unwrap();
        finish(&mut dto.root, &|p: &str| p.eq_ignore_ascii_case("C:\\Users\\Public"), "C:\\Users", Some("Arthur"));
        let bob_tile = dto.root.children.iter().find(|c| c.name == "Bob").unwrap();
        assert!(bob_tile.children.is_empty() && !bob_tile.has_children);
        let arthur = dto.root.children.iter().find(|c| c.name == "Arthur").unwrap();
        assert!(arthur.has_children);
        assert!(dto.root.children.iter().find(|c| c.name == "Public").unwrap().protected);
        assert!(!arthur.protected);
    }
}
