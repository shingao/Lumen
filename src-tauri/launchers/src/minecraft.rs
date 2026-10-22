//! Minecraft : chaque instance de CurseForge est un jeu à part
//! (`%USERPROFILE%\curseforge\minecraft\Instances\<instance>\minecraftinstance.json`, ou
//! l'ancien `Documents\Curse\Minecraft\Instances`), avec sa taille (mesurée) et sa dernière
//! partie (`lastPlayed`). Minecraft: Java Edition (`%APPDATA%\.minecraft`) en est un aussi, ainsi
//! que chaque installation du Minecraft Launcher qui a son propre dossier de jeu (`gameDir`) ;
//! dernière partie : `lastUsed` de `launcher_profiles.json`.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::{list_dirs, norm, parse_iso, read_text, win_path, Detection, Env, Game, Launcher, Reason};

/// Dossiers d'instances de CurseForge (actuel, puis ancien emplacement).
pub fn instance_roots(profile: &Path) -> [PathBuf; 2] {
    [
        profile.join("curseforge").join("minecraft").join("Instances"),
        profile.join("Documents").join("Curse").join("Minecraft").join("Instances"),
    ]
}

/// Exécutable de l'appli CurseForge (installée pour l'utilisateur seul).
pub fn curseforge_exe(local_app_data: &Path) -> PathBuf {
    local_app_data.join("Programs").join("CurseForge Windows").join("CurseForge.exe")
}

fn json(file: &Path, d: &mut Detection, launcher: Launcher) -> Option<Value> {
    match read_text(file) {
        Ok(t) => match serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')) {
            Ok(v) => Some(v),
            Err(_) => {
                d.ignore(launcher, file, Reason::Malformed);
                None
            }
        },
        Err(r) => {
            d.ignore(launcher, file, r);
            None
        }
    }
}

fn curseforge(env: &Env, d: &mut Detection) {
    if let Some(local) = &env.local_app_data {
        let exe = curseforge_exe(local);
        if exe.is_file() {
            d.found(Launcher::CurseForge);
            d.launcher_exes.push((Launcher::CurseForge, exe));
        }
    }
    let Some(profile) = &env.profile else { return };
    for root in instance_roots(profile) {
        if !root.is_dir() {
            continue;
        }
        d.found(Launcher::CurseForge);
        for dir in list_dirs(&root) {
            let Some(folder) = dir.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
            let Some(v) = json(&dir.join("minecraftinstance.json"), d, Launcher::CurseForge) else { continue };
            let name = v.get("name").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).unwrap_or(&folder).to_owned();
            if d.has_dir(&dir) {
                continue;
            }
            d.games.push(Game {
                launcher: Launcher::CurseForge,
                key: folder,
                name,
                install_dir: Some(dir),
                size: None,
                last_played: v.get("lastPlayed").and_then(Value::as_str).and_then(parse_iso),
                cover: None,
                shader_cache: None,
            });
        }
    }
}

fn vanilla(env: &Env, d: &mut Detection) {
    let Some(roaming) = &env.app_data else { return };
    let mc = roaming.join(".minecraft");
    if !mc.is_dir() {
        return;
    }
    d.found(Launcher::Minecraft);
    let mut last = None;
    let mut extra: Vec<Game> = Vec::new();
    let profiles = mc.join("launcher_profiles.json");
    if profiles.is_file() {
        if let Some(v) = json(&profiles, d, Launcher::Minecraft) {
            for (id, p) in v.get("profiles").and_then(Value::as_object).into_iter().flatten() {
                let used = p.get("lastUsed").and_then(Value::as_str).and_then(parse_iso);
                match p.get("gameDir").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(win_path) {
                    Some(dir) if norm(&dir) != norm(&mc) && dir.is_dir() => {
                        let name = p.get("name").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).unwrap_or(id).to_owned();
                        extra.push(Game { launcher: Launcher::Minecraft, key: format!("profile:{id}"), name, install_dir: Some(dir), size: None, last_played: used, cover: None, shader_cache: None });
                    }
                    _ => last = last.max(used),
                }
            }
        }
    }
    if !d.has_dir(&mc) {
        d.games.push(Game { launcher: Launcher::Minecraft, key: "java".into(), name: "Minecraft: Java Edition".into(), install_dir: Some(mc), size: None, last_played: last, cover: None, shader_cache: None });
    }
    for g in extra {
        if g.install_dir.as_deref().is_some_and(|p| !d.has_dir(p)) {
            d.games.push(g);
        }
    }
}

pub fn detect(env: &Env, d: &mut Detection) {
    curseforge(env, d);
    vanilla(env, d);
}
