//! Dossiers d'installation des jeux et des applications (jalon 13) : jamais proposés par
//! Dormants, Doublons ni Gros fichiers, et refusés par `safety` à toute sélection
//! (`DenyReason::AppInstall`). Seuls les modules dédiés y agissent : l'onglet Jeux par le
//! launcher, les règles explicites.
//!
//! Sources : dossiers des jeux détectés chez leurs launchers, `InstallLocation` des clés
//! Uninstall (HKLM et HKCU), et par repli, dans l'analyse, tout dossier qui contient un
//! exécutable et un désinstalleur (`files::Zones`). `safety` ajoute de lui-même tout ce qui est à
//! la racine d'un volume hors du dossier des profils.

use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use safety::path::WinPath;

use crate::games::GamesState;

/// Durée de vie de la liste lue dans le registre.
const TTL: Duration = Duration::from_secs(600);

static UNINSTALL: Mutex<Option<(Instant, Vec<WinPath>)>> = Mutex::new(None);

/// Sous-dossiers du profil qui ne sont jamais le dossier d'une application, même si une entrée
/// Uninstall mal écrite les donne comme `InstallLocation` (le profil entier deviendrait sinon
/// intouchable, ou au contraire un parent de l'installation).
const PROFILE_FOLDERS: &[&[&str]] = &[
    &["AppData"],
    &["AppData", "Local"],
    &["AppData", "Roaming"],
    &["AppData", "LocalLow"],
    &["AppData", "Local", "Programs"],
    &["Documents"],
    &["Downloads"],
    &["Desktop"],
    &["Pictures"],
    &["Videos"],
    &["Music"],
];

/// Un dossier d'installation plausible : pas une racine de volume, pas le profil ni un de ses
/// parents, pas un dossier connu du profil (Documents, AppData…).
pub fn plausible(dir: &WinPath, profile: Option<&WinPath>) -> bool {
    if dir.depth() == 0 {
        return false;
    }
    if let Some(profile) = profile {
        if profile.starts_with(dir) {
            return false;
        }
        if PROFILE_FOLDERS.iter().any(|rel| rel.iter().try_fold(profile.clone(), |acc, c| acc.join(c).ok()).is_some_and(|p| &p == dir)) {
            return false;
        }
    }
    true
}

fn parse(p: &Path) -> Option<WinPath> {
    WinPath::parse(&p.display().to_string()).ok()
}

/// Dossiers d'installation lus dans le registre (gardés 10 min).
fn uninstall_dirs() -> Vec<WinPath> {
    if let Ok(g) = UNINSTALL.lock() {
        if let Some((at, dirs)) = g.as_ref() {
            if at.elapsed() < TTL {
                return dirs.clone();
            }
        }
    }
    let dirs: Vec<WinPath> = crate::games::install_locations().iter().filter_map(|p| parse(p)).collect();
    if let Ok(mut g) = UNINSTALL.lock() {
        *g = Some((Instant::now(), dirs.clone()));
    }
    dirs
}

/// Tous les dossiers d'installation connus : jeux (détectés au besoin, sans mesurer les tailles)
/// et applications ; seulement les plausibles.
pub fn installed(games: &GamesState, profile: Option<&WinPath>) -> Vec<WinPath> {
    let mut out: Vec<WinPath> = Vec::new();
    for d in crate::games::game_dirs_now(games).into_iter().chain(uninstall_dirs()) {
        if plausible(&d, profile) && !out.contains(&d) {
            out.push(d);
        }
    }
    out
}

/// Dossiers d'installation, pour le profil de l'utilisateur courant.
pub fn installed_now(games: &GamesState) -> Vec<WinPath> {
    let profile = crate::scanning::app_info().profile.and_then(|p| WinPath::parse(&p).ok());
    installed(games, profile.as_ref())
}

/// Dossiers à protéger pour une sélection faite dans une analyse : installés, plus ceux trouvés
/// par repli dans l'analyse (exécutable et désinstalleur).
pub fn protected_for(games: &GamesState, scan: &scan::Scan) -> Vec<WinPath> {
    let mut dirs = installed_now(games);
    for d in crate::advanced::files::heuristic_app_dirs(scan) {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dossiers_d_installation_plausibles() {
        let p = |s: &str| WinPath::parse(s).unwrap();
        let me = p("C:\\Users\\Arthur");
        for ok in ["C:\\Riot Games\\League of Legends", "D:\\Jeux\\Genshin Impact", "C:\\Users\\Arthur\\AppData\\Local\\Programs\\Discord"] {
            assert!(plausible(&p(ok), Some(&me)), "{ok}");
        }
        for bad in ["C:\\", "C:\\Users", "C:\\Users\\Arthur", "C:\\Users\\Arthur\\AppData\\Local", "C:\\Users\\Arthur\\Documents", "C:\\Users\\Arthur\\AppData\\Local\\Programs"] {
            assert!(!plausible(&p(bad), Some(&me)), "{bad}");
        }
    }
}
