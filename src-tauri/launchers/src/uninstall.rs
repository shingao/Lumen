//! GOG, Ubisoft Connect, Battle.net, EA app et Riot : leurs clés de registre quand elles existent
//! (`GOG.com\Games`, `Ubisoft\Launcher\Installs`), puis les clés Uninstall de Windows en repli
//! (éditeur ou nom de clé). Un jeu déjà trouvé ailleurs (même dossier) n'est jamais compté deux
//! fois ; les jeux Steam (« Steam App … ») restent à Steam.

use crate::{win_path, Detection, Game, Hive, Launcher, Reason, Registry};

const UNINSTALL: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";

/// Entrée de « Programmes et fonctionnalités ».
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub name: String,
    pub publisher: String,
    pub location: Option<String>,
    /// `EstimatedSize`, en octets.
    pub size: Option<u64>,
}

pub fn entries(reg: &dyn Registry) -> Vec<Entry> {
    let mut out = Vec::new();
    for hive in [Hive::LocalMachine, Hive::CurrentUser] {
        for key in reg.subkeys(hive, UNINSTALL) {
            let path = format!("{UNINSTALL}\\{key}");
            let Some(name) = reg.string(hive, &path, "DisplayName").map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()) else { continue };
            out.push(Entry {
                publisher: reg.string(hive, &path, "Publisher").unwrap_or_default().trim().to_owned(),
                location: reg.string(hive, &path, "InstallLocation").map(|s| s.trim().trim_matches('"').to_owned()).filter(|s| !s.is_empty()),
                size: reg.dword(hive, &path, "EstimatedSize").map(|kb| u64::from(kb) * 1024).filter(|s| *s > 0),
                key,
                name,
            });
        }
    }
    out
}

fn push(d: &mut Detection, launcher: Launcher, key: &str, name: &str, dir: Option<&str>, size: Option<u64>) {
    let dir = dir.map(win_path);
    if dir.as_deref().is_some_and(|p| d.has_dir(p)) || d.games.iter().any(|g| g.launcher == launcher && g.key.eq_ignore_ascii_case(key)) {
        return;
    }
    d.games.push(Game { launcher, key: key.to_owned(), name: name.to_owned(), install_dir: dir, size, last_played: None, cover: None, shader_cache: None });
}

/// Launcher d'une entrée Uninstall qui est un jeu (pas le launcher lui-même), d'après son
/// éditeur ou le nom de sa clé.
fn launcher_of(e: &Entry) -> Option<Launcher> {
    if e.key.to_ascii_lowercase().starts_with("steam app ") {
        return None;
    }
    let p = e.publisher.to_ascii_lowercase();
    let n = e.name.to_ascii_lowercase();
    if e.key.to_ascii_lowercase().starts_with("uplay install ") {
        Some(Launcher::Ubisoft)
    } else if p == "gog.com" && e.key.to_ascii_lowercase().ends_with("_is1") {
        Some(Launcher::Gog)
    } else if p.starts_with("blizzard entertainment") && n != "battle.net" {
        Some(Launcher::BattleNet)
    } else if p.starts_with("electronic arts") && !["ea app", "ea", "origin", "ea desktop"].contains(&n.as_str()) {
        Some(Launcher::Ea)
    } else if (e.key.to_ascii_lowercase().starts_with("riot game ") || p.starts_with("riot games")) && !n.starts_with("riot client") && !n.starts_with("riot vanguard") {
        Some(Launcher::Riot)
    } else {
        None
    }
}

pub fn detect(reg: &dyn Registry, d: &mut Detection) {
    // GOG : clés de GOG Galaxy (vue 32 bits fusionnée par l'appli).
    let gog = "SOFTWARE\\GOG.com\\Games";
    for id in reg.subkeys(Hive::LocalMachine, gog) {
        let path = format!("{gog}\\{id}");
        d.found(Launcher::Gog);
        match (reg.string(Hive::LocalMachine, &path, "gameName"), reg.string(Hive::LocalMachine, &path, "path")) {
            (Some(name), Some(dir)) if !name.trim().is_empty() && !dir.trim().is_empty() => push(d, Launcher::Gog, &id, name.trim(), Some(&dir), None),
            _ => d.ignore(Launcher::Gog, std::path::Path::new(&format!("HKLM\\{path}")), Reason::Incomplete),
        }
    }
    // Ubisoft Connect : un dossier par identifiant, le nom vient de l'entrée Uninstall.
    let ubi = "SOFTWARE\\Ubisoft\\Launcher\\Installs";
    let all = entries(reg);
    if reg.string(Hive::LocalMachine, "SOFTWARE\\Ubisoft\\Launcher", "InstallDir").is_some() {
        d.found(Launcher::Ubisoft);
    }
    for id in reg.subkeys(Hive::LocalMachine, ubi) {
        d.found(Launcher::Ubisoft);
        let Some(dir) = reg.string(Hive::LocalMachine, &format!("{ubi}\\{id}"), "InstallDir").filter(|s| !s.trim().is_empty()) else {
            d.ignore(Launcher::Ubisoft, std::path::Path::new(&format!("HKLM\\{ubi}\\{id}")), Reason::Incomplete);
            continue;
        };
        let entry = all.iter().find(|e| e.key.eq_ignore_ascii_case(&format!("Uplay Install {id}")));
        let folder = win_path(&dir).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| id.clone());
        let name = entry.map(|e| e.name.clone()).unwrap_or(folder);
        push(d, Launcher::Ubisoft, &format!("Uplay Install {id}"), &name, Some(&dir), entry.and_then(|e| e.size));
    }
    // Repli : entrées Uninstall de ces launchers.
    for e in &all {
        let n = e.name.to_ascii_lowercase();
        if n == "battle.net" {
            d.found(Launcher::BattleNet);
        } else if n == "ea app" || n == "origin" {
            d.found(Launcher::Ea);
        } else if n.starts_with("gog galaxy") {
            d.found(Launcher::Gog);
        } else if n.starts_with("ubisoft connect") || n.starts_with("uplay") {
            d.found(Launcher::Ubisoft);
        } else if n.starts_with("riot client") {
            d.found(Launcher::Riot);
        }
        if let Some(l) = launcher_of(e) {
            d.found(l);
            push(d, l, &e.key, &e.name, e.location.as_deref(), e.size);
        }
    }
}
