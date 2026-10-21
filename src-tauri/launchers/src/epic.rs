//! Epic Games : un manifeste JSON par jeu dans
//! `%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests\*.item`. Les DLC (AppName différent du
//! jeu principal) et les installations incomplètes ne sont pas des jeux à part. Epic ne garde pas
//! localement la date de dernière partie : elle reste inconnue.

use std::path::Path;

use serde_json::Value;

use crate::{list_files, read_text, win_path, Detection, Game, Launcher, Reason};

pub fn manifests_dir(program_data: &Path) -> std::path::PathBuf {
    program_data.join("Epic").join("EpicGamesLauncher").join("Data").join("Manifests")
}

pub fn detect(program_data: &Path, d: &mut Detection) {
    let dir = manifests_dir(program_data);
    if !dir.is_dir() {
        return;
    }
    d.found(Launcher::Epic);
    for file in list_files(&dir, |n| n.to_ascii_lowercase().ends_with(".item")) {
        let text = match read_text(&file) {
            Ok(t) => t,
            Err(r) => {
                d.ignore(Launcher::Epic, &file, r);
                continue;
            }
        };
        let Ok(json) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else {
            d.ignore(Launcher::Epic, &file, Reason::Malformed);
            continue;
        };
        let s = |k: &str| json.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty());
        let (Some(app), Some(name), Some(location)) = (s("AppName"), s("DisplayName"), s("InstallLocation")) else {
            d.ignore(Launcher::Epic, &file, Reason::Incomplete);
            continue;
        };
        if s("MainGameAppName").is_some_and(|main| !main.eq_ignore_ascii_case(app)) || json.get("bIsIncompleteInstall").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        let dir = win_path(location);
        if d.has_dir(&dir) {
            continue;
        }
        d.games.push(Game {
            launcher: Launcher::Epic,
            key: app.to_owned(),
            name: name.to_owned(),
            size: json.get("InstallSize").and_then(Value::as_u64).filter(|s| *s > 0),
            install_dir: Some(dir),
            last_played: None,
            cover: None,
            shader_cache: None,
        });
    }
}
