//! Ouvrir un launcher de jeux (logique pure) : Lumen ne désinstalle ni ne déplace jamais un jeu
//! lui-même, il ouvre l'outil du launcher. Liste fermée : les adresses de Steam
//! (`steam://uninstall/<appid>`, paramètres de Steam pour le stockage), deux applis du Store à
//! l'identifiant figé (Xbox, Minecraft Launcher), l'exécutable d'un launcher connu installé sous
//! Program Files (non modifiable sans droits d'administrateur), ou celui d'un launcher installé
//! pour l'utilisateur seul (Riot Client, CurseForge) : modifiable par l'utilisateur, il n'est
//! jamais ouvert quand Lumen tourne en administrateur (il hériterait de ses droits).
//! S'y ajoute une page fixe des Paramètres de Windows (« Applications installées »), d'où
//! l'utilisateur désinstalle lui-même une application (analyse en profondeur, jalon 12).

use crate::path::WinPath;

/// Exécutables de launchers qu'il est permis d'ouvrir (casse ignorée).
pub const LAUNCHER_EXES: [&str; 9] = [
    "EpicGamesLauncher.exe",
    "GalaxyClient.exe",
    "Battle.net Launcher.exe",
    "Battle.net.exe",
    "EADesktop.exe",
    "EALauncher.exe",
    "UbisoftConnect.exe",
    "upc.exe",
    "MinecraftLauncher.exe",
];

/// Launchers installés pour l'utilisateur seul, hors de Program Files (casse ignorée).
pub const USER_LAUNCHER_EXES: [&str; 2] = ["RiotClientServices.exe", "CurseForge.exe"];

/// Launchers dont l'exécutable a un nom générique : permis seulement dans un dossier de ce nom
/// (dossier, exécutable ; casse ignorée). HoYoPlay : `HoYoPlay\launcher.exe`.
pub const FOLDER_LAUNCHERS: [(&str, &str); 1] = [("HoYoPlay", "launcher.exe")];

fn folder_launcher(exe: &WinPath) -> bool {
    let parent = exe.parent();
    let folder = parent.as_ref().and_then(|p| p.file_name()).unwrap_or_default();
    let name = exe.file_name().unwrap_or_default();
    FOLDER_LAUNCHERS.iter().any(|(d, e)| d.eq_ignore_ascii_case(folder) && e.eq_ignore_ascii_case(name))
}

/// Applis du Microsoft Store ouvertes par leur identifiant (AUMID), figé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreApp {
    /// Appli Xbox : désinstaller ou déplacer un jeu Gaming Services.
    Xbox,
    /// Minecraft Launcher du Store.
    MinecraftLauncher,
}

impl StoreApp {
    pub fn aumid(self) -> &'static str {
        match self {
            StoreApp::Xbox => "Microsoft.GamingApp_8wekyb3d8bbwe!Microsoft.Xbox.App",
            StoreApp::MinecraftLauncher => "Microsoft.4297127D64EC6_8wekyb3d8bbwe!Minecraft",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LauncherOpen {
    /// Désinstallation d'un jeu par Steam (Steam demande lui-même confirmation).
    SteamUninstall(u32),
    /// Paramètres de Steam (Stockage : déplacer un jeu vers une autre bibliothèque).
    SteamSettings,
    /// Launcher ouvert sur sa page d'accueil (sa bibliothèque permet de désinstaller ou déplacer).
    App(WinPath),
    /// Appli du Store (Xbox, Minecraft Launcher).
    Store(StoreApp),
    /// Launcher installé pour l'utilisateur seul (Riot Client, CurseForge), hors Program Files.
    UserApp(WinPath),
    /// Paramètres de Windows › Applications installées (adresse figée) : Lumen ne désinstalle
    /// jamais une application lui-même.
    AppsSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LaunchRefusal {
    #[error("identifiant Steam invalide")]
    BadAppId,
    #[error("exécutable de launcher inconnu")]
    UnknownExe,
    #[error("le launcher n'est pas installé sous Program Files")]
    OutsideProgramFiles,
    #[error("Lumen tourne en administrateur : ouvre ce launcher toi-même (il est installé hors de Program Files)")]
    Elevated,
}

impl LauncherOpen {
    /// Adresse ouverte par le shell (Steam), ou `None` pour un exécutable.
    pub fn uri(&self) -> Option<String> {
        match self {
            LauncherOpen::SteamUninstall(id) => Some(format!("steam://uninstall/{id}")),
            LauncherOpen::SteamSettings => Some("steam://open/settings".into()),
            LauncherOpen::Store(app) => Some(format!("shell:AppsFolder\\{}", app.aumid())),
            LauncherOpen::AppsSettings => Some("ms-settings:appsfeatures".into()),
            LauncherOpen::App(_) | LauncherOpen::UserApp(_) => None,
        }
    }

    /// Contrôle de la liste fermée ; `program_files` : `%ProgramFiles%` et `%ProgramFiles(x86)%` ;
    /// `elevated` : Lumen tourne-t-il en administrateur ?
    pub fn validate(&self, program_files: &[WinPath], elevated: bool) -> Result<(), LaunchRefusal> {
        match self {
            LauncherOpen::SteamUninstall(0) => Err(LaunchRefusal::BadAppId),
            LauncherOpen::SteamUninstall(_) | LauncherOpen::SteamSettings | LauncherOpen::Store(_) | LauncherOpen::AppsSettings => Ok(()),
            LauncherOpen::UserApp(exe) => {
                let name = exe.file_name().unwrap_or_default();
                if !USER_LAUNCHER_EXES.iter().any(|e| e.eq_ignore_ascii_case(name)) && !folder_launcher(exe) {
                    return Err(LaunchRefusal::UnknownExe);
                }
                if !matches!(exe.prefix(), crate::path::Prefix::Drive(_)) {
                    return Err(LaunchRefusal::UnknownExe);
                }
                if elevated {
                    return Err(LaunchRefusal::Elevated);
                }
                Ok(())
            }
            LauncherOpen::App(exe) => {
                let name = exe.file_name().unwrap_or_default();
                if !LAUNCHER_EXES.iter().any(|e| e.eq_ignore_ascii_case(name)) && !folder_launcher(exe) {
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
        assert_eq!(LauncherOpen::SteamUninstall(0).validate(&pf, false), Err(LaunchRefusal::BadAppId));
        assert_eq!(LauncherOpen::SteamSettings.validate(&pf, true), Ok(()));
        // Paramètres de Windows › Applications installées : adresse figée, rien d'autre.
        assert_eq!(LauncherOpen::AppsSettings.uri().as_deref(), Some("ms-settings:appsfeatures"));
        assert_eq!(LauncherOpen::AppsSettings.validate(&pf, true), Ok(()));
        let epic = LauncherOpen::App(p("C:\\Program Files (x86)\\Epic Games\\Launcher\\Portal\\Binaries\\Win64\\EpicGamesLauncher.exe"));
        assert_eq!((epic.validate(&pf, true), epic.uri()), (Ok(()), None));
        // Hors Program Files (modifiable par l'utilisateur) ou exécutable inconnu : refus.
        assert_eq!(LauncherOpen::App(p("C:\\Users\\Arthur\\Downloads\\EpicGamesLauncher.exe")).validate(&pf, false), Err(LaunchRefusal::OutsideProgramFiles));
        assert_eq!(LauncherOpen::App(p("C:\\Program Files\\Epic Games\\cmd.exe")).validate(&pf, false), Err(LaunchRefusal::UnknownExe));
        assert_eq!(LauncherOpen::App(p("C:\\Program Files (x86)Evil\\upc.exe")).validate(&pf, false), Err(LaunchRefusal::OutsideProgramFiles));
        // Un launcher de l'utilisateur ne passe pas par la liste de Program Files, et inversement.
        assert_eq!(LauncherOpen::App(p("C:\\Riot Games\\Riot Client\\RiotClientServices.exe")).validate(&pf, false), Err(LaunchRefusal::UnknownExe));
    }

    #[test]
    fn launchers_de_l_utilisateur_et_applis_du_store() {
        let pf = [p("C:\\Program Files"), p("C:\\Program Files (x86)")];
        // Appli Xbox : identifiant figé, ouvert même en administrateur (le Store ne s'élève pas).
        let xbox = LauncherOpen::Store(StoreApp::Xbox);
        assert_eq!(xbox.uri().as_deref(), Some("shell:AppsFolder\\Microsoft.GamingApp_8wekyb3d8bbwe!Microsoft.Xbox.App"));
        assert_eq!(xbox.validate(&pf, true), Ok(()));
        // Riot Client et CurseForge : hors Program Files, jamais quand Lumen est élevé.
        let riot = LauncherOpen::UserApp(p("C:\\Riot Games\\Riot Client\\RiotClientServices.exe"));
        assert_eq!((riot.validate(&pf, false), riot.uri()), (Ok(()), None));
        assert_eq!(riot.validate(&pf, true), Err(LaunchRefusal::Elevated));
        let cf = LauncherOpen::UserApp(p("C:\\Users\\Arthur\\AppData\\Local\\Programs\\CurseForge Windows\\CurseForge.exe"));
        assert_eq!(cf.validate(&pf, false), Ok(()));
        // Autre exécutable, ou sur un partage réseau : refus.
        assert_eq!(LauncherOpen::UserApp(p("C:\\Riot Games\\Riot Client\\cmd.exe")).validate(&pf, false), Err(LaunchRefusal::UnknownExe));
        assert_eq!(LauncherOpen::UserApp(p("\\\\serveur\\partage\\CurseForge.exe")).validate(&pf, false), Err(LaunchRefusal::UnknownExe));
        // HoYoPlay : `launcher.exe` seulement dans un dossier HoYoPlay (sous Program Files, ou
        // ailleurs pour l'utilisateur, jamais élevé) ; un autre `launcher.exe` est refusé.
        assert_eq!(LauncherOpen::App(p("C:\\Program Files\\HoYoPlay\\launcher.exe")).validate(&pf, true), Ok(()));
        assert_eq!(LauncherOpen::App(p("C:\\Program Files\\Autre\\launcher.exe")).validate(&pf, false), Err(LaunchRefusal::UnknownExe));
        let hoyo = LauncherOpen::UserApp(p("D:\\Jeux\\HoYoPlay\\launcher.exe"));
        assert_eq!((hoyo.validate(&pf, false), hoyo.validate(&pf, true)), (Ok(()), Err(LaunchRefusal::Elevated)));
        assert_eq!(LauncherOpen::UserApp(p("D:\\Jeux\\Piege\\launcher.exe")).validate(&pf, false), Err(LaunchRefusal::UnknownExe));
    }
}
