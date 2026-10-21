//! Jeux installés, détectés en lecture seule dans les fichiers et les clés de registre de leurs
//! launchers (Steam, Epic, GOG, Battle.net, EA app, Ubisoft Connect, avec repli sur les clés
//! Uninstall de Windows). Toute lecture est tolérante : un fichier absent, illisible, verrouillé,
//! trop gros ou mal formé est ignoré et compté (`Ignored`), jamais une erreur qui arrête tout.
//! Aucun accès réseau : les jaquettes ne viennent que du cache local des launchers.

pub mod epic;
pub mod registry;
pub mod steam;
pub mod uninstall;
pub mod vdf;

use std::path::{Path, PathBuf};

pub use registry::{Hive, MemRegistry, Registry};

/// Taille maximale d'un fichier de launcher lu (manifeste, bibliothèque, config).
pub const MAX_FILE: u64 = 8 << 20;
/// Jeu « À vérifier » s'il n'a pas été lancé depuis plus de 6 mois.
pub const REVIEW_AFTER_SECS: i64 = 183 * 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Launcher {
    Steam,
    Epic,
    Gog,
    BattleNet,
    Ea,
    Ubisoft,
}

impl Launcher {
    pub const ALL: [Launcher; 6] = [Launcher::Steam, Launcher::Epic, Launcher::Gog, Launcher::BattleNet, Launcher::Ea, Launcher::Ubisoft];

    pub fn id(self) -> &'static str {
        match self {
            Launcher::Steam => "steam",
            Launcher::Epic => "epic",
            Launcher::Gog => "gog",
            Launcher::BattleNet => "battlenet",
            Launcher::Ea => "ea",
            Launcher::Ubisoft => "ubisoft",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Game {
    pub launcher: Launcher,
    /// Identifiant chez le launcher (appid Steam, AppName Epic, id GOG, clé Uninstall…).
    pub key: String,
    pub name: String,
    pub install_dir: Option<PathBuf>,
    /// Taille annoncée par le launcher (octets), si elle est connue.
    pub size: Option<u64>,
    /// Dernière partie (secondes Unix), seulement si le launcher la connaît.
    pub last_played: Option<i64>,
    /// Jaquette dans le cache local du launcher.
    pub cover: Option<PathBuf>,
    /// Cache de shaders du jeu géré par le launcher (Steam), informatif.
    pub shader_cache: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Missing,
    /// Illisible ou verrouillé.
    Unreadable,
    TooLarge,
    Malformed,
    /// Lisible, mais sans ce qui identifie un jeu (nom, dossier).
    Incomplete,
}

/// Élément ignoré, compté et affiché, jamais bloquant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ignored {
    pub launcher: Launcher,
    pub file: String,
    pub reason: Reason,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Detection {
    pub games: Vec<Game>,
    pub ignored: Vec<Ignored>,
    /// Launchers trouvés sur la machine.
    pub found: Vec<Launcher>,
    /// Bibliothèques Steam (shadercache, informatif).
    pub steam_libraries: Vec<PathBuf>,
}

impl Detection {
    fn ignore(&mut self, launcher: Launcher, file: &Path, reason: Reason) {
        self.ignored.push(Ignored { launcher, file: file.display().to_string(), reason });
    }

    fn found(&mut self, launcher: Launcher) {
        if !self.found.contains(&launcher) {
            self.found.push(launcher);
        }
    }

    /// Un jeu déjà trouvé ailleurs (même dossier) n'est pas compté deux fois.
    fn has_dir(&self, dir: &Path) -> bool {
        let d = norm(dir);
        self.games.iter().any(|g| g.install_dir.as_deref().is_some_and(|x| norm(x) == d))
    }
}

/// Chemin comparable : barres obliques Windows, casse ignorée, sans séparateur final.
pub fn norm(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// Chemin Windows d'un texte de launcher (`c:/program files (x86)/steam` → `c:\…`).
pub fn win_path(s: &str) -> PathBuf {
    PathBuf::from(s.trim().replace(['/', '\\'], std::path::MAIN_SEPARATOR_STR))
}

/// Lecture tolérante d'un fichier texte de launcher.
pub fn read_text(path: &Path) -> Result<String, Reason> {
    use std::io::Read;
    let meta = std::fs::metadata(path).map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { Reason::Missing } else { Reason::Unreadable })?;
    if !meta.is_file() {
        return Err(Reason::Unreadable);
    }
    if meta.len() > MAX_FILE {
        return Err(Reason::TooLarge);
    }
    let mut buf = Vec::new();
    std::fs::File::open(path).and_then(|f| f.take(MAX_FILE).read_to_end(&mut buf)).map_err(|_| Reason::Unreadable)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Fichiers d'un dossier dont le nom vérifie `keep`, triés ; dossier absent ou illisible : vide.
pub fn list_files(dir: &Path, keep: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|it| it.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(&keep)).collect())
        .unwrap_or_default();
    out.sort();
    out
}

/// Statut d'un jeu : « À vérifier » seulement si sa dernière partie est connue et date de plus de
/// 6 mois ; sinon « À garder » (une date inconnue ne fait jamais d'un jeu un candidat).
pub fn needs_review(last_played: Option<i64>, now: i64) -> bool {
    last_played.is_some_and(|t| now - t > REVIEW_AFTER_SECS)
}

/// Où chercher sur cette machine.
#[derive(Debug, Clone)]
pub struct Env {
    /// `%ProgramData%`.
    pub program_data: PathBuf,
}

/// Tous les launchers, dans l'ordre ; les clés Uninstall ne servent qu'en repli.
pub fn detect(env: &Env, reg: &dyn Registry) -> Detection {
    let mut d = Detection::default();
    steam::detect(reg, &mut d);
    epic::detect(&env.program_data, &mut d);
    uninstall::detect(reg, &mut d);
    d.games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.launcher.cmp(&b.launcher)));
    d
}

#[cfg(test)]
mod tests;
