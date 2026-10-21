//! Ouvrir un launcher de jeux (logique pure) : Lumen ne désinstalle ni ne déplace jamais un jeu
//! lui-même, il ouvre l'outil du launcher. Liste fermée : les adresses de Steam
//! (`steam://uninstall/<appid>`, paramètres de Steam pour le stockage), ou l'exécutable d'un
//! launcher connu, installé sous Program Files (non modifiable sans droits d'administrateur).

use crate::path::WinPath;

/// Exécutables de launchers qu'il est permis d'ouvrir (casse ignorée).
pub const LAUNCHER_EXES: [&str; 8] = [
    "EpicGamesLauncher.exe",
    "GalaxyClient.exe",
    "Battle.net Launcher.exe",
    "Battle.net.exe",
    "EADesktop.exe",
    "EALauncher.exe",
    "UbisoftConnect.exe",
    "upc.exe",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LauncherOpen {
    /// Désinstallation d'un jeu par Steam (Steam demande lui-même confirmation).
    SteamUninstall(u32),
    /// Paramètres de Steam (Stockage : déplacer un jeu vers une autre bibliothèque).
    SteamSettings,
    /// Launcher ouvert sur sa page d'accueil (sa bibliothèque permet de désinstaller ou déplacer).
    App(WinPath),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LaunchRefusal {
    #[error("identifiant Steam invalide")]
    BadAppId,
    #[error("exécutable de launcher inconnu")]
    UnknownExe,
    #[error("le launcher n'est pas installé sous Program Files")]
    OutsideProgramFiles,
}

impl LauncherOpen {
    /// Adresse ouverte par le shell (Steam), ou `None` pour un exécutable.
    pub fn uri(&self) -> Option<String> {
        match self {
            LauncherOpen::SteamUninstall(id) => Some(format!("steam://uninstall/{id}")),
            LauncherOpen::SteamSettings => Some("steam://open/settings".into()),
            LauncherOpen::App(_) => None,
        }
    }

    /// Contrôle de la liste fermée ; `program_files` : `%ProgramFiles%` et `%ProgramFiles(x86)%`.
    pub fn validate(&self, program_files: &[WinPath]) -> Result<(), LaunchRefusal> {
        match self {
            LauncherOpen::SteamUninstall(0) => Err(LaunchRefusal::BadAppId),
            LauncherOpen::SteamUninstall(_) | LauncherOpen::SteamSettings => Ok(()),
            LauncherOpen::App(exe) => {
                let name = exe.file_name().unwrap_or_default();
                if !LAUNCHER_EXES.iter().any(|e| e.eq_ignore_ascii_case(name)) {
                    return Err(LaunchRefusal::UnknownExe);
                }
                if !program_files.iter().any(|pf| exe.is_strictly_under(pf)) {
                    return Err(LaunchRefusal::OutsideProgramFiles);
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> WinPath {
        WinPath::parse(s).unwrap()
    }

    #[test]
    fn liste_fermee_des_launchers() {
        let pf = [p("C:\\Program Files"), p("C:\\Program Files (x86)")];
        assert_eq!(LauncherOpen::SteamUninstall(570).uri().as_deref(), Some("steam://uninstall/570"));
        assert_eq!(LauncherOpen::SteamUninstall(0).validate(&pf), Err(LaunchRefusal::BadAppId));
        assert_eq!(LauncherOpen::SteamSettings.validate(&pf), Ok(()));
        let epic = LauncherOpen::App(p("C:\\Program Files (x86)\\Epic Games\\Launcher\\Portal\\Binaries\\Win64\\EpicGamesLauncher.exe"));
        assert_eq!((epic.validate(&pf), epic.uri()), (Ok(()), None));
        // Hors Program Files (modifiable par l'utilisateur) ou exécutable inconnu : refus.
        assert_eq!(LauncherOpen::App(p("C:\\Users\\Arthur\\Downloads\\EpicGamesLauncher.exe")).validate(&pf), Err(LaunchRefusal::OutsideProgramFiles));
        assert_eq!(LauncherOpen::App(p("C:\\Program Files\\Epic Games\\cmd.exe")).validate(&pf), Err(LaunchRefusal::UnknownExe));
        assert_eq!(LauncherOpen::App(p("C:\\Program Files (x86)Evil\\upc.exe")).validate(&pf), Err(LaunchRefusal::OutsideProgramFiles));
    }
}
