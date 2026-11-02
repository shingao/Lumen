//! HoYoPlay (Genshin Impact, Honkai: Star Rail, Zenless Zone Zero, Honkai Impact 3rd), en lecture
//! seule : une clé par jeu sous `HKCU\Software\Cognosphere\HYP\<version>\<game_biz>` (version
//! mondiale) ou `HKCU\Software\miHoYo\HYP\<version>\<game_biz>` (version chinoise), avec le dossier
//! du jeu dans `GameInstallPath`. Le launcher (`launcher.exe` de son dossier, lu dans son entrée
//! Uninstall) désinstalle ; Lumen ne fait que l'ouvrir. Repli : entrées Uninstall des jeux
//! (éditeur Cognosphere / miHoYo).

use crate::{uninstall, win_path, Detection, Game, Hive, Launcher, Reason, Registry};

const ROOTS: [&str; 2] = ["Software\\Cognosphere\\HYP", "Software\\miHoYo\\HYP"];

/// Nom affiché d'un jeu d'après le préfixe de son identifiant (`hk4e_global`, `hkrpg_cn`…).
fn game_name(biz: &str) -> Option<&'static str> {
    let prefix = biz.split('_').next().unwrap_or(biz).to_ascii_lowercase();
    match prefix.as_str() {
        "hk4e" => Some("Genshin Impact"),
        "hkrpg" => Some("Honkai: Star Rail"),
        "nap" => Some("Zenless Zone Zero"),
        "bh3" => Some("Honkai Impact 3rd"),
        _ => None,
    }
}

/// L'entrée Uninstall du launcher lui-même.
fn is_launcher_entry(e: &uninstall::Entry) -> bool {
    let n = e.name.to_ascii_lowercase();
    n == "hoyoplay" || n.starts_with("hoyoplay ") || n == "米哈游启动器"
}

/// Jeu HoYoverse installé par HoYoPlay, d'après son entrée Uninstall (repli).
pub fn is_game_entry(e: &uninstall::Entry) -> bool {
    let p = e.publisher.to_ascii_lowercase();
    (p.starts_with("cognosphere") || p.starts_with("mihoyo") || p.starts_with("hoyoverse")) && !is_launcher_entry(e)
}

pub fn detect(reg: &dyn Registry, d: &mut Detection) {
    let entries = uninstall::entries(reg);
    if let Some(launcher) = entries.iter().find(|e| is_launcher_entry(e)) {
        d.found(Launcher::HoYoPlay);
        if let Some(dir) = &launcher.location {
            d.launcher_exes.push((Launcher::HoYoPlay, win_path(dir).join("launcher.exe")));
        }
    }
    for root in ROOTS {
        for version in reg.subkeys(Hive::CurrentUser, root) {
            let base = format!("{root}\\{version}");
            for biz in reg.subkeys(Hive::CurrentUser, &base) {
                let key = format!("{base}\\{biz}");
                let Some(dir) = reg.string(Hive::CurrentUser, &key, "GameInstallPath").map(|s| s.trim().trim_matches('"').to_owned()).filter(|s| !s.is_empty()) else {
                    // Clé de réglages du launcher sans dossier de jeu : ignorée sans bruit, sauf si
                    // c'est un jeu connu (installation incomplète).
                    if game_name(&biz).is_some() {
                        d.ignore(Launcher::HoYoPlay, std::path::Path::new(&format!("HKCU\\{key}")), Reason::Incomplete);
                    }
                    continue;
                };
                d.found(Launcher::HoYoPlay);
                let install = win_path(&dir);
                if d.has_dir(&install) {
                    continue;
                }
                let folder = install.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| biz.clone());
                let name = game_name(&biz).map_or(folder, str::to_owned);
                d.games.push(Game {
                    launcher: Launcher::HoYoPlay,
                    key: biz,
                    name,
                    install_dir: Some(install),
                    size: None,
                    // HoYoPlay ne garde pas la date de dernière partie : inconnue.
                    last_played: None,
                    cover: None,
                    shader_cache: None,
                    modes: Vec::new(),
                    last_played_estimated: false,
                });
            }
        }
    }
}
