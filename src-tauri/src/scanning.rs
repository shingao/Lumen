//! Commandes IPC de l'analyse. Le front ne transmet jamais de chemin : une cible (« dossier
//! utilisateur » ou une lettre de lecteur présente dans la liste des volumes), puis des
//! identifiants de nœuds de l'analyse. Il ne reçoit que des tranches agrégées.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::analysis::{self, Applied, RecoDetailDto, RecoDto, RuleKpisDto};
use crate::error::LumenError;
use scan::{Kind, NodeId, Progress, Scan, ScanError, SliceNode};

/// Enfants les plus gros envoyés par niveau ; le reste est regroupé en « autres ».
const TOP_N: usize = 30;
/// Niveaux de la treemap sous le dossier affiché, au plus.
const MAX_LEVELS: u8 = 4;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ScanTarget {
    UserProfile,
    Volume { letter: String },
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ScanMethod {
    /// Parcours, ou MFT si l'appli est déjà élevée et le volume NTFS.
    Auto,
    /// « Scan rapide (admin) » : MFT, avec demande UAC si besoin.
    Fast,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub elevated: bool,
    /// « Scan rapide (admin) » proposé : Lumen installé dans Program Files, non modifiable.
    pub fast_scan_available: bool,
    pub version: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeDto {
    pub letter: String,
    pub label: String,
    pub file_system: String,
    pub kind: &'static str,
    pub total: u64,
    pub free: u64,
    pub mft_capable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopDto {
    pub name: String,
    pub allocated: u64,
    pub files: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressDto {
    pub scan_id: u32,
    pub files: u64,
    pub dirs: u64,
    pub allocated: u64,
    pub errors: u64,
    pub elapsed_ms: u64,
    pub phase: &'static str,
    pub tops: Vec<TopDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryDto {
    pub scan_id: u32,
    pub root_path: String,
    pub method: &'static str,
    pub files: u64,
    pub dirs: u64,
    pub allocated: u64,
    pub logical: u64,
    pub links: u64,
    pub cloud_files: u64,
    pub cloud_dirs: u64,
    pub cloud_bytes: u64,
    pub hardlink_dups: u64,
    pub access_denied: u64,
    pub locked: u64,
    pub other_errors: u64,
    pub elapsed_ms: u64,
    pub last_access_reliable: bool,
    /// Lecteur analysé, pour l'espace libre et « non analysé ».
    pub volume: String,
    /// Information discrète pour l'interface (`mftFragmented` : le parcours a pris le relais).
    pub notice: Option<&'static str>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanFailedDto {
    pub scan_id: u32,
    pub cancelled: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeNodeDto {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub kind: &'static str,
    /// Taille allouée sur le disque (affichée par défaut).
    pub size: u64,
    /// Taille logique (infobulle).
    pub logical: u64,
    pub files: u64,
    /// Dernière utilisation, en millisecondes depuis 1970.
    pub last_opened: i64,
    pub others: Option<u32>,
    pub has_children: bool,
    /// Moteur de règles : statut, catégorie, recommandation qui le couvre, octets récupérables dessous.
    pub status: &'static str,
    pub category: &'static str,
    pub reco: Option<String>,
    pub gain: u64,
    pub gain_status: Option<&'static str>,
    pub gain_up_to: bool,
    pub children: Vec<TreeNodeDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrumbDto {
    pub label: String,
    pub id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeSliceDto {
    pub root: TreeNodeDto,
    pub breadcrumb: Vec<CrumbDto>,
}

pub(crate) struct Entry {
    pub(crate) scan: Scan,
    pub(crate) volume: String,
    notice: Option<&'static str>,
    /// Règles appliquées (absent hors de Windows ou si les règles livrées sont invalides).
    pub(crate) applied: Option<Applied>,
}

#[derive(Default)]
struct Inner {
    next_id: u32,
    running: Option<(u32, Arc<AtomicBool>)>,
    results: HashMap<u32, Arc<Entry>>,
}

#[derive(Default)]
pub struct ScanState(Mutex<Inner>);

fn kind_name(k: Kind) -> &'static str {
    match k {
        Kind::Dir => "dir",
        Kind::File => "file",
        Kind::Link => "link",
        Kind::CloudFile => "cloudFile",
        Kind::CloudDir => "cloudDir",
    }
}

/// FILETIME (100 ns depuis 1601) → millisecondes depuis 1970.
fn epoch_ms(filetime: i64) -> i64 {
    if filetime <= 0 {
        return 0;
    }
    filetime / 10_000 - 11_644_473_600_000
}

fn to_dto(n: SliceNode, applied: Option<&Applied>, scan: &Scan) -> TreeNodeDto {
    // Nœud « autres » (identifiant négatif) : statut, catégorie et gain de ce qu'il regroupe.
    let view = applied.map(|a| if n.id >= 0 { a.node(n.id as NodeId) } else { a.aggregate(scan, &n.members) });
    let reco = view.and_then(|v| v.reco);
    TreeNodeDto {
        id: n.id,
        name: n.name,
        path: n.path,
        kind: kind_name(n.kind),
        size: n.allocated,
        logical: n.logical,
        files: n.files,
        last_opened: epoch_ms(n.last_used),
        others: n.others,
        has_children: n.has_children,
        status: view.map_or("keep", |v| v.status),
        category: view.map_or("tools", |v| v.category),
        reco: reco.and_then(|r| applied.and_then(|a| a.reco_id(r))),
        gain: view.map_or(0, |v| v.gain),
        gain_status: view.and_then(|v| v.gain_status),
        gain_up_to: reco.is_some_and(|r| applied.is_some_and(|a| a.reco_up_to(r))),
        children: n.children.into_iter().map(|c| to_dto(c, applied, scan)).collect(),
    }
}

fn summary(id: u32, e: &Entry) -> SummaryDto {
    let s = &e.scan.stats;
    SummaryDto {
        scan_id: id,
        root_path: e.scan.root_path.clone(),
        method: match e.scan.method {
            scan::Method::Walk => "walk",
            scan::Method::Mft => "mft",
        },
        files: s.files,
        dirs: s.dirs,
        allocated: s.allocated,
        logical: s.logical,
        links: s.links,
        cloud_files: s.cloud_files,
        cloud_dirs: s.cloud_dirs,
        cloud_bytes: s.cloud_bytes,
        hardlink_dups: s.hardlink_dups,
        access_denied: s.access_denied,
        locked: s.locked,
        other_errors: s.other_errors,
        elapsed_ms: s.elapsed_ms,
        last_access_reliable: e.scan.last_access_reliable,
        volume: e.volume.clone(),
        notice: e.notice,
    }
}

/// MFT demandée mais de forme non gérée (MFT très fragmentée) : le parcours prend le relais, et
/// l'interface l'indique. Toute autre erreur de la MFT reste une erreur.
fn with_fallback(
    mft: Option<Box<dyn FnOnce() -> Result<Scan, ScanError> + '_>>,
    walk: impl FnOnce() -> Result<Scan, ScanError>,
) -> Result<(Scan, Option<&'static str>), ScanError> {
    match mft.map(|m| m()) {
        Some(Err(ScanError::MftUnsupported(_))) => walk().map(|s| (s, Some("mftFragmented"))),
        Some(other) => other.map(|s| (s, None)),
        None => walk().map(|s| (s, None)),
    }
}

pub(crate) mod platform {
    use super::*;

    #[cfg(windows)]
    pub fn volumes() -> Vec<VolumeDto> {
        scan::win::volumes()
            .into_iter()
            .map(|v| VolumeDto {
                letter: format!("{}:", v.letter),
                mft_capable: v.mft_capable(),
                label: v.label,
                file_system: v.file_system,
                kind: match v.kind {
                    scan::win::VolumeKind::Fixed => "fixed",
                    scan::win::VolumeKind::Removable => "removable",
                    scan::win::VolumeKind::Network => "network",
                    scan::win::VolumeKind::Other => "other",
                },
                total: v.total,
                free: v.free,
            })
            .collect()
    }

    #[cfg(not(windows))]
    pub fn volumes() -> Vec<VolumeDto> {
        Vec::new()
    }

    #[cfg(windows)]
    pub fn elevated() -> bool {
        scan::win::is_elevated()
    }

    #[cfg(not(windows))]
    pub fn elevated() -> bool {
        false
    }

    #[cfg(windows)]
    pub fn user_profile() -> Option<String> {
        scan::win::user_profile()
    }

    #[cfg(not(windows))]
    pub fn user_profile() -> Option<String> {
        std::env::var("HOME").ok()
    }

    #[cfg(windows)]
    pub fn fast_scan_available() -> bool {
        scan::win::trust::helper_executable().is_ok()
    }

    #[cfg(not(windows))]
    pub fn fast_scan_available() -> bool {
        false
    }

    /// Lecture de la MFT. Déjà administrateur : directement. Sinon par l'assistant élevé, qui ne
    /// reçoit que la lettre du volume ; le dossier demandé est extrait ici, sans privilège.
    #[cfg(windows)]
    fn mft(root: &str, cancel: &AtomicBool, on: &(dyn Fn(&Progress) + Sync)) -> Result<Scan, ScanError> {
        if scan::win::is_elevated() {
            return scan::win::mft::scan_mft(root, cancel, on);
        }
        let root = scan::win::root_path(root);
        let drive = root.chars().next().filter(char::is_ascii_alphabetic).ok_or_else(|| ScanError::Mft("racine sans lettre de lecteur".into()))?;
        let volume = scan::win::helper::scan_volume_elevated(drive, cancel, on)?;
        let relative = root.get(3..).unwrap_or("");
        if relative.trim_matches('\\').is_empty() {
            return Ok(volume);
        }
        let id = volume.find(relative).ok_or_else(|| ScanError::Mft(format!("{root} introuvable dans la MFT")))?;
        volume.subtree(id).ok_or_else(|| ScanError::Mft(format!("{root} n'est pas un dossier")))
    }

    #[cfg(windows)]
    pub fn run(root: &str, mft: bool, cancel: &AtomicBool, on: &(dyn Fn(&Progress) + Sync)) -> Result<(Scan, Option<&'static str>), ScanError> {
        let read_mft = mft.then(|| Box::new(move || self::mft(root, cancel, on)) as Box<dyn FnOnce() -> Result<Scan, ScanError> + '_>);
        with_fallback(read_mft, || {
            let root = scan::win::root_path(root);
            scan::walk(&scan::win::WinSource::new(&root), &root, cancel, on)
        })
    }

    #[cfg(not(windows))]
    pub fn run(root: &str, _mft: bool, cancel: &AtomicBool, on: &(dyn Fn(&Progress) + Sync)) -> Result<(Scan, Option<&'static str>), ScanError> {
        with_fallback(None, || scan::walk(&scan::unix::StdSource, root, cancel, on))
    }
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo { elevated: platform::elevated(), fast_scan_available: platform::fast_scan_available(), version: env!("CARGO_PKG_VERSION") }
}

#[tauri::command]
pub fn volumes_list() -> Vec<VolumeDto> {
    platform::volumes()
}

#[tauri::command]
pub fn scan_start(app: AppHandle, state: State<'_, ScanState>, target: ScanTarget, method: ScanMethod) -> Result<u32, LumenError> {
    let volumes = platform::volumes();
    let (root, volume) = match &target {
        ScanTarget::UserProfile => {
            let profile = platform::user_profile().ok_or(LumenError::NoProfile)?;
            let volume: String = profile.chars().take(2).collect();
            (profile, volume)
        }
        ScanTarget::Volume { letter } => {
            // Seule une lettre présente dans la liste des volumes est acceptée.
            let v = volumes.iter().find(|v| v.letter.eq_ignore_ascii_case(letter)).ok_or_else(|| LumenError::UnknownVolume(letter.clone()))?;
            (format!("{}\\", v.letter), v.letter.clone())
        }
    };
    let vol = volumes.iter().find(|v| v.letter.eq_ignore_ascii_case(&volume));
    let mft_capable = vol.is_some_and(|v| v.mft_capable);
    // MFT : seulement sur NTFS local, et seulement si l'appli est déjà élevée ou si l'utilisateur
    // a demandé le scan rapide. Sinon (exFAT, FAT32, réseau…), parcours.
    let use_mft = mft_capable && (method == ScanMethod::Fast || platform::elevated());

    // L'analyse de l'utilisateur passe avant un snapshot quotidien en cours.
    crate::timeline::cancel_daily(&app);
    let cancel = Arc::new(AtomicBool::new(false));
    let id = {
        let mut inner = state.0.lock().map_err(|_| LumenError::ScanRunning)?;
        if inner.running.is_some() {
            return Err(LumenError::ScanRunning);
        }
        inner.next_id += 1;
        let id = inner.next_id;
        inner.running = Some((id, cancel.clone()));
        id
    };

    std::thread::Builder::new()
        .name("lumen-scan".into())
        .spawn(move || {
            let progress = |p: &Progress| {
                let _ = app.emit(
                    "scan:progress",
                    ProgressDto {
                        scan_id: id,
                        files: p.files,
                        dirs: p.dirs,
                        allocated: p.allocated,
                        errors: p.errors,
                        elapsed_ms: p.elapsed_ms,
                        phase: p.phase,
                        tops: p.tops.iter().map(|t| TopDto { name: t.name.clone(), allocated: t.allocated, files: t.files }).collect(),
                    },
                );
            };
            let result = platform::run(&root, use_mft, &cancel, &progress);
            let state = tauri::Manager::state::<ScanState>(&app);
            let mut inner = match state.0.lock() {
                Ok(i) => i,
                Err(_) => return,
            };
            inner.running = None;
            match result {
                Ok((scan, notice)) => {
                    drop(inner);
                    let _ = app.emit(
                        "scan:progress",
                        ProgressDto { scan_id: id, files: scan.stats.files, dirs: scan.stats.dirs, allocated: scan.stats.allocated, errors: scan.stats.errors(), elapsed_ms: scan.stats.elapsed_ms, phase: "rules", tops: Vec::new() },
                    );
                    let applied = analysis::apply(&scan);
                    let mut inner = match state.0.lock() {
                        Ok(i) => i,
                        Err(_) => return,
                    };
                    let entry = Arc::new(Entry { scan, volume, notice, applied });
                    let dto = summary(id, &entry);
                    // Une seule analyse gardée en mémoire : la dernière.
                    inner.results.clear();
                    inner.results.insert(id, entry.clone());
                    drop(inner);
                    let _ = app.emit("scan:done", dto);
                    // Snapshot de la Timeline (tailles agrégées par dossier) ; un échec n'empêche
                    // jamais l'analyse d'être affichée.
                    let _ = crate::timeline::record_scan(&app, &entry.scan, "scan");
                }
                Err(e) => {
                    drop(inner);
                    let _ = app.emit("scan:failed", ScanFailedDto { scan_id: id, cancelled: e == ScanError::Cancelled, message: e.to_string() });
                }
            }
        })
        .map_err(|e| LumenError::Startup(e.to_string()))?;
    Ok(id)
}

#[tauri::command]
pub fn scan_cancel(state: State<'_, ScanState>, scan_id: u32) {
    if let Ok(inner) = state.0.lock() {
        if let Some((id, cancel)) = &inner.running {
            if *id == scan_id {
                cancel.store(true, Ordering::Relaxed);
            }
        }
    }
}

fn entry(state: &State<'_, ScanState>, scan_id: u32) -> Result<Arc<Entry>, LumenError> {
    entry_of(state, scan_id)
}

/// Une analyse de l'utilisateur est-elle en cours ?
pub(crate) fn is_running(state: &ScanState) -> bool {
    state.0.lock().map_or(true, |i| i.running.is_some())
}

pub(crate) fn entry_of(state: &ScanState, scan_id: u32) -> Result<Arc<Entry>, LumenError> {
    state.0.lock().ok().and_then(|i| i.results.get(&scan_id).cloned()).ok_or(LumenError::UnknownScan)
}

#[tauri::command]
pub fn scan_summary(state: State<'_, ScanState>, scan_id: u32) -> Result<SummaryDto, LumenError> {
    let e = entry(&state, scan_id)?;
    Ok(summary(scan_id, &e))
}

#[tauri::command]
pub fn scan_tree(state: State<'_, ScanState>, scan_id: u32, node: Option<i64>) -> Result<TreeSliceDto, LumenError> {
    let e = entry(&state, scan_id)?;
    let s = &e.scan;
    let id = match node {
        None => s.root(),
        Some(n) => u32::try_from(n).ok().filter(|&n| (n as usize) < s.len()).ok_or(LumenError::UnknownNode)?,
    };
    // Profondeur adaptative : jusqu'à 4 niveaux sous les dossiers d'au moins 1 % de la vue,
    // assez grands pour être dessinés avec leurs enfants.
    let min_expand = s.node(id).map_or(0, |n| n.allocated / 100).max(1);
    let slice = s.slice_adaptive(id, MAX_LEVELS, TOP_N, min_expand).ok_or(LumenError::UnknownNode)?;

    // Fil d'Ariane : segments au-dessus de la racine (non cliquables), puis la racine et ses descendants.
    let mut chain = Vec::new();
    let mut cur = id;
    while cur != s.root() {
        chain.push(CrumbDto { label: s.name(cur).to_owned(), id: Some(i64::from(cur)) });
        cur = s.node(cur).map_or(s.root(), |n| n.parent);
    }
    chain.reverse();
    let root_path = s.root_path.trim_end_matches('\\');
    let mut parts: Vec<&str> = root_path.split('\\').filter(|p| !p.is_empty()).collect();
    let root_label = parts.pop().unwrap_or(root_path).to_owned();
    let mut breadcrumb: Vec<CrumbDto> = parts.into_iter().map(|p| CrumbDto { label: p.to_owned(), id: None }).collect();
    breadcrumb.push(CrumbDto { label: root_label, id: Some(0) });
    breadcrumb.extend(chain);
    Ok(TreeSliceDto { root: to_dto(slice, e.applied.as_ref(), s), breadcrumb })
}

/// Nœud d'un chemin complet dans l'analyse (historique de navigation : les identifiants changent
/// d'une analyse à l'autre, pas les chemins). `None` s'il n'existe plus ou est hors de l'analyse.
#[tauri::command]
pub fn scan_find(state: State<'_, ScanState>, scan_id: u32, path: String) -> Result<Option<i64>, LumenError> {
    let e = entry(&state, scan_id)?;
    Ok(find_path(&e.scan, &path).map(i64::from))
}

pub(crate) fn find_path(s: &Scan, path: &str) -> Option<NodeId> {
    let root = s.root_path.trim_end_matches('\\');
    let path = path.trim_end_matches('\\');
    if path.eq_ignore_ascii_case(root) {
        return Some(s.root());
    }
    let head = path.get(..root.len())?;
    let rest = path.get(root.len()..)?.strip_prefix('\\')?;
    if !head.eq_ignore_ascii_case(root) {
        return None;
    }
    s.find(rest)
}

#[tauri::command]
pub fn recommendations(state: State<'_, ScanState>, scan_id: u32) -> Result<Vec<RecoDto>, LumenError> {
    let e = entry(&state, scan_id)?;
    Ok(e.applied.as_ref().map(|a| a.recommendations(&e.scan)).unwrap_or_default())
}

#[tauri::command]
pub fn recommendation_detail(state: State<'_, ScanState>, scan_id: u32, id: String) -> Result<RecoDetailDto, LumenError> {
    let e = entry(&state, scan_id)?;
    e.applied.as_ref().and_then(|a| a.detail(&e.scan, &id)).ok_or(LumenError::UnknownReco(id))
}

/// KPI des règles ; `None` tant qu'aucune règle n'est appliquée (hors Windows, règles invalides).
#[tauri::command]
pub fn rule_kpis(state: State<'_, ScanState>, scan_id: u32) -> Result<Option<RuleKpisDto>, LumenError> {
    let e = entry(&state, scan_id)?;
    Ok(e.applied.as_ref().map(Applied::kpis))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_et_types() {
        assert_eq!(epoch_ms(116_444_736_000_000_000), 0);
        assert_eq!(epoch_ms(0), 0);
        assert_eq!(kind_name(Kind::CloudFile), "cloudFile");
    }

    #[test]
    fn regroupement_autres_statut_de_son_contenu() {
        let (a, scan) = crate::analysis::tests::applied();
        let local = scan.find("AppData\\Local").unwrap();
        // Un seul enfant montré : l'autre (D3DSCache) part dans « autres ».
        let slice = scan.slice(local, 2, 1).unwrap();
        let dto = to_dto(slice, Some(&a), &scan);
        let others = dto.children.iter().find(|c| c.others.is_some()).unwrap();
        // (le dossier D3DSCache lui-même n'est pas couvert : seul son contenu l'est)
        assert_eq!((others.status, others.category, others.gain, others.gain_status), ("keep", "tools", 0, None));
        // Dans D3DSCache : les fichiers regroupés sont « À vérifier » (onglet Jeux), comme leur contenu.
        let d3d = scan.find("AppData\\Local\\D3DSCache").unwrap();
        let dto = to_dto(scan.slice(d3d, 1, 1).unwrap(), Some(&a), &scan);
        let others = dto.children.iter().find(|c| c.others.is_some()).unwrap();
        assert_eq!((others.status, others.category), ("review", "games"));
        // Cache npm et shaders regroupés : statut majoritaire en octets (npm, sans risque), gain additionné.
        let npm = scan.find("AppData\\Local\\npm-cache").unwrap();
        let v = a.aggregate(&scan, &[npm, d3d]);
        assert_eq!((v.status, v.category, v.gain, v.gain_status), ("safe", "packages", 1000, Some("safe")));
        assert_eq!(a.aggregate(&scan, &[npm]).reco, a.node(npm).reco, "recommandation commune");
    }

    #[test]
    fn noeud_retrouve_par_son_chemin() {
        let (_, scan) = crate::analysis::tests::applied();
        let npm = scan.find("AppData\\Local\\npm-cache").unwrap();
        assert_eq!(find_path(&scan, "C:\\Users\\Arthur\\AppData\\Local\\npm-cache"), Some(npm));
        assert_eq!(find_path(&scan, "c:\\users\\arthur\\appdata\\local\\NPM-CACHE\\"), Some(npm));
        assert_eq!(find_path(&scan, "C:\\Users\\Arthur"), Some(scan.root()));
        assert_eq!(find_path(&scan, "C:\\Users\\Arthur\\Absent"), None);
        assert_eq!(find_path(&scan, "D:\\Users\\Arthur\\AppData"), None);
        assert_eq!(find_path(&scan, "C:\\Users\\ArthurBis"), None);
    }

    fn tiny() -> Scan {
        let root = scan::RawEntry { name: "C:".into(), kind: Kind::Dir, attributes: 0, file_id: 5, logical: 0, allocated: 0, modified: 0, accessed: 0 };
        scan::model::Builder::new(&root).finish("C:\\".into(), 1, scan::Method::Walk, false, scan::Stats::default())
    }

    #[test]
    fn mft_fragmentee_bascule_sur_le_parcours() {
        let unsupported: Box<dyn FnOnce() -> Result<Scan, ScanError>> = Box::new(|| Err(ScanError::MftUnsupported("liste d'attributs".into())));
        let (_, notice) = with_fallback(Some(unsupported), || Ok(tiny())).unwrap();
        assert_eq!(notice, Some("mftFragmented"));
        // Toute autre erreur de la MFT n'est pas masquée par le parcours.
        let refused: Box<dyn FnOnce() -> Result<Scan, ScanError>> = Box::new(|| Err(ScanError::NotElevated));
        assert_eq!(with_fallback(Some(refused), || Ok(tiny())).err(), Some(ScanError::NotElevated));
        let ok: Box<dyn FnOnce() -> Result<Scan, ScanError>> = Box::new(|| Ok(tiny()));
        assert_eq!(with_fallback(Some(ok), || Err(ScanError::Cancelled)).unwrap().1, None);
        assert_eq!(with_fallback(None, || Ok(tiny())).unwrap().1, None);
    }

    #[test]
    fn cibles_deserialisees() {
        let t: ScanTarget = serde_json::from_str(r#"{"kind":"volume","letter":"D:"}"#).unwrap();
        assert_eq!(t, ScanTarget::Volume { letter: "D:".into() });
        let t: ScanTarget = serde_json::from_str(r#"{"kind":"userProfile"}"#).unwrap();
        assert_eq!(t, ScanTarget::UserProfile);
        // Aucun chemin libre accepté.
        assert!(serde_json::from_str::<ScanTarget>(r#"{"kind":"path","path":"C:\\Windows"}"#).is_err());
    }
}
