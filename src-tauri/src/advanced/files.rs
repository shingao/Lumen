//! Socle commun du nettoyage avancé : fichiers candidats de l'analyse, dossiers exclus (applis,
//! système, projets git), type d'un fichier d'après son extension.

use serde::Serialize;

use safety::denylist::DenyList;
use safety::path::{fold, WinPath};
use scan::model::F_HARDLINK_DUP;
use scan::{Kind, NodeId, Scan};

use crate::analysis::epoch_ms;

const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;

/// Dossiers d'applications et du système : jamais proposés comme dormants ni doublons.
const APP_DIRS: &[&str] = &[
    "APPDATA",
    "PROGRAMDATA",
    "PROGRAM FILES",
    "PROGRAM FILES (X86)",
    "WINDOWS",
    "WINDOWSAPPS",
    "SCOOP",
    "STEAMAPPS",
    "EPIC GAMES",
    "NODE_MODULES",
    "SYSTEM VOLUME INFORMATION",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Video,
    Image,
    Audio,
    Archive,
    Disk,
    Installer,
    Document,
    Other,
}

impl FileKind {
    pub fn of(name: &str) -> FileKind {
        use scan::kinds::FileType;
        match FileType::of(name) {
            FileType::Video => FileKind::Video,
            FileType::Image => FileKind::Image,
            FileType::Audio => FileKind::Audio,
            FileType::Archive => FileKind::Archive,
            FileType::Disk => FileKind::Disk,
            FileType::Installer => FileKind::Installer,
            FileType::Document => FileKind::Document,
            FileType::Other => FileKind::Other,
        }
    }

    pub fn parse(raw: &str) -> Option<FileKind> {
        Some(match raw {
            "video" => FileKind::Video,
            "image" => FileKind::Image,
            "audio" => FileKind::Audio,
            "archive" => FileKind::Archive,
            "disk" => FileKind::Disk,
            "installer" => FileKind::Installer,
            "document" => FileKind::Document,
            "other" => FileKind::Other,
            _ => return None,
        })
    }
}

/// Pour chaque nœud : dans un dossier d'appli ou système, ou dans un projet git.
pub struct Zones {
    app: Vec<bool>,
    git: Vec<bool>,
}

/// Désinstalleur d'une application (`unins000.exe`, `Uninstall.exe`, `uninstaller.dat`…).
fn is_uninstaller(name: &str) -> bool {
    name.starts_with("UNINS") || name.contains("UNINSTALL")
}

/// Dossier d'application par repli (aucune détection) : il contient un exécutable et un
/// désinstalleur.
fn has_exe_and_uninstaller(scan: &Scan, id: NodeId) -> bool {
    let (mut exe, mut uninstaller) = (false, false);
    for c in scan.children(id) {
        if scan.node(c).is_some_and(|n| n.kind == Kind::File) {
            let name = fold(scan.name(c));
            exe |= name.ends_with(".EXE");
            uninstaller |= is_uninstaller(&name);
        }
    }
    exe && uninstaller
}

/// Analyse d'un volume entier : sa racine est une racine de volume (`C:\`).
fn volume_scan(scan: &Scan) -> bool {
    WinPath::parse(&scan.root_path).is_ok_and(|p| p.is_volume_root())
}

/// Dossiers d'applications trouvés par repli dans l'analyse (exécutable + désinstalleur), transmis
/// à `safety` avec les dossiers détectés.
pub fn heuristic_app_dirs(scan: &Scan) -> Vec<WinPath> {
    (0..scan.len() as NodeId)
        .filter(|&id| id != scan.root() && scan.node(id).is_some_and(|n| n.kind == Kind::Dir) && has_exe_and_uninstaller(scan, id))
        .filter_map(|id| WinPath::parse(&scan.path(id)).ok())
        .collect()
}

impl Zones {
    /// Un seul passage : les enfants ont toujours un indice supérieur à leur parent.
    pub fn new(scan: &Scan) -> Zones {
        let n = scan.len();
        let mut app = vec![false; n];
        let mut git = vec![false; n];
        let volume = volume_scan(scan);
        for id in 0..n as NodeId {
            let Some(node) = scan.node(id) else { continue };
            let (pa, pg) = if id == scan.root() { (false, false) } else { (app[node.parent as usize], git[node.parent as usize]) };
            let name = fold(scan.name(id));
            // Repli quand rien n'est détecté : à la racine d'un volume hors du dossier des profils
            // (`C:\Riot Games`, `D:\SteamLibrary`…), ou un dossier avec un exécutable et un
            // désinstalleur. Les dossiers détectés (launchers, Uninstall) passent par la denylist.
            let root_level = volume && id != scan.root() && node.parent == scan.root() && node.kind == Kind::Dir && name != "USERS";
            let is_app = id != scan.root()
                && (root_level
                    || (node.kind == Kind::Dir
                        && (APP_DIRS.contains(&name.as_str()) || name.starts_with('.') || name.starts_with('$') || has_exe_and_uninstaller(scan, id))));
            let is_git = node.kind == Kind::Dir && scan.children(id).any(|c| scan.name(c).eq_ignore_ascii_case(".git"));
            app[id as usize] = pa || is_app;
            git[id as usize] = pg || is_git;
        }
        Zones { app, git }
    }

    pub fn in_app(&self, id: NodeId) -> bool {
        self.app.get(id as usize).copied().unwrap_or(true)
    }

    pub fn in_git(&self, id: NodeId) -> bool {
        self.git.get(id as usize).copied().unwrap_or(true)
    }
}

/// Fichier ordinaire, local (pas dans le cloud), compté (pas un second lien physique), ni système
/// ni caché.
pub fn plain_file(scan: &Scan, id: NodeId) -> bool {
    scan.node(id).is_some_and(|n| {
        n.kind == Kind::File && n.flags & F_HARDLINK_DUP == 0 && n.attributes & (FILE_ATTRIBUTE_SYSTEM | FILE_ATTRIBUTE_HIDDEN) == 0
    })
}

/// La denylist de `safety` refuse-t-elle ce chemin, y compris comme dossier d'installation d'un
/// jeu ou d'une application ? (`safety` le revérifie de toute façon au moment de l'action.)
pub fn denied(deny: Option<&DenyList>, scan: &Scan, id: NodeId) -> bool {
    match deny {
        Some(d) => WinPath::parse(&scan.path(id)).map_or(true, |p| d.check(&p).is_some() || d.app_install(&p).is_some()),
        None => false,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRowDto {
    pub id: i64,
    pub name: String,
    /// Chemin affiché (`~\…`).
    pub path: String,
    pub size: u64,
    pub logical: u64,
    /// Millisecondes depuis 1970.
    pub modified: i64,
    pub accessed: i64,
    pub kind: FileKind,
    /// Dans un dossier d'appli : à vérifier avec soin.
    pub app: bool,
}

pub fn row(scan: &Scan, id: NodeId, zones: &Zones, display: &dyn Fn(&str) -> String) -> FileRowDto {
    let n = scan.node(id);
    let name = scan.name(id).to_owned();
    FileRowDto {
        id: i64::from(id),
        kind: FileKind::of(&name),
        name,
        path: display(&scan.path(id)),
        size: n.map_or(0, |n| n.allocated),
        logical: n.map_or(0, |n| n.logical),
        modified: n.map_or(0, |n| epoch_ms(n.modified)),
        accessed: n.map_or(0, |n| epoch_ms(n.accessed)),
        app: zones.in_app(id),
    }
}
