//! Steam : dossier de Steam par `HKCU\Software\Valve\Steam` (`SteamPath`, repli sur
//! `InstallPath` de HKLM), bibliothèques par `steamapps\libraryfolders.vdf` (format actuel et
//! ancien), jeux par `appmanifest_*.acf`, dernière partie par `userdata\<id>\config\localconfig.vdf`
//! (et `LastPlayed` des manifestes récents), jaquettes dans `appcache\librarycache`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::vdf::{self, Value};
use crate::{list_files, norm, read_text, win_path, Detection, Game, Hive, Launcher, Reason, Registry};

/// Outils de Steam installés comme des jeux (redistribuables), jamais proposés.
const TOOLS: [&str; 1] = ["228980"];

pub fn steam_root(reg: &dyn Registry) -> Option<PathBuf> {
    reg.string(Hive::CurrentUser, "Software\\Valve\\Steam", "SteamPath")
        .or_else(|| reg.string(Hive::LocalMachine, "SOFTWARE\\Valve\\Steam", "InstallPath"))
        .filter(|s| !s.trim().is_empty())
        .map(|s| win_path(&s))
}

fn parse_file(path: &Path, d: &mut Detection) -> Option<Value> {
    match read_text(path) {
        Err(r) => {
            d.ignore(Launcher::Steam, path, r);
            None
        }
        Ok(text) => match vdf::parse(&text) {
            Ok(v) => Some(v),
            Err(_) => {
                d.ignore(Launcher::Steam, path, Reason::Malformed);
                None
            }
        },
    }
}

/// Bibliothèques : le dossier de Steam, puis celles de `libraryfolders.vdf`, sans doublon.
pub fn libraries(root: &Path, d: &mut Detection) -> Vec<PathBuf> {
    let mut out = vec![root.to_path_buf()];
    let file = root.join("steamapps").join("libraryfolders.vdf");
    if !file.exists() {
        return out;
    }
    let Some(doc) = parse_file(&file, d) else { return out };
    let block = doc.get("libraryfolders").or_else(|| doc.get("LibraryFolders"));
    for (key, value) in block.map(Value::entries).unwrap_or_default() {
        // Format actuel : "0" { "path" "…" } ; ancien : "1" "D:\\SteamLibrary".
        let path = match value {
            Value::Obj(_) => value.get("path").and_then(Value::str),
            Value::Str(s) if key.chars().all(|c| c.is_ascii_digit()) => Some(s.as_str()),
            Value::Str(_) => None,
        };
        if let Some(p) = path.filter(|p| !p.trim().is_empty()) {
            let p = win_path(p);
            if !out.iter().any(|x| norm(x) == norm(&p)) {
                out.push(p);
            }
        }
    }
    out
}

/// Dernière partie par appid, la plus récente de tous les comptes de la machine.
pub fn last_played(root: &Path, d: &mut Detection) -> HashMap<String, i64> {
    let mut out: HashMap<String, i64> = HashMap::new();
    let users = std::fs::read_dir(root.join("userdata")).map(|it| it.filter_map(Result::ok).map(|e| e.path()).collect::<Vec<_>>()).unwrap_or_default();
    for user in users {
        let file = user.join("config").join("localconfig.vdf");
        if !file.exists() {
            continue;
        }
        let Some(doc) = parse_file(&file, d) else { continue };
        let apps = doc.path(&["UserLocalConfigStore", "Software", "Valve", "Steam", "apps"]);
        for (appid, app) in apps.map(Value::entries).unwrap_or_default() {
            if let Some(t) = app.get("LastPlayed").and_then(Value::str).and_then(|s| s.trim().parse::<i64>().ok()).filter(|t| *t > 0) {
                let e = out.entry(appid.clone()).or_insert(t);
                *e = (*e).max(t);
            }
        }
    }
    out
}

/// Jaquette locale (format 2024 : un dossier par appid ; ancien : fichiers à plat).
pub fn cover(root: &Path, appid: &str) -> Option<PathBuf> {
    let cache = root.join("appcache").join("librarycache");
    [
        cache.join(appid).join("library_600x900.jpg"),
        cache.join(format!("{appid}_library_600x900.jpg")),
        cache.join(appid).join("header.jpg"),
        cache.join(format!("{appid}_header.jpg")),
    ]
    .into_iter()
    .find(|p| p.is_file())
}

pub fn detect(reg: &dyn Registry, d: &mut Detection) {
    let Some(root) = steam_root(reg).filter(|r| r.is_dir()) else { return };
    d.found(Launcher::Steam);
    let libs = libraries(&root, d);
    let played = last_played(&root, d);
    for lib in &libs {
        let apps = lib.join("steamapps");
        for file in list_files(&apps, |n| n.starts_with("appmanifest_") && n.ends_with(".acf")) {
            let Some(doc) = parse_file(&file, d) else { continue };
            let Some(state) = doc.get("AppState") else {
                d.ignore(Launcher::Steam, &file, Reason::Incomplete);
                continue;
            };
            let text = |k: &str| state.get(k).and_then(Value::str).map(str::trim).filter(|s| !s.is_empty());
            let (Some(appid), Some(name), Some(dir)) = (text("appid"), text("name"), text("installdir")) else {
                d.ignore(Launcher::Steam, &file, Reason::Incomplete);
                continue;
            };
            if TOOLS.contains(&appid) {
                continue;
            }
            let manifest_played = text("LastPlayed").and_then(|s| s.parse::<i64>().ok()).filter(|t| *t > 0);
            let last = match (manifest_played, played.get(appid).copied()) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            };
            let shader = apps.join("shadercache").join(appid);
            d.games.push(Game {
                launcher: Launcher::Steam,
                key: appid.to_owned(),
                name: name.to_owned(),
                install_dir: Some(apps.join("common").join(dir)),
                size: text("SizeOnDisk").and_then(|s| s.parse::<u64>().ok()).filter(|s| *s > 0),
                last_played: last,
                cover: cover(&root, appid),
                shader_cache: shader.is_dir().then_some(shader),
            });
        }
    }
    d.steam_libraries = libs;
}
