//! Commandes IPC du nettoyage avancé. Le front n'envoie que des identifiants de nœuds proposés
//! par la dernière recherche (dormants, gros fichiers, doublons) ou des actions système de la
//! liste fermée ; les sélections passent ensuite par le même plan `safety` que les
//! recommandations (`cleanup::prepare`).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use safety::allow::{Allowlist, Authority, DupGroup, DupMember, DuplicateSelection, Selection as AllowSelection};
use safety::check::Target;
use safety::commands::{Drive, OfficialCommand};
use safety::path::WinPath;
use safety::plan::PlannedCommand;
use scan::{NodeId, Scan};

use super::duplicates::{self, DuplicatesDto, Found, ProgressDto};
use super::{dormant, files::FileKind, largest};
use crate::cleanup::{self, CleanState, ModeDto, PlanDto, Selection};
use crate::error::LumenError;
use crate::scanning::ScanState;

/// Identifiant de la sélection dans l'allowlist d'un plan (une sélection par plan).
const SELECTION_ID: u64 = 1;
const DEFAULT_MONTHS: u32 = 12;
const MB: u64 = 1 << 20;

#[derive(Default)]
struct Inner {
    /// Nœuds proposés par la dernière recherche, par analyse : seuls ceux-là sont sélectionnables.
    dormant: Option<(u32, HashSet<NodeId>)>,
    largest: Option<(u32, HashSet<NodeId>)>,
    duplicates: Option<(u32, Arc<Found>)>,
    searching: Option<(u32, Arc<AtomicBool>)>,
}

#[derive(Default)]
pub struct AdvancedState(Mutex<Inner>);

impl AdvancedState {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Inner>, LumenError> {
        self.0.lock().map_err(|_| LumenError::Plan("état indisponible".into()))
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SystemAction {
    /// `DISM /Online /Cleanup-Image /StartComponentCleanup` (sans `/ResetBase`).
    Components,
    HibernateOff,
    HibernateReduced,
    /// `vssadmin delete shadows /for=C: /oldest` : jamais `/all`.
    OldestRestorePoint,
    /// Nettoyage de disque (Windows.old), interactif.
    WindowsOld,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AdvancedRequest {
    Dormant { ids: Vec<i64> },
    Largest { ids: Vec<i64> },
    Duplicates { ids: Vec<i64> },
    System {
        actions: Vec<SystemAction>,
        #[serde(rename = "restorePoint")]
        restore_point: bool,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SinksDto {
    /// Faits lus sur ce PC (faux hors de Windows).
    pub available: bool,
    pub system_drive: String,
    pub hiberfil: Option<u64>,
    pub pagefile: Option<u64>,
    pub swapfile: Option<u64>,
    pub hibernate_enabled: Option<bool>,
    pub fast_startup: Option<bool>,
    pub windows_old: bool,
    pub restore_point_frequency_min: u32,
    pub last_access_raw: Option<u32>,
    pub last_access_updated: bool,
    pub free: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DupProgressEvent {
    scan_id: u32,
    #[serde(flatten)]
    progress: ProgressDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DupDoneEvent {
    scan_id: u32,
    #[serde(flatten)]
    result: DuplicatesDto,
}

pub(crate) mod platform {
    use super::*;
    use crate::advanced::duplicates::{ContentReader, ReadError};
    use safety::denylist::DenyList;

    #[cfg(windows)]
    pub fn deny() -> Option<DenyList> {
        safety::win::system_layout().ok().map(|l| DenyList::new(&l))
    }

    #[cfg(not(windows))]
    pub fn deny() -> Option<DenyList> {
        None
    }

    #[cfg(windows)]
    pub fn last_access() -> (Option<u32>, bool) {
        safety::win::sinks::last_access_update()
    }

    #[cfg(not(windows))]
    pub fn last_access() -> (Option<u32>, bool) {
        (None, false)
    }

    #[cfg(windows)]
    pub fn sinks() -> SinksDto {
        let f = safety::win::sinks::system_sinks();
        let root = WinPath::parse(&format!("{}:\\", f.system_drive)).ok();
        SinksDto {
            available: true,
            system_drive: format!("{}:", f.system_drive),
            hiberfil: f.hiberfil,
            pagefile: f.pagefile,
            swapfile: f.swapfile,
            hibernate_enabled: f.hibernate_enabled,
            fast_startup: f.fast_startup,
            windows_old: f.windows_old,
            restore_point_frequency_min: f.restore_point_frequency_min,
            last_access_raw: f.last_access_raw,
            last_access_updated: f.last_access_updated,
            free: root.and_then(|r| safety::exec::Effects::free_bytes(&safety::win::WinEffects::new(), &r)),
        }
    }

    #[cfg(not(windows))]
    pub fn sinks() -> SinksDto {
        SinksDto {
            available: false,
            system_drive: "C:".into(),
            hiberfil: None,
            pagefile: None,
            swapfile: None,
            hibernate_enabled: None,
            fast_startup: None,
            windows_old: false,
            restore_point_frequency_min: 1440,
            last_access_raw: None,
            last_access_updated: false,
            free: None,
        }
    }

    /// Lecture réelle : `safety::win::content` (dernier accès gelé, E/S basse priorité, jamais
    /// un fichier cloud). Même empreinte que celle que `safety` recalcule avant la suppression.
    pub struct Reader;

    #[cfg(windows)]
    impl ContentReader for Reader {
        fn edges(&self, path: &str, cancel: &AtomicBool) -> Result<[u8; 32], ReadError> {
            read(path, safety::win::content::Digest::Edges, cancel)
        }
        fn full(&self, path: &str, cancel: &AtomicBool) -> Result<[u8; 32], ReadError> {
            read(path, safety::win::content::Digest::Full, cancel)
        }
    }

    #[cfg(windows)]
    fn read(path: &str, digest: safety::win::content::Digest, cancel: &AtomicBool) -> Result<[u8; 32], ReadError> {
        use safety::probe::ProbeError;
        let p = WinPath::parse(path).map_err(|e| ReadError::Other(e.to_string()))?;
        safety::win::content::hash_file(&p, digest, Some(cancel)).map_err(|e| match e {
            ProbeError::Cloud => ReadError::Cloud,
            ProbeError::AccessTimeNotFrozen => ReadError::NotFrozen,
            ProbeError::Cancelled => ReadError::Cancelled,
            other => ReadError::Other(other.to_string()),
        })
    }

    #[cfg(not(windows))]
    impl ContentReader for Reader {
        fn edges(&self, _: &str, _: &AtomicBool) -> Result<[u8; 32], ReadError> {
            Err(ReadError::Other("Windows uniquement".into()))
        }
        fn full(&self, _: &str, _: &AtomicBool) -> Result<[u8; 32], ReadError> {
            Err(ReadError::Other("Windows uniquement".into()))
        }
    }
}

fn node_ids(ids: &[i64], allowed: &HashSet<NodeId>) -> Result<Vec<NodeId>, LumenError> {
    ids.iter()
        .map(|&id| NodeId::try_from(id).ok().filter(|n| allowed.contains(n)).ok_or(LumenError::UnknownNode))
        .collect()
}

fn names(fr: &str, en: &str) -> HashMap<String, (String, String)> {
    HashMap::from([(SELECTION_ID.to_string(), (fr.to_owned(), en.to_owned()))])
}

/// Fichiers cochés parmi ceux proposés : chemins exacts autorisés un par un.
pub(crate) fn select_files(scan: &Scan, ids: &[NodeId], label: (&str, &str)) -> Result<Selection, LumenError> {
    let mut targets = Vec::new();
    let mut members = Vec::new();
    for &id in ids {
        let path = scan.path(id);
        let expected = cleanup::expected(scan, id).ok_or(LumenError::UnknownNode)?;
        members.push(WinPath::parse(&path).map_err(|e| LumenError::Plan(e.to_string()))?);
        targets.push(Target { path, authority: Authority::Selection(SELECTION_ID), expected, bytes: cleanup::bytes_of(scan, id), contents: None, hash: None });
    }
    let mut allow = Allowlist::new();
    allow.add_selection(AllowSelection { id: SELECTION_ID, members });
    Ok(Selection { targets, allow, commands: Vec::new(), rule_names: names(label.0, label.1), elevated: false })
}

/// Exemplaires cochés : chaque groupe concerné est transmis en entier à `safety` (empreinte et
/// identité de chaque exemplaire), qui en garde toujours au moins un et recalcule l'empreinte
/// juste avant la suppression.
pub(crate) fn select_duplicates(scan: &Scan, found: &Found, ids: &[i64]) -> Result<Selection, LumenError> {
    let mut targets = Vec::new();
    let mut groups: Vec<usize> = Vec::new();
    for &raw in ids {
        let id = NodeId::try_from(raw).map_err(|_| LumenError::UnknownNode)?;
        let gi = found.groups.iter().position(|g| g.members.contains(&id)).ok_or(LumenError::UnknownNode)?;
        let expected = cleanup::expected(scan, id).ok_or(LumenError::UnknownNode)?;
        targets.push(Target {
            path: scan.path(id),
            authority: Authority::Duplicates(SELECTION_ID),
            expected,
            bytes: cleanup::bytes_of(scan, id),
            contents: None,
            hash: Some(found.groups[gi].hash),
        });
        if !groups.contains(&gi) {
            groups.push(gi);
        }
    }
    let mut dup_groups = Vec::new();
    for gi in groups {
        let g = &found.groups[gi];
        let picked = g.members.iter().filter(|m| ids.contains(&i64::from(**m))).count();
        if picked >= g.members.len() {
            return Err(LumenError::Plan("chaque groupe de doublons garde au moins un exemplaire".into()));
        }
        let mut members = Vec::new();
        for &m in &g.members {
            let path = WinPath::parse(&scan.path(m)).map_err(|e| LumenError::Plan(e.to_string()))?;
            let expected = cleanup::expected(scan, m).ok_or(LumenError::UnknownNode)?;
            members.push(DupMember { path, expected, hash: g.hash });
        }
        dup_groups.push(DupGroup { members });
    }
    let mut allow = Allowlist::new();
    allow.add_duplicates(DuplicateSelection { id: SELECTION_ID, groups: dup_groups });
    Ok(Selection { targets, allow, commands: Vec::new(), rule_names: names("Doublons", "Duplicates"), elevated: false })
}

/// Actions système : commandes officielles élevées, mesurées sur le lecteur système, après un
/// point de restauration obligatoire s'il est demandé.
pub(crate) fn select_system(drive: char, hiberfil: Option<u64>, actions: &[SystemAction], restore_point: bool) -> Result<Selection, LumenError> {
    if actions.is_empty() {
        return Err(LumenError::Plan("aucune action système choisie".into()));
    }
    if actions.contains(&SystemAction::HibernateOff) && actions.contains(&SystemAction::HibernateReduced) {
        return Err(LumenError::Plan("hibernation : choisir désactivée ou réduite, pas les deux".into()));
    }
    let d = Drive::parse(&drive.to_string()).map_err(|e| LumenError::Plan(e.to_string()))?;
    let root = WinPath::parse(&format!("{drive}:\\")).map_err(|e| LumenError::Plan(e.to_string()))?;
    let mut commands = Vec::new();
    if restore_point {
        commands.push(PlannedCommand::prerequisite(OfficialCommand::RestorePointCreate, true));
    }
    let mut seen = Vec::new();
    for &a in actions {
        if seen.contains(&a) {
            continue;
        }
        seen.push(a);
        let (command, bytes) = match a {
            SystemAction::Components => (OfficialCommand::DismComponentCleanup, 0),
            SystemAction::HibernateOff => (OfficialCommand::HibernateOff, hiberfil.unwrap_or(0)),
            SystemAction::HibernateReduced => (OfficialCommand::HibernateReduced, 0),
            SystemAction::OldestRestorePoint => (OfficialCommand::ShadowDeleteOldest(d), 0),
            SystemAction::WindowsOld => (OfficialCommand::DiskCleanup(d), 0),
        };
        commands.push(PlannedCommand::action(command, Some(root.clone()), bytes));
    }
    Ok(Selection { targets: Vec::new(), allow: Allowlist::new(), commands, rule_names: HashMap::new(), elevated: true })
}

fn display_of(entry: &crate::scanning::Entry) -> impl Fn(&str) -> String + '_ {
    move |p: &str| entry.applied.as_ref().map_or_else(|| p.to_owned(), |a| a.display(p))
}

/// Fichiers dormants (« À vérifier ») : dernier accès si Windows le tient à jour, sinon
/// dernière modification, ce que l'interface affiche.
#[tauri::command]
pub fn dormant_find(scans: State<'_, ScanState>, state: State<'_, AdvancedState>, scan_id: u32, months: Option<u32>) -> Result<dormant::DormantDto, LumenError> {
    let entry = crate::scanning::entry_of(&scans, scan_id)?;
    let deny = platform::deny();
    let (raw, updated) = platform::last_access();
    let covered = |id: NodeId| entry.applied.as_ref().is_some_and(|a| a.node(id).reco.is_some());
    let display = display_of(&entry);
    let found = dormant::find(&dormant::Inputs {
        scan: &entry.scan,
        deny: deny.as_ref(),
        now: crate::analysis::now(),
        months: months.unwrap_or(DEFAULT_MONTHS).clamp(1, 120),
        last_access_updated: updated,
        last_access_raw: raw,
        covered: &covered,
        display: &display,
    });
    state.lock()?.dormant = Some((scan_id, found.eligible.into_iter().collect()));
    Ok(found.dto)
}

/// Les 100 plus gros fichiers (« À vérifier »), filtrables par type.
#[tauri::command]
pub fn largest_files(scans: State<'_, ScanState>, state: State<'_, AdvancedState>, scan_id: u32, kind: Option<String>) -> Result<largest::LargestDto, LumenError> {
    let entry = crate::scanning::entry_of(&scans, scan_id)?;
    let kind = kind.as_deref().map(|k| FileKind::parse(k).ok_or(LumenError::UnknownNode)).transpose()?;
    let deny = platform::deny();
    let found = largest::find(&entry.scan, deny.as_ref(), kind, &display_of(&entry));
    let mut inner = state.lock()?;
    // Changer de filtre ne retire pas ce qui a déjà été proposé pour cette analyse.
    match &mut inner.largest {
        Some((id, set)) if *id == scan_id => set.extend(found.eligible),
        slot => *slot = Some((scan_id, found.eligible.into_iter().collect())),
    }
    Ok(found.dto)
}

/// Recherche des doublons en arrière-plan : `dup:progress`, puis `dup:done` ou `dup:failed`.
#[tauri::command]
pub fn duplicates_start(app: AppHandle, scans: State<'_, ScanState>, state: State<'_, AdvancedState>, scan_id: u32, min_size_mb: Option<u64>) -> Result<(), LumenError> {
    if !cfg!(windows) {
        return Err(LumenError::Plan("recherche de doublons disponible sous Windows uniquement".into()));
    }
    let entry = crate::scanning::entry_of(&scans, scan_id)?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut inner = state.lock()?;
        if inner.searching.is_some() {
            return Err(LumenError::Plan("une recherche de doublons est déjà en cours".into()));
        }
        inner.searching = Some((scan_id, cancel.clone()));
    }
    let min_size = min_size_mb.unwrap_or(1).clamp(1, 100_000) * MB;
    let spawned = std::thread::Builder::new().name("lumen-duplicates".into()).spawn(move || {
        let scan = &entry.scan;
        let cache = crate::cleanup::with_journal(&app, |j| j.hash_cache(scan.volume_serial)).unwrap_or_default();
        let deny = platform::deny();
        let mut last = Instant::now() - Duration::from_secs(1);
        let mut phase = "";
        let on_progress = |p: ProgressDto| {
            if p.phase != phase || p.done == p.total || last.elapsed() >= Duration::from_millis(100) {
                phase = p.phase;
                last = Instant::now();
                let _ = app.emit("dup:progress", DupProgressEvent { scan_id, progress: p });
            }
        };
        let result = duplicates::find(
            &duplicates::Inputs { scan, deny: deny.as_ref(), min_size, reader: &platform::Reader, cache: &cache, cancel: &cancel },
            on_progress,
        );
        let state = app.state::<AdvancedState>();
        match result {
            Ok(found) => {
                if let Err(e) = crate::cleanup::with_journal(&app, |j| j.store_hashes(&found.cache, crate::cleanup::unix_ms())) {
                    eprintln!("cache des empreintes : {e}");
                }
                let dto = duplicates::dto(scan, &found, &display_of(&entry));
                if let Ok(mut inner) = state.0.lock() {
                    inner.duplicates = Some((scan_id, Arc::new(found)));
                    inner.searching = None;
                }
                let _ = app.emit("dup:done", DupDoneEvent { scan_id, result: dto });
            }
            Err(e) => {
                if let Ok(mut inner) = state.0.lock() {
                    inner.searching = None;
                }
                let cancelled = e == duplicates::ReadError::Cancelled;
                let message = if cancelled { "recherche annulée".to_owned() } else { format!("{e:?}") };
                let _ = app.emit("dup:failed", serde_json::json!({ "scanId": scan_id, "cancelled": cancelled, "message": message }));
            }
        }
    });
    if let Err(e) = spawned {
        state.lock()?.searching = None;
        return Err(LumenError::Startup(e.to_string()));
    }
    Ok(())
}

/// Annulation : prise en compte entre deux blocs lus.
#[tauri::command]
pub fn duplicates_cancel(state: State<'_, AdvancedState>, scan_id: u32) {
    if let Ok(inner) = state.0.lock() {
        if let Some((id, cancel)) = &inner.searching {
            if *id == scan_id {
                cancel.store(true, Ordering::SeqCst);
            }
        }
    }
}

/// Gouffres système : tailles et réglages lus sans droits d'administrateur.
#[tauri::command]
pub fn system_sinks() -> SinksDto {
    platform::sinks()
}

/// Plan d'une sélection du nettoyage avancé, à confirmer (rien n'est fait ici).
#[tauri::command]
pub fn advanced_plan(
    scans: State<'_, ScanState>,
    state: State<'_, AdvancedState>,
    clean: State<'_, CleanState>,
    scan_id: u32,
    request: AdvancedRequest,
    mode: ModeDto,
) -> Result<PlanDto, LumenError> {
    let mode = cleanup::delete_mode(mode);
    if let AdvancedRequest::System { actions, restore_point } = &request {
        let sinks = platform::sinks();
        let drive = sinks.system_drive.chars().next().unwrap_or('C');
        let sel = select_system(drive, sinks.hiberfil, actions, *restore_point)?;
        return cleanup::prepare(&clean, &sel, mode, "Gouffres système".into(), "system", &|p| p.to_owned());
    }
    let entry = crate::scanning::entry_of(&scans, scan_id)?;
    let display = display_of(&entry);
    let inner = state.lock()?;
    let for_scan = |slot: &Option<(u32, HashSet<NodeId>)>| slot.as_ref().filter(|(id, _)| *id == scan_id).map(|(_, s)| s.clone()).unwrap_or_default();
    let (sel, title, kind) = match &request {
        AdvancedRequest::Dormant { ids } => {
            let ids = node_ids(ids, &for_scan(&inner.dormant))?;
            (select_files(&entry.scan, &ids, ("Fichiers dormants", "Dormant files"))?, "Fichiers dormants", "dormant")
        }
        AdvancedRequest::Largest { ids } => {
            let ids = node_ids(ids, &for_scan(&inner.largest))?;
            (select_files(&entry.scan, &ids, ("Gros fichiers", "Large files"))?, "Gros fichiers", "largest")
        }
        AdvancedRequest::Duplicates { ids } => {
            let found = inner.duplicates.as_ref().filter(|(id, _)| *id == scan_id).map(|(_, f)| f.clone()).ok_or(LumenError::UnknownNode)?;
            (select_duplicates(&entry.scan, &found, ids)?, "Doublons", "duplicates")
        }
        AdvancedRequest::System { .. } => return Err(LumenError::Plan("requête inattendue".into())),
    };
    drop(inner);
    if sel.targets.is_empty() {
        return Err(LumenError::Plan("aucun élément choisi".into()));
    }
    cleanup::prepare(&clean, &sel, mode, title.into(), kind, &display)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::advanced::duplicates::{Group, StatsDto};
    use safety::plan::CommandRole;
    use scan::model::Builder;
    use scan::{Kind, Method, RawEntry, Stats};

    fn e(name: &str, kind: Kind, size: u64, id: u128) -> RawEntry {
        RawEntry { name: name.into(), kind, attributes: 0, file_id: id, logical: size, allocated: size, modified: 5, accessed: 5 }
    }

    fn scan() -> Scan {
        let mut b = Builder::new(&e("Arthur", Kind::Dir, 0, 1));
        b.add_children(0, &[e("a.mp4", Kind::File, 100, 10), e("b.mp4", Kind::File, 100, 11), e("c.mp4", Kind::File, 100, 12)]);
        b.finish("C:\\Users\\Arthur".into(), 7, Method::Walk, true, Stats::default())
    }

    #[test]
    fn selection_de_fichiers_chemins_exacts() {
        let s = scan();
        let a = s.find("a.mp4").unwrap();
        let sel = select_files(&s, &[a], ("Gros fichiers", "Large files")).unwrap();
        assert_eq!(sel.targets.len(), 1);
        let path = WinPath::parse("C:\\Users\\Arthur\\a.mp4").unwrap();
        assert!(sel.allow.permits(&path, &Authority::Selection(SELECTION_ID)).is_some());
        assert!(sel.allow.permits(&WinPath::parse("C:\\Users\\Arthur\\b.mp4").unwrap(), &Authority::Selection(SELECTION_ID)).is_none());
        assert!(!sel.elevated);
        // Un nœud non proposé par la dernière recherche est refusé.
        assert!(matches!(node_ids(&[i64::from(a)], &HashSet::new()), Err(LumenError::UnknownNode)));
        assert!(matches!(node_ids(&[-1], &HashSet::from([a])), Err(LumenError::UnknownNode)));
    }

    #[test]
    fn doublons_groupe_entier_transmis_jamais_tous_les_exemplaires() {
        let s = scan();
        let ids: Vec<NodeId> = ["a.mp4", "b.mp4", "c.mp4"].iter().map(|n| s.find(n).unwrap()).collect();
        let found = Found { groups: vec![Group { hash: [9; 32], size: 100, members: ids.clone() }], stats: StatsDto::default(), cache: Vec::new() };
        let sel = select_duplicates(&s, &found, &[i64::from(ids[1]), i64::from(ids[2])]).unwrap();
        assert_eq!(sel.targets.len(), 2);
        assert!(sel.targets.iter().all(|t| t.hash == Some([9; 32]) && t.authority == Authority::Duplicates(SELECTION_ID)));
        let keeper = WinPath::parse("C:\\Users\\Arthur\\a.mp4").unwrap();
        assert_eq!(sel.allow.duplicate_group(SELECTION_ID, &keeper).map(|g| g.members.len()), Some(3));
        let all: Vec<i64> = ids.iter().map(|&i| i64::from(i)).collect();
        assert!(matches!(select_duplicates(&s, &found, &all), Err(LumenError::Plan(_))));
        assert!(matches!(select_duplicates(&s, &found, &[9999]), Err(LumenError::UnknownNode)));
    }

    #[test]
    fn actions_systeme_elevees_apres_le_point_de_restauration() {
        let sel = select_system('C', Some(6 << 30), &[SystemAction::Components, SystemAction::HibernateOff, SystemAction::OldestRestorePoint, SystemAction::WindowsOld], true).unwrap();
        assert!(sel.elevated && sel.targets.is_empty());
        let first = &sel.commands[0];
        assert_eq!((first.command.clone(), first.role, first.required), (OfficialCommand::RestorePointCreate, CommandRole::Prerequisite, true));
        let lines: Vec<String> = sel.commands[1..].iter().map(|c| c.command.args().join(" ")).collect();
        assert_eq!(lines[0], "/Online /Cleanup-Image /StartComponentCleanup", "jamais /ResetBase");
        assert_eq!(lines[2], "delete shadows /for=C: /oldest /quiet", "jamais /all");
        assert!(sel.commands[1..].iter().all(|c| c.role == CommandRole::Action && c.measured.as_ref().map(WinPath::display_string).as_deref() == Some("C:\\")));
        assert_eq!(sel.commands[2].bytes, 6 << 30, "gain estimé de l'hibernation : hiberfil.sys");
        assert!(select_system('C', None, &[], true).is_err());
        assert!(select_system('C', None, &[SystemAction::HibernateOff, SystemAction::HibernateReduced], false).is_err());
        let sans = select_system('C', None, &[SystemAction::Components], false).unwrap();
        assert_eq!(sans.commands.len(), 1);
    }
}
