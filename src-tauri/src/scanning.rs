//! Commandes IPC de l'analyse. Le front ne transmet jamais de chemin : une cible (« dossier
//! utilisateur » ou une lettre de lecteur présente dans la liste des volumes), puis des
//! identifiants de nœuds de l'analyse. Il ne reçoit que des tranches agrégées.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

use crate::error::LumenError;
use scan::{Kind, Progress, Scan, ScanError, SliceNode};

/// Enfants les plus gros envoyés par niveau ; le reste est regroupé en « autres ».
const TOP_N: usize = 30;

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

struct Entry {
    scan: Scan,
    volume: String,
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

fn to_dto(n: SliceNode) -> TreeNodeDto {
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
        children: n.children.into_iter().map(to_dto).collect(),
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
    }
}

mod platform {
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
    pub fn run(root: &str, mft: bool, cancel: &AtomicBool, on: &(dyn Fn(&Progress) + Sync)) -> Result<Scan, ScanError> {
        if mft {
            if scan::win::is_elevated() {
                scan::win::mft::scan_mft(root, cancel, on)
            } else {
                scan::win::helper::scan_mft_elevated(root, cancel, on)
            }
        } else {
            let root = scan::win::long_path(root);
            scan::walk(&scan::win::WinSource::new(&root), &root, cancel, on)
        }
    }

    #[cfg(not(windows))]
    pub fn run(root: &str, _mft: bool, cancel: &AtomicBool, on: &(dyn Fn(&Progress) + Sync)) -> Result<Scan, ScanError> {
        scan::walk(&scan::unix::StdSource, root, cancel, on)
    }
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo { elevated: platform::elevated(), version: env!("CARGO_PKG_VERSION") }
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
                Ok(scan) => {
                    let entry = Arc::new(Entry { scan, volume });
                    let dto = summary(id, &entry);
                    // Une seule analyse gardée en mémoire : la dernière.
                    inner.results.clear();
                    inner.results.insert(id, entry);
                    drop(inner);
                    let _ = app.emit("scan:done", dto);
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
    let slice = s.slice(id, 2, TOP_N).ok_or(LumenError::UnknownNode)?;

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
    Ok(TreeSliceDto { root: to_dto(slice), breadcrumb })
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
    fn cibles_deserialisees() {
        let t: ScanTarget = serde_json::from_str(r#"{"kind":"volume","letter":"D:"}"#).unwrap();
        assert_eq!(t, ScanTarget::Volume { letter: "D:".into() });
        let t: ScanTarget = serde_json::from_str(r#"{"kind":"userProfile"}"#).unwrap();
        assert_eq!(t, ScanTarget::UserProfile);
        // Aucun chemin libre accepté.
        assert!(serde_json::from_str::<ScanTarget>(r#"{"kind":"path","path":"C:\\Windows"}"#).is_err());
    }
}
