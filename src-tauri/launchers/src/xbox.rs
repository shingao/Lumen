//! Jeux Xbox / Microsoft Store installés par Gaming Services : à la racine de chaque disque fixe,
//! `.GamingRoot` (en-tête `RGBX`, puis les dossiers de jeux en UTF-16, sans lettre) désigne le
//! dossier des jeux (`XboxGames` par défaut) ; chaque jeu y a `Content\MicrosoftGame.config`
//! (nom affiché, logo local). Affichés seulement : ils se désinstallent et se déplacent par
//! l'appli Xbox. WindowsApps n'est jamais lu.

use std::path::{Path, PathBuf};

use crate::{list_dirs, read_bytes, read_text, Detection, Game, Launcher, Reason};

/// Dossiers de jeux d'un `.GamingRoot`, relatifs à la racine du disque ; illisible : `None`.
pub fn gaming_roots(bytes: &[u8]) -> Option<Vec<String>> {
    if bytes.len() < 8 || &bytes[..4] != b"RGBX" || (bytes.len() - 8) % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = bytes[8..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let text = String::from_utf16(&units).ok()?;
    let out: Vec<String> = text
        .split('\0')
        .map(|s| s.trim().trim_start_matches(['\\', '/']).to_owned())
        .filter(|s| !s.is_empty() && !s.contains(':') && !s.split(['\\', '/']).any(|c| c == ".."))
        .collect();
    Some(out)
}

/// Valeur d'un attribut XML (`Nom="valeur"`) après la balise donnée, sans analyser le reste.
pub fn xml_attr(text: &str, tag: &str, attr: &str) -> Option<String> {
    let start = text.find(&format!("<{tag}"))?;
    let rest = &text[start..];
    let end = rest.find('>')?;
    let head = &rest[..end];
    let key = format!("{attr}=\"");
    let i = head.find(&key)? + key.len();
    let v = head[i..].split('"').next()?.trim();
    (!v.is_empty() && !v.starts_with("ms-resource:")).then(|| v.replace("&amp;", "&").replace("&apos;", "'").replace("&quot;", "\""))
}

fn game(dir: &Path, d: &mut Detection) {
    let Some(folder) = dir.file_name().map(|n| n.to_string_lossy().into_owned()) else { return };
    let config = dir.join("Content").join("MicrosoftGame.config");
    let text = match read_text(&config) {
        Ok(t) => t,
        Err(r) => {
            d.ignore(Launcher::Xbox, &config, if r == Reason::Missing { Reason::Incomplete } else { r });
            return;
        }
    };
    if !text.contains("<Game") {
        d.ignore(Launcher::Xbox, &config, Reason::Malformed);
        return;
    }
    let name = xml_attr(&text, "ShellVisuals", "DefaultDisplayName").unwrap_or_else(|| folder.clone());
    let key = xml_attr(&text, "Identity", "Name").unwrap_or_else(|| folder.clone());
    let cover = xml_attr(&text, "ShellVisuals", "StoreLogo")
        .filter(|f| !f.contains("..") && !f.contains(':'))
        .map(|f| dir.join("Content").join(f.replace('\\', std::path::MAIN_SEPARATOR_STR)))
        .filter(|p| p.is_file());
    if d.has_dir(dir) {
        return;
    }
    d.games.push(Game { launcher: Launcher::Xbox, key, name, install_dir: Some(dir.to_path_buf()), size: None, last_played: None, cover, shader_cache: None });
}

pub fn detect(drives: &[PathBuf], d: &mut Detection) {
    for drive in drives {
        let file = drive.join(".GamingRoot");
        let mut roots: Vec<PathBuf> = match read_bytes(&file) {
            Ok(b) => match gaming_roots(&b) {
                Some(r) => r.iter().map(|r| drive.join(r.replace('\\', std::path::MAIN_SEPARATOR_STR))).collect(),
                None => {
                    d.ignore(Launcher::Xbox, &file, Reason::Malformed);
                    Vec::new()
                }
            },
            Err(Reason::Missing) => Vec::new(),
            Err(r) => {
                d.ignore(Launcher::Xbox, &file, r);
                Vec::new()
            }
        };
        let default = drive.join("XboxGames");
        if !roots.iter().any(|r| crate::norm(r) == crate::norm(&default)) {
            roots.push(default);
        }
        for root in roots.iter().filter(|r| r.is_dir()) {
            d.found(Launcher::Xbox);
            for dir in list_dirs(root) {
                game(&dir, d);
            }
        }
    }
}
