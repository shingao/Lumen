//! Jeux installés, détectés en lecture seule dans les fichiers et les clés de registre de leurs
//! launchers (Steam, Epic, GOG, Battle.net, EA app, Ubisoft Connect, Riot Client, CurseForge et
//! Minecraft, jeux Xbox / Microsoft Store de Gaming Services, avec repli sur les clés Uninstall de
//! Windows). Toute lecture est tolérante : un fichier absent, illisible, verrouillé,
//! trop gros ou mal formé est ignoré et compté (`Ignored`), jamais une erreur qui arrête tout.
//! Aucun accès réseau : les jaquettes ne viennent que du cache local des launchers.

pub mod data;
pub mod epic;
pub mod minecraft;
pub mod registry;
pub mod riot;
pub mod steam;
pub mod uninstall;
pub mod vdf;
pub mod xbox;

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
    Riot,
    CurseForge,
    Minecraft,
    /// Jeux du Microsoft Store / Xbox (Gaming Services) : affichés seulement.
    Xbox,
}

impl Launcher {
    pub const ALL: [Launcher; 10] = [
        Launcher::Steam,
        Launcher::Epic,
        Launcher::Gog,
        Launcher::BattleNet,
        Launcher::Ea,
        Launcher::Ubisoft,
        Launcher::Riot,
        Launcher::CurseForge,
        Launcher::Minecraft,
        Launcher::Xbox,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Launcher::Steam => "steam",
            Launcher::Epic => "epic",
            Launcher::Gog => "gog",
            Launcher::BattleNet => "battlenet",
            Launcher::Ea => "ea",
            Launcher::Ubisoft => "ubisoft",
            Launcher::Riot => "riot",
            Launcher::CurseForge => "curseforge",
            Launcher::Minecraft => "minecraft",
            Launcher::Xbox => "xbox",
        }
    }

    /// Le launcher sait-il déplacer un jeu ? (Riot Client et CurseForge : non.)
    pub fn can_move(self) -> bool {
        !matches!(self, Launcher::Riot | Launcher::CurseForge)
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
    /// Exécutables de launchers lus dans leurs propres fichiers (Riot Client, CurseForge), le
    /// plus souvent hors de Program Files : `safety::launch` décide s'ils peuvent être ouverts.
    pub launcher_exes: Vec<(Launcher, PathBuf)>,
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

/// Lecture tolérante d'un fichier binaire de launcher (`.GamingRoot`).
pub fn read_bytes(path: &Path) -> Result<Vec<u8>, Reason> {
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
    Ok(buf)
}

/// Sous-dossiers d'un dossier, triés ; dossier absent ou illisible : vide.
pub fn list_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|it| it.filter_map(Result::ok).filter(|e| e.file_type().is_ok_and(|t| t.is_dir())).map(|e| e.path()).collect())
        .unwrap_or_default();
    out.sort();
    out
}

/// Date ISO 8601 (`2026-05-12T18:22:13.1234567+02:00`, `…Z`, sans fuseau = UTC) en secondes
/// Unix ; années avant 1971 (`0001-01-01T00:00:00` = jamais) ou texte illisible : `None`.
pub fn parse_iso(s: &str) -> Option<i64> {
    let s = s.trim();
    let num = |a: usize, b: usize| s.get(a..b).filter(|t| t.bytes().all(|c| c.is_ascii_digit())).and_then(|t| t.parse::<i64>().ok());
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    if s.as_bytes().get(4) != Some(&b'-') || s.as_bytes().get(7) != Some(&b'-') || !(1971..=9999).contains(&y) || !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let (h, mi, sec) = if s.len() >= 19 && matches!(s.as_bytes()[10], b'T' | b' ') { (num(11, 13)?, num(14, 16)?, num(17, 19)?) } else { (0, 0, 0) };
    if h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    // Fuseau : `Z`, `+hh:mm` ou `-hh:mm` après les secondes et leurs décimales.
    let tail = s.get(19..).unwrap_or("").trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    let offset = match tail.as_bytes().first() {
        Some(b'+') | Some(b'-') => {
            let sign = if tail.starts_with('-') { -1 } else { 1 };
            let hh = tail.get(1..3).and_then(|t| t.parse::<i64>().ok())?;
            let mm = tail.get(4..6).and_then(|t| t.parse::<i64>().ok()).unwrap_or(0);
            sign * (hh * 3600 + mm * 60)
        }
        _ => 0,
    };
    // Jours depuis 1970 (algorithme de Howard Hinnant).
    let yy = if mo <= 2 { y - 1 } else { y };
    let era = yy.div_euclid(400);
    let yoe = yy - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + h * 3600 + mi * 60 + sec - offset)
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
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// `%ProgramData%`.
    pub program_data: PathBuf,
    /// Profil de l'utilisateur (`%USERPROFILE%`).
    pub profile: Option<PathBuf>,
    /// `%LOCALAPPDATA%`.
    pub local_app_data: Option<PathBuf>,
    /// `%APPDATA%`.
    pub app_data: Option<PathBuf>,
    /// Racines des disques fixes (`C:\`, `D:\`…), pour les jeux Xbox.
    pub drives: Vec<PathBuf>,
}

/// Tous les launchers, dans l'ordre ; les clés Uninstall ne servent qu'en repli.
pub fn detect(env: &Env, reg: &dyn Registry) -> Detection {
    let mut d = Detection::default();
    steam::detect(reg, &mut d);
    epic::detect(&env.program_data, &mut d);
    riot::detect(&env.program_data, &mut d);
    minecraft::detect(env, &mut d);
    xbox::detect(&env.drives, &mut d);
    uninstall::detect(reg, &mut d);
    d.games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.launcher.cmp(&b.launcher)));
    d
}

#[cfg(test)]
mod tests;
