//! Données et sauvegardes d'un jeu hors de son dossier d'installation : `Documents\My Games`,
//! `Saved Games`, `AppData\Local`, `AppData\LocalLow`, `AppData\Roaming` (sous le nom du jeu,
//! directement ou sous celui de son éditeur), `userdata\<compte>\<appid>` de Steam et le dossier
//! `saves` d'une instance Minecraft. Rattachées par le nom du jeu (lettres et chiffres, casse
//! ignorée ; sous un éditeur, le début ou la fin du nom suffit : « Skyrim Special Edition » pour
//! « The Elder Scrolls V: Skyrim Special Edition ») ; lecture des listes de dossiers seulement.

use std::path::{Path, PathBuf};

use crate::{list_dirs, norm, Game, Launcher};

/// Où chercher, d'après les dossiers connus de l'utilisateur.
#[derive(Debug, Clone, Default)]
pub struct DataRoots {
    pub documents: Option<PathBuf>,
    pub profile: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub app_data: Option<PathBuf>,
    /// Dossiers `userdata\<compte>` de Steam.
    pub steam_users: Vec<PathBuf>,
}

/// Dossier candidat : clé de nom, chemin, et si un préfixe suffit (sinon nom identique).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub key: String,
    pub path: PathBuf,
    pub prefix: bool,
}

/// Clé de comparaison : lettres et chiffres, en minuscules (`Baldur's Gate 3` → `baldursgate3`).
pub fn key(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

fn push_children(out: &mut Vec<Candidate>, dir: &Path, prefix: bool) {
    for p in list_dirs(dir) {
        if let Some(name) = p.file_name().map(|n| key(&n.to_string_lossy())).filter(|k| !k.is_empty()) {
            out.push(Candidate { key: name, path: p, prefix });
        }
    }
}

impl DataRoots {
    /// Dossiers candidats, listés une fois pour tous les jeux.
    pub fn candidates(&self) -> Vec<Candidate> {
        let mut out = Vec::new();
        // Dossiers propres aux jeux : le nom du jeu ou de son éditeur, puis le jeu.
        let games: Vec<PathBuf> = [self.documents.as_ref().map(|d| d.join("My Games")), self.profile.as_ref().map(|p| p.join("Saved Games"))].into_iter().flatten().collect();
        for root in &games {
            push_children(&mut out, root, true);
            for publisher in list_dirs(root) {
                push_children(&mut out, &publisher, true);
            }
        }
        // AppData : premier niveau seulement sous le nom exact (Local\Microsoft n'est pas un jeu),
        // second niveau (éditeur\jeu) par préfixe.
        let appdata: Vec<PathBuf> = [
            self.local_app_data.clone(),
            self.profile.as_ref().map(|p| p.join("AppData").join("LocalLow")),
            self.app_data.clone(),
        ]
        .into_iter()
        .flatten()
        .collect();
        for root in &appdata {
            push_children(&mut out, root, false);
            for publisher in list_dirs(root) {
                push_children(&mut out, &publisher, true);
            }
        }
        out
    }
}

fn same(candidate: &Candidate, name: &str) -> bool {
    let c = candidate.key.as_str();
    c == name
        || (candidate.prefix && ((c.len() >= 6 && name.starts_with(c)) || (name.len() >= 6 && c.starts_with(name)) || (c.len() >= 10 && name.ends_with(c))))
}

/// Dossiers de données d'un jeu (sans doublon ni dossier inclus dans un autre).
pub fn data_dirs(game: &Game, candidates: &[Candidate], steam_users: &[PathBuf]) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    match game.launcher {
        Launcher::Steam => {
            for user in steam_users {
                let p = user.join(&game.key);
                if game.key.chars().all(|c| c.is_ascii_digit()) && p.is_dir() {
                    found.push(p);
                }
            }
        }
        Launcher::CurseForge | Launcher::Minecraft => {
            if let Some(saves) = game.install_dir.as_ref().map(|d| d.join("saves")).filter(|p| p.is_dir()) {
                found.push(saves);
            }
        }
        _ => {}
    }
    let mut names = vec![key(&game.name)];
    if let Some(folder) = game.install_dir.as_ref().and_then(|d| d.file_name()).map(|n| key(&n.to_string_lossy())) {
        if !names.contains(&folder) {
            names.push(folder);
        }
    }
    names.retain(|n| n.len() >= 3);
    // Une instance Minecraft n'a pas de dossier de données ailleurs que dans son dossier.
    if !matches!(game.launcher, Launcher::CurseForge | Launcher::Minecraft) {
        let install = game.install_dir.as_deref().map(norm);
        for c in candidates {
            let inside_install = install.as_ref().is_some_and(|i| norm(&c.path).starts_with(&format!("{i}\\")) || &norm(&c.path) == i);
            if !inside_install && names.iter().any(|n| same(c, n)) {
                found.push(c.path.clone());
            }
        }
    }
    found.sort_by_key(|p| norm(p));
    found.dedup_by_key(|p| norm(p));
    let all = found.clone();
    found.retain(|p| {
        let n = norm(p);
        !all.iter().any(|q| {
            let m = norm(q);
            m != n && n.starts_with(&format!("{m}\\"))
        })
    });
    found
}
