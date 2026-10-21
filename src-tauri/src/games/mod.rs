//! Onglet Jeux : jeux installés détectés en lecture seule chez leurs launchers (crate
//! `launchers`), tailles mesurées quand le launcher ne les donne pas, jaquettes du cache local,
//! statut (« À vérifier » si la dernière partie connue a plus de 6 mois, sinon « À garder »).
//! Lumen ne désinstalle ni ne déplace jamais un jeu : il ouvre le launcher (`safety::launch`).
//! Les caches de shaders se nettoient ici seulement, refusés si un jeu ou un launcher tourne.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use launchers::{Detection, Game, Launcher, Reason};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::error::LumenError;

/// Jaquette lue au plus (le cache de Steam fait quelques centaines de Ko).
const MAX_COVER: u64 = 2 << 20;

#[derive(Default)]
pub struct GamesState(Mutex<Option<Arc<Found>>>);

pub(crate) struct Found {
    detection: Detection,
    /// Exécutable de chaque launcher trouvé sous Program Files (ouvrir sa bibliothèque).
    apps: Vec<(Launcher, PathBuf)>,
}

impl GamesState {
    pub(crate) fn found(&self) -> Option<Arc<Found>> {
        self.0.lock().ok().and_then(|g| g.clone())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameDto {
    /// `launcher:clé`, seul identifiant que le front renvoie.
    pub id: String,
    pub launcher: &'static str,
    pub name: String,
    pub install_dir: Option<String>,
    pub size: Option<u64>,
    /// `launcher` (annoncée), `measured` (mesurée sur le disque) ou `unknown`.
    pub size_source: &'static str,
    pub drive: Option<String>,
    /// Dernière partie (ms), seulement si le launcher la connaît.
    pub last_played: Option<i64>,
    /// `keep` ou `review` ; jamais coché, Lumen ne supprime jamais un jeu.
    pub status: &'static str,
    pub cover: bool,
    /// Ce que le launcher permet : `uri` (Steam), `app` (ouvrir le launcher) ou `none`.
    pub open: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoredDto {
    pub launcher: &'static str,
    pub file: String,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SteamShaderDto {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GamesDto {
    pub games: Vec<GameDto>,
    pub ignored: Vec<IgnoredDto>,
    pub launchers: Vec<&'static str>,
    /// `steamapps\shadercache` de chaque bibliothèque Steam : sous Program Files le plus souvent,
    /// informatif (à gérer depuis Steam).
    pub steam_shader_caches: Vec<SteamShaderDto>,
}

fn reason(r: Reason) -> &'static str {
    match r {
        Reason::Missing => "missing",
        Reason::Unreadable => "unreadable",
        Reason::TooLarge => "tooLarge",
        Reason::Malformed => "malformed",
        Reason::Incomplete => "incomplete",
    }
}

pub(crate) fn game_id(g: &Game) -> String {
    format!("{}:{}", g.launcher.id(), g.key)
}

fn drive_of(p: &Path) -> Option<String> {
    let s = p.to_string_lossy();
    let b = s.as_bytes();
    (b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic()).then(|| s[..2].to_ascii_uppercase())
}

/// Ce que l'on sait d'un jeu, mis en forme ; `measured` : taille mesurée faute de mieux.
pub(crate) fn game_dto(g: &Game, measured: Option<u64>, apps: &[(Launcher, PathBuf)], now_secs: i64) -> GameDto {
    let (size, size_source) = match (g.size, measured) {
        (Some(s), _) => (Some(s), "launcher"),
        (None, Some(m)) => (Some(m), "measured"),
        (None, None) => (None, "unknown"),
    };
    GameDto {
        id: game_id(g),
        launcher: g.launcher.id(),
        name: g.name.clone(),
        install_dir: g.install_dir.as_ref().map(|p| p.display().to_string()),
        size,
        size_source,
        drive: g.install_dir.as_deref().and_then(drive_of),
        last_played: g.last_played.map(|t| t * 1000),
        status: if launchers::needs_review(g.last_played, now_secs) { "review" } else { "keep" },
        cover: g.cover.is_some(),
        open: match g.launcher {
            Launcher::Steam if g.key.parse::<u32>().is_ok_and(|id| id > 0) => "uri",
            l if apps.iter().any(|(a, _)| *a == l) => "app",
            _ => "none",
        },
    }
}

/// Exécutables des launchers aux emplacements d'installation habituels (Program Files).
fn launcher_apps(program_files: &[PathBuf]) -> Vec<(Launcher, PathBuf)> {
    let candidates: [(Launcher, &str); 10] = [
        (Launcher::Epic, "Epic Games\\Launcher\\Portal\\Binaries\\Win64\\EpicGamesLauncher.exe"),
        (Launcher::Epic, "Epic Games\\Launcher\\Portal\\Binaries\\Win32\\EpicGamesLauncher.exe"),
        (Launcher::Gog, "GOG Galaxy\\GalaxyClient.exe"),
        (Launcher::BattleNet, "Battle.net\\Battle.net Launcher.exe"),
        (Launcher::BattleNet, "Battle.net\\Battle.net.exe"),
        (Launcher::Ea, "Electronic Arts\\EA Desktop\\EA Desktop\\EALauncher.exe"),
        (Launcher::Ea, "Electronic Arts\\EA Desktop\\EA Desktop\\EADesktop.exe"),
        (Launcher::Ubisoft, "Ubisoft\\Ubisoft Game Launcher\\UbisoftConnect.exe"),
        (Launcher::Ubisoft, "Ubisoft\\Ubisoft Game Launcher\\upc.exe"),
        (Launcher::Gog, "GOG Galaxy\\GalaxyClient.exe"),
    ];
    let mut out: Vec<(Launcher, PathBuf)> = Vec::new();
    for (l, rel) in candidates {
        if out.iter().any(|(x, _)| *x == l) {
            continue;
        }
        if let Some(p) = program_files.iter().map(|pf| pf.join(rel.replace('\\', std::path::MAIN_SEPARATOR_STR))).find(|p| p.is_file()) {
            out.push((l, p));
        }
    }
    out
}

mod platform {
    use super::*;

    /// Registre réel : vues 64 et 32 bits fusionnées (les launchers 32 bits écrivent sous
    /// `WOW6432Node`).
    #[cfg(windows)]
    pub struct WinRegistry;

    #[cfg(windows)]
    impl launchers::Registry for WinRegistry {
        fn subkeys(&self, hive: launchers::Hive, path: &str) -> Vec<String> {
            use scan::win::registry::{subkeys, View};
            let h = hive_of(hive);
            let mut out = subkeys(h, View::Native, path);
            for k in subkeys(h, View::Wow32, path) {
                if !out.iter().any(|x| x.eq_ignore_ascii_case(&k)) {
                    out.push(k);
                }
            }
            out
        }

        fn string(&self, hive: launchers::Hive, path: &str, name: &str) -> Option<String> {
            use scan::win::registry::{string, View};
            string(hive_of(hive), View::Native, path, name).or_else(|| string(hive_of(hive), View::Wow32, path, name))
        }

        fn dword(&self, hive: launchers::Hive, path: &str, name: &str) -> Option<u32> {
            use scan::win::registry::{dword, View};
            dword(hive_of(hive), View::Native, path, name).or_else(|| dword(hive_of(hive), View::Wow32, path, name))
        }
    }

    #[cfg(windows)]
    fn hive_of(h: launchers::Hive) -> scan::win::registry::Hive {
        match h {
            launchers::Hive::LocalMachine => scan::win::registry::Hive::LocalMachine,
            launchers::Hive::CurrentUser => scan::win::registry::Hive::CurrentUser,
        }
    }

    #[cfg(windows)]
    pub fn detect() -> Detection {
        let program_data = std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\ProgramData"));
        launchers::detect(&launchers::Env { program_data }, &WinRegistry)
    }

    #[cfg(not(windows))]
    pub fn detect() -> Detection {
        Detection::default()
    }

    pub fn program_files() -> Vec<PathBuf> {
        ["ProgramFiles", "ProgramFiles(x86)"].iter().filter_map(std::env::var_os).map(PathBuf::from).collect()
    }

    /// Taille allouée d'un dossier, par le parcours de l'analyse (métadonnées seulement, dernier
    /// accès gelé). Absent ou illisible : rien.
    #[cfg(windows)]
    pub fn measure(dir: &Path) -> Option<u64> {
        let root = scan::win::root_path(&dir.to_string_lossy());
        let cancel = std::sync::atomic::AtomicBool::new(false);
        scan::walk(&scan::win::WinSource::new(&root), &root, &cancel, &|_| {}).ok().map(|s| s.stats.allocated)
    }

    #[cfg(not(windows))]
    pub fn measure(dir: &Path) -> Option<u64> {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        dir.is_dir().then(|| scan::walk(&scan::unix::StdSource, &dir.to_string_lossy(), &cancel, &|_| {}).ok().map(|s| s.stats.allocated)).flatten()
    }

    #[cfg(windows)]
    pub fn open(target: &safety::launch::LauncherOpen) -> Result<(), String> {
        safety::win::open_launcher(target).map_err(|e| e.to_string())
    }

    #[cfg(not(windows))]
    pub fn open(_: &safety::launch::LauncherOpen) -> Result<(), String> {
        Err("disponible sous Windows uniquement".into())
    }

    #[cfg(windows)]
    pub fn running() -> Vec<safety::guard::RunningProcess> {
        safety::win::running_processes()
    }

    #[cfg(not(windows))]
    pub fn running() -> Vec<safety::guard::RunningProcess> {
        Vec::new()
    }
}

pub(crate) fn now_secs() -> i64 {
    crate::cleanup::unix_ms() / 1000
}

/// Détection complète, tailles manquantes mesurées : appelée hors du fil de l'interface.
fn list(state: &GamesState) -> GamesDto {
    let detection = platform::detect();
    let apps = launcher_apps(&platform::program_files());
    let now = now_secs();
    let games = detection
        .games
        .iter()
        .map(|g| {
            let measured = if g.size.is_none() { g.install_dir.as_deref().and_then(platform::measure) } else { None };
            game_dto(g, measured, &apps, now)
        })
        .collect();
    let steam_shader_caches = detection
        .steam_libraries
        .iter()
        .map(|lib| lib.join("steamapps").join("shadercache"))
        .filter(|p| p.is_dir())
        .filter_map(|p| platform::measure(&p).map(|size| SteamShaderDto { path: p.display().to_string(), size }))
        .collect();
    let dto = GamesDto {
        games,
        ignored: detection.ignored.iter().map(|i| IgnoredDto { launcher: i.launcher.id(), file: i.file.clone(), reason: reason(i.reason) }).collect(),
        launchers: detection.found.iter().map(|l| l.id()).collect(),
        steam_shader_caches,
    };
    if let Ok(mut g) = state.0.lock() {
        *g = Some(Arc::new(Found { detection, apps }));
    }
    dto
}

#[tauri::command]
pub async fn games_list(app: AppHandle) -> Result<GamesDto, LumenError> {
    let worker = app.clone();
    tauri::async_runtime::spawn_blocking(move || list(&worker.state::<GamesState>())).await.map_err(|e| LumenError::Games(e.to_string()))
}

fn find<'a>(found: &'a Found, id: &str) -> Result<&'a Game, LumenError> {
    found.detection.games.iter().find(|g| game_id(g) == id).ok_or_else(|| LumenError::Games("jeu inconnu : relance la détection".into()))
}

/// Jaquette du cache local du launcher, en `data:` (aucun accès réseau).
#[tauri::command]
pub fn game_cover(state: State<'_, GamesState>, id: String) -> Result<Option<String>, LumenError> {
    let found = state.found().ok_or_else(|| LumenError::Games("jeux non détectés".into()))?;
    let Some(path) = find(&found, &id)?.cover.clone() else { return Ok(None) };
    let mime = match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        _ => return Ok(None),
    };
    let bytes = match std::fs::metadata(&path) {
        Ok(m) if m.len() <= MAX_COVER => std::fs::read(&path).ok(),
        _ => None,
    };
    Ok(bytes.map(|b| format!("data:{mime};base64,{}", base64(&b))))
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |acc, (i, &b)| acc | u32::from(b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(T[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Ce que le launcher ouvre pour une action : jamais Lumen ne désinstalle ni ne déplace un jeu.
pub(crate) fn open_target(found: &Found, game: &Game, action: &str) -> Result<safety::launch::LauncherOpen, LumenError> {
    use safety::launch::LauncherOpen;
    if game.launcher == Launcher::Steam {
        let appid = game.key.parse::<u32>().map_err(|_| LumenError::Games("identifiant Steam invalide".into()))?;
        return Ok(match action {
            "uninstall" => LauncherOpen::SteamUninstall(appid),
            _ => LauncherOpen::SteamSettings,
        });
    }
    let exe = found.apps.iter().find(|(l, _)| *l == game.launcher).map(|(_, p)| p).ok_or_else(|| LumenError::Games("launcher introuvable sous Program Files".into()))?;
    let path = safety::path::WinPath::parse(&exe.display().to_string()).map_err(|e| LumenError::Games(e.to_string()))?;
    Ok(LauncherOpen::App(path))
}

#[tauri::command]
pub fn game_open(state: State<'_, GamesState>, id: String, action: String) -> Result<(), LumenError> {
    let found = state.found().ok_or_else(|| LumenError::Games("jeux non détectés".into()))?;
    let target = open_target(&found, find(&found, &id)?, &action)?;
    platform::open(&target).map_err(LumenError::Games)
}

/// Dossiers des jeux détectés (garde des caches de shaders : aucun programme lancé d'ici).
pub(crate) fn game_dirs(state: &GamesState) -> Vec<safety::path::WinPath> {
    state
        .found()
        .map(|f| f.detection.games.iter().filter_map(|g| g.install_dir.as_ref()).filter_map(|p| safety::path::WinPath::parse(&p.display().to_string()).ok()).collect())
        .unwrap_or_default()
}

/// Jeux et launchers ouverts maintenant (avertissement avant de préparer le nettoyage ; le
/// refus définitif est vérifié par `safety` au moment de l'action).
#[tauri::command]
pub fn games_running(state: State<'_, GamesState>) -> Vec<String> {
    let guard = safety::guard::ProcessGuard::games(game_dirs(&state));
    let running = platform::running();
    let mut out: Vec<String> = Vec::new();
    for p in &running {
        if guard.blocking(std::slice::from_ref(p)).is_some() && !out.contains(&p.name) {
            out.push(p.name.clone());
        }
    }
    out
}

/// Règle des caches de shaders (rules.toml, onglet Jeux).
pub(crate) const SHADERS_RULE: &str = "caches-shaders";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShaderCacheDto {
    pub path: String,
    pub size: u64,
    pub files: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShadersDto {
    /// L'analyse affichée couvre-t-elle les caches (dossier utilisateur ou disque système) ?
    pub analysed: bool,
    pub caches: Vec<ShaderCacheDto>,
    pub total: u64,
}

/// Caches de shaders (DirectX, NVIDIA, AMD) trouvés par l'analyse affichée, par dossier de cache.
pub(crate) fn shaders_of(applied: &crate::analysis::Applied, scan: &scan::Scan) -> ShadersDto {
    let Some(set) = applied.set() else { return ShadersDto { analysed: false, caches: Vec::new(), total: 0 } };
    let Some(reco) = applied.analysis.recos.iter().find(|r| set.rules[r.rule].id == SHADERS_RULE) else {
        return ShadersDto { analysed: true, caches: Vec::new(), total: 0 };
    };
    let mut caches: Vec<ShaderCacheDto> = Vec::new();
    for &n in &reco.covered {
        // Portée « contenu » : chaque élément couvert est dans un dossier de cache.
        let dir = scan.node(n).map(|node| node.parent).filter(|&p| p != n).unwrap_or(n);
        let path = applied.display(&scan.path(dir));
        let (size, files) = scan.node(n).map_or((0, 0), |x| (x.allocated, u64::from(x.files.max(1))));
        match caches.iter_mut().find(|c| c.path == path) {
            Some(c) => {
                c.size += size;
                c.files += files;
            }
            None => caches.push(ShaderCacheDto { path, size, files }),
        }
    }
    caches.sort_by_key(|c| std::cmp::Reverse(c.size));
    ShadersDto { analysed: true, total: caches.iter().map(|c| c.size).sum(), caches }
}

#[tauri::command]
pub fn games_shaders(scans: State<'_, crate::scanning::ScanState>, scan_id: Option<u32>) -> Result<ShadersDto, LumenError> {
    let Some(id) = scan_id else { return Ok(ShadersDto { analysed: false, caches: Vec::new(), total: 0 }) };
    let entry = crate::scanning::entry_of(&scans, id)?;
    Ok(match entry.applied.as_ref() {
        Some(a) => shaders_of(a, &entry.scan),
        None => ShadersDto { analysed: false, caches: Vec::new(), total: 0 },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(launcher: Launcher, key: &str, last: Option<i64>, size: Option<u64>) -> Game {
        Game { launcher, key: key.into(), name: "Jeu".into(), install_dir: Some(PathBuf::from("D:\\Jeux\\Jeu")), size, last_played: last, cover: None, shader_cache: None }
    }

    #[test]
    fn statut_taille_et_ouverture() {
        let now = 1_792_584_000;
        let apps = vec![(Launcher::Epic, PathBuf::from("C:\\Program Files (x86)\\Epic Games\\EpicGamesLauncher.exe"))];
        let old = game_dto(&game(Launcher::Steam, "570", Some(now - 200 * 86_400), Some(10)), None, &apps, now);
        assert_eq!((old.status, old.open, old.size_source, old.drive.as_deref()), ("review", "uri", "launcher", Some("D:")));
        assert_eq!(old.last_played, Some((now - 200 * 86_400) * 1000));
        // Date inconnue : « À garder », jamais une date inventée.
        let epic = game_dto(&game(Launcher::Epic, "Fortnite", None, None), Some(42), &apps, now);
        assert_eq!((epic.status, epic.last_played, epic.size, epic.size_source, epic.open), ("keep", None, Some(42), "measured", "app"));
        let gog = game_dto(&game(Launcher::Gog, "1", None, None), None, &apps, now);
        assert_eq!((gog.size, gog.size_source, gog.open), (None, "unknown", "none"));
    }

    #[test]
    fn ouvrir_le_launcher_jamais_supprimer() {
        let found = Found { detection: Detection::default(), apps: vec![(Launcher::Epic, PathBuf::from("C:\\Program Files (x86)\\Epic Games\\EpicGamesLauncher.exe"))] };
        use safety::launch::LauncherOpen;
        assert_eq!(open_target(&found, &game(Launcher::Steam, "570", None, None), "uninstall").unwrap(), LauncherOpen::SteamUninstall(570));
        assert_eq!(open_target(&found, &game(Launcher::Steam, "570", None, None), "move").unwrap(), LauncherOpen::SteamSettings);
        assert!(matches!(open_target(&found, &game(Launcher::Epic, "F", None, None), "uninstall").unwrap(), LauncherOpen::App(_)));
        assert!(open_target(&found, &game(Launcher::Gog, "1", None, None), "uninstall").is_err());
        assert!(open_target(&found, &game(Launcher::Steam, "abc", None, None), "uninstall").is_err());
    }

    #[test]
    fn caches_de_shaders_par_dossier() {
        let (a, scan) = crate::analysis::tests::applied();
        let s = shaders_of(&a, &scan);
        assert!(s.analysed);
        assert_eq!(s.caches.len(), 1);
        assert_eq!((s.caches[0].path.as_str(), s.caches[0].size, s.caches[0].files), ("~\\AppData\\Local\\D3DSCache", 500, 2));
        assert_eq!(s.total, 500);
    }

    #[test]
    fn jaquette_en_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xff, 0xfe, 0x00]), "//4A");
    }
}
