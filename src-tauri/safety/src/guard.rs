//! Garde de processus (logique pure) : certaines actions sont refusées tant qu'un programme est
//! ouvert, par exemple vider les caches de shaders pendant qu'un jeu ou un launcher tourne. La
//! garde fait partie du plan (son empreinte la couvre) et `exec` la vérifie au moment de l'action,
//! avant toute commande et tout élément : un programme ouvert = rien n'est fait.

use crate::path::WinPath;

/// Launchers de jeux (nom de l'exécutable, casse ignorée).
pub const LAUNCHER_PROCESSES: [&str; 11] = [
    "steam.exe",
    "steamwebhelper.exe",
    "EpicGamesLauncher.exe",
    "GalaxyClient.exe",
    "Battle.net.exe",
    "Battle.net Launcher.exe",
    "EADesktop.exe",
    "EALauncher.exe",
    "Origin.exe",
    "UbisoftConnect.exe",
    "upc.exe",
];

/// Processus en cours, tel que Windows le décrit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningProcess {
    /// Nom de l'exécutable (`steam.exe`).
    pub name: String,
    /// Chemin complet, quand il est lisible.
    pub path: Option<WinPath>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessGuard {
    names: Vec<String>,
    dirs: Vec<WinPath>,
}

impl ProcessGuard {
    /// `names` : exécutables interdits ; `dirs` : dossiers dont aucun programme ne doit tourner
    /// (dossiers d'installation des jeux).
    pub fn new(names: &[&str], dirs: Vec<WinPath>) -> ProcessGuard {
        let mut names: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
        names.sort();
        names.dedup();
        ProcessGuard { names, dirs }
    }

    /// Jeux et launchers : les launchers connus, et tout programme lancé depuis un dossier de jeu.
    pub fn games(dirs: Vec<WinPath>) -> ProcessGuard {
        ProcessGuard::new(&LAUNCHER_PROCESSES, dirs)
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn dirs(&self) -> &[WinPath] {
        &self.dirs
    }

    /// Premier processus ouvert qui bloque l'action.
    pub fn blocking<'a>(&self, running: &'a [RunningProcess]) -> Option<&'a RunningProcess> {
        running.iter().find(|p| self.names.contains(&p.name.to_lowercase()) || p.path.as_ref().is_some_and(|path| self.dirs.iter().any(|d| path.starts_with(d))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> WinPath {
        WinPath::parse(s).unwrap()
    }

    #[test]
    fn launcher_ou_jeu_ouvert_bloque() {
        let guard = ProcessGuard::games(vec![p("D:\\SteamLibrary\\steamapps\\common\\Cyberpunk 2077")]);
        let explorer = RunningProcess { name: "explorer.exe".into(), path: Some(p("C:\\Windows\\explorer.exe")) };
        assert_eq!(guard.blocking(std::slice::from_ref(&explorer)), None);
        let steam = RunningProcess { name: "STEAM.EXE".into(), path: None };
        assert_eq!(guard.blocking(&[explorer.clone(), steam.clone()]), Some(&steam));
        // Jeu lancé depuis son dossier, quel que soit le nom de son exécutable.
        let game = RunningProcess { name: "Cyberpunk2077.exe".into(), path: Some(p("d:\\steamlibrary\\steamapps\\common\\cyberpunk 2077\\bin\\x64\\Cyberpunk2077.exe")) };
        assert_eq!(guard.blocking(&[explorer.clone(), game.clone()]), Some(&game));
        // Un dossier voisin au nom proche ne bloque pas.
        let other = RunningProcess { name: "x.exe".into(), path: Some(p("D:\\SteamLibrary\\steamapps\\common\\Cyberpunk 2077 Mods\\x.exe")) };
        assert_eq!(guard.blocking(&[other]), None);
    }
}
