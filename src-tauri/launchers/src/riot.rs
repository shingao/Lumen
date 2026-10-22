//! Riot Client : un dossier par jeu dans `%ProgramData%\Riot Games\Metadata\<produit>.<branche>`,
//! avec `<produit>.<branche>.product_settings.yaml` (`product_install_full_path`). L'exécutable
//! du client est lu dans `RiotClientInstalls.json` (`rc_default`). Les clés Uninstall
//! (« Riot Game … ») servent de repli. Riot ne garde pas localement la date de dernière partie.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{list_dirs, read_text, win_path, Detection, Game, Launcher, Reason};

pub fn riot_dir(program_data: &Path) -> PathBuf {
    program_data.join("Riot Games")
}

/// Nom affiché des produits connus ; sinon le nom du dossier d'installation.
fn product_name(product: &str) -> Option<&'static str> {
    match product {
        "league_of_legends" => Some("League of Legends"),
        "valorant" => Some("VALORANT"),
        "bacon" => Some("Legends of Runeterra"),
        _ => None,
    }
}

/// Valeur d'une clé `cle: "valeur"` d'un YAML plat, sans interpréter le reste du fichier.
pub fn yaml_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|line| {
        let (k, v) = line.trim_start().split_once(':')?;
        (k.trim() == key).then(|| v.trim().trim_matches(|c| c == '"' || c == '\'').trim().to_owned()).filter(|v| !v.is_empty())
    })
}

pub fn detect(program_data: &Path, d: &mut Detection) {
    let root = riot_dir(program_data);
    if !root.is_dir() {
        return;
    }
    d.found(Launcher::Riot);
    let installs = root.join("RiotClientInstalls.json");
    match read_text(&installs).map(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}'))) {
        Ok(Ok(json)) => {
            if let Some(exe) = ["rc_default", "rc_live"].iter().find_map(|k| json.get(k).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty())) {
                d.launcher_exes.push((Launcher::Riot, win_path(exe)));
            }
        }
        Ok(Err(_)) => d.ignore(Launcher::Riot, &installs, Reason::Malformed),
        Err(Reason::Missing) => {}
        Err(r) => d.ignore(Launcher::Riot, &installs, r),
    }
    for dir in list_dirs(&root.join("Metadata")) {
        let Some(folder) = dir.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
        let product = folder.split('.').next().unwrap_or(&folder).to_ascii_lowercase();
        if product == "riot_client" {
            continue;
        }
        let file = dir.join(format!("{folder}.product_settings.yaml"));
        let text = match read_text(&file) {
            Ok(t) => t,
            Err(r) => {
                d.ignore(Launcher::Riot, &file, r);
                continue;
            }
        };
        let Some(location) = yaml_value(&text, "product_install_full_path") else {
            d.ignore(Launcher::Riot, &file, Reason::Incomplete);
            continue;
        };
        let install = win_path(&location);
        if d.has_dir(&install) {
            continue;
        }
        let fallback = install.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| folder.clone());
        d.games.push(Game {
            launcher: Launcher::Riot,
            key: folder.clone(),
            name: product_name(&product).map_or(fallback, str::to_owned),
            install_dir: Some(install),
            size: None,
            last_played: None,
            cover: None,
            shader_cache: None,
        });
    }
}
