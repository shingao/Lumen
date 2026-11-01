//! Garde de processus (logique pure) : certaines actions sont refusées tant qu'un programme est
//! ouvert, par exemple vider les caches de shaders pendant qu'un jeu ou un launcher tourne. La
//! garde fait partie du plan (son empreinte la couvre) et `exec` la vérifie au moment de l'action,
//! avant toute commande et tout élément : un programme ouvert = rien n'est fait.

use crate::path::{fold, WinPath};

/// Launchers de jeux (nom de l'exécutable, casse ignorée).
pub const LAUNCHER_PROCESSES: [&str; 17] = [
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
    "RiotClientServices.exe",
    "RiotClientUx.exe",
    "Riot Client.exe",
    "CurseForge.exe",
    "MinecraftLauncher.exe",
    "XboxPcApp.exe",
];

/// Dossiers de cache des applis Electron / Chromium (règle « Caches d'applications ») : ce qui
/// les précède sous AppData désigne l'appli (`AppData\Roaming\discord\Cache\Cache_Data`).
pub const APP_CACHE_DIRS: [&str; 8] = ["Cache", "Code Cache", "GPUCache", "DawnCache", "DawnGraphiteCache", "DawnWebGPUCache", "ShaderCache", "ComputeCache"];

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

    /// Caches d'applications : l'appli à qui appartient chaque cache ne doit pas tourner. Elle se
    /// lit dans le chemin du cache sous l'une des `roots` (AppData\Roaming, AppData\Local) :
    /// `discord`, `Opera Software\Opera Stable`… Sont interdits les exécutables qui portent son
    /// nom (`discord.exe`, `Opera Stable.exe`, `OperaStable.exe`, premier mot `opera.exe`) et tout
    /// programme lancé depuis son dossier sous `roots` ou `programs` (Program Files,
    /// AppData\Local\Programs). Un chemin hors de `roots` n'ajoute rien.
    pub fn app_caches(caches: &[WinPath], roots: &[WinPath], programs: &[WinPath]) -> ProcessGuard {
        let mut names = Vec::new();
        let mut dirs: Vec<WinPath> = Vec::new();
        for cache in caches {
            let Some(root) = roots.iter().find(|r| cache.is_strictly_under(r)) else { continue };
            let owner: Vec<&String> = cache.components()[root.depth()..].iter().take_while(|c| !APP_CACHE_DIRS.iter().any(|d| fold(d) == fold(c))).collect();
            if owner.is_empty() || owner.len() == cache.depth() - root.depth() {
                continue; // Pas un dossier de cache connu : rien à déduire.
            }
            for c in &owner {
                names.push(format!("{c}.exe"));
                names.push(format!("{}.exe", c.replace(' ', "")));
                if let Some(first) = c.split(' ').next().filter(|w| !w.is_empty()) {
                    names.push(format!("{first}.exe"));
                }
            }
            let rel = owner.iter().map(|c| c.as_str()).collect::<Vec<_>>().join("\\");
            for base in roots.iter().chain(programs) {
                if let Ok(d) = base.join(&rel) {
                    if !dirs.contains(&d) {
                        dirs.push(d);
                    }
                }
            }
        }
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        ProcessGuard::new(&names, dirs)
    }

    /// Les deux gardes à la fois.
    pub fn merge(self, other: ProcessGuard) -> ProcessGuard {
        let names: Vec<&str> = self.names.iter().chain(&other.names).map(String::as_str).collect();
        let mut dirs = self.dirs.clone();
        for d in other.dirs {
            if !dirs.contains(&d) {
                dirs.push(d);
            }
        }
        ProcessGuard::new(&names, dirs)
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty() && self.dirs.is_empty()
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

    #[test]
    fn appli_d_un_cache_ouverte_bloque() {
        let roots = [p("C:\\Users\\Arthur\\AppData\\Roaming"), p("C:\\Users\\Arthur\\AppData\\Local")];
        let programs = [p("C:\\Program Files"), p("C:\\Users\\Arthur\\AppData\\Local\\Programs")];
        let caches = [
            p("C:\\Users\\Arthur\\AppData\\Roaming\\discord\\Cache\\Cache_Data\\f_000001"),
            p("C:\\Users\\Arthur\\AppData\\Roaming\\Opera Software\\Opera Stable\\GPUCache"),
            p("C:\\Users\\Arthur\\AppData\\Roaming\\NVIDIA\\ComputeCache\\0\\1"),
            // Hors AppData, ou sans dossier de cache connu : rien n'en est déduit.
            p("D:\\Jeux\\Cache\\x"),
            p("C:\\Users\\Arthur\\AppData\\Roaming\\Autre\\Données"),
        ];
        let guard = ProcessGuard::app_caches(&caches, &roots, &programs);
        let run = |name: &str, path: &str| RunningProcess { name: name.into(), path: WinPath::parse(path).ok() };
        // Discord lancé depuis AppData\Local\Discord (nom et dossier).
        assert!(guard.blocking(&[run("Discord.exe", "C:\\Users\\Arthur\\AppData\\Local\\Discord\\app-1.0.9\\Discord.exe")]).is_some());
        assert!(guard.blocking(&[run("Update.exe", "C:\\Users\\Arthur\\AppData\\Local\\Discord\\Update.exe")]).is_some());
        // Opera : premier mot du dossier de l'éditeur.
        assert!(guard.blocking(&[run("opera.exe", "C:\\Users\\Arthur\\AppData\\Local\\Programs\\Opera\\opera.exe")]).is_some());
        assert!(guard.blocking(&[run("x.exe", "C:\\Program Files\\NVIDIA\\x.exe")]).is_some());
        // Programmes sans rapport : rien ne bloque.
        let other = [run("explorer.exe", "C:\\Windows\\explorer.exe"), run("Code.exe", "C:\\Users\\Arthur\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe"), run("Jeux.exe", "D:\\Jeux\\Jeux.exe"), run("Autre.exe", "C:\\Autre.exe")];
        assert_eq!(guard.blocking(&other), None);
        assert!(!guard.dirs().iter().any(|d| d.display_string().contains("Jeux") || d.display_string().contains("Autre")));
        // Fusion avec la garde des jeux.
        let both = guard.merge(ProcessGuard::games(Vec::new()));
        assert!(both.blocking(&[run("steam.exe", "C:\\Steam\\steam.exe")]).is_some());
        assert!(both.blocking(&[run("discord.exe", "C:\\x\\discord.exe")]).is_some());
        assert!(ProcessGuard::app_caches(&[], &roots, &programs).is_empty());
    }
}
