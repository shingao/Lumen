//! Riot Client : un dossier par jeu dans `%ProgramData%\Riot Games\Metadata\<produit>.<branche>`,
//! avec `<produit>.<branche>.product_settings.yaml` (`product_install_full_path`). L'exécutable
//! du client est lu dans `RiotClientInstalls.json` (`rc_default`). Les clés Uninstall
//! (« Riot Game … ») servent de repli.
//!
//! Chaque produit est une carte : `league_of_legends.live` et `league_of_legends.pbe` (serveur de
//! test, son propre dossier) sont deux jeux. Teamfight Tactics se joue dans le client de League
//! of Legends : c'est un mode rattaché à sa carte, jamais un second jeu (taille comptée une fois).
//! Riot ne garde pas la date de dernière partie : elle est estimée par le journal de partie le
//! plus récent (`Logs\GameLogs` du jeu), et dite « estimée ».

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

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

/// Teamfight Tactics : un mode de League of Legends, jamais un jeu à part.
fn is_tft(product: &str) -> bool {
    matches!(product, "tft" | "teamfight_tactics")
}

/// Modes inclus dans un produit (affichés sur sa carte).
fn modes_of(product: &str) -> Vec<String> {
    if product == "league_of_legends" {
        vec!["Teamfight Tactics".to_owned()]
    } else {
        Vec::new()
    }
}

/// Journaux de partie lus au plus (le dossier peut en accumuler des milliers).
const MAX_LOG_ENTRIES: usize = 4_000;

/// Dernière partie estimée : date de modification la plus récente dans `Logs\GameLogs` (et un
/// niveau dessous : un dossier par partie). Métadonnées seulement ; absent ou vide : inconnue.
pub fn estimated_last_played(install: &Path) -> Option<i64> {
    let logs = install.join("Logs").join("GameLogs");
    let mut newest: Option<i64> = None;
    let mut seen = 0usize;
    let mut stamp = |meta: &std::fs::Metadata| {
        if let Some(secs) = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).and_then(|d| i64::try_from(d.as_secs()).ok()) {
            newest = Some(newest.map_or(secs, |n| n.max(secs)));
        }
    };
    for entry in std::fs::read_dir(&logs).ok()?.flatten() {
        seen += 1;
        if seen > MAX_LOG_ENTRIES {
            break;
        }
        let Ok(meta) = entry.metadata() else { continue };
        stamp(&meta);
        if meta.is_dir() {
            for inner in std::fs::read_dir(entry.path()).into_iter().flatten().flatten().take(64) {
                if let Ok(m) = inner.metadata() {
                    stamp(&m);
                }
            }
        }
    }
    newest
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
        let mut parts = folder.split('.');
        let product = parts.next().unwrap_or(&folder).to_ascii_lowercase();
        let branch = parts.next().unwrap_or("live").to_ascii_lowercase();
        if product == "riot_client" || is_tft(&product) {
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
        let base = product_name(&product).map_or(fallback, str::to_owned);
        // Autre branche que « live » (PBE…) : un jeu à part entière, nommé avec sa branche.
        let name = if branch == "live" { base } else { format!("{base} {}", branch.to_ascii_uppercase()) };
        let last_played = estimated_last_played(&install);
        d.games.push(Game {
            launcher: Launcher::Riot,
            key: folder.clone(),
            name,
            install_dir: Some(install),
            size: None,
            last_played,
            cover: None,
            shader_cache: None,
            modes: modes_of(&product),
            last_played_estimated: last_played.is_some(),
        });
    }
}

/// Jeux Riot trouvés par le repli Uninstall (YAML absent) : mêmes modes et même estimation de la
/// dernière partie que ceux de `Metadata`.
pub fn complete(d: &mut Detection) {
    for g in d.games.iter_mut().filter(|g| g.launcher == Launcher::Riot) {
        let key = g.key.to_ascii_lowercase();
        if g.modes.is_empty() && key.contains("league_of_legends") {
            g.modes = modes_of("league_of_legends");
        }
        if g.last_played.is_none() {
            g.last_played = g.install_dir.as_deref().and_then(estimated_last_played);
            g.last_played_estimated = g.last_played.is_some();
        }
    }
}
