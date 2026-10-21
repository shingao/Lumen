//! Commandes IPC du nettoyage avancé. Le front n'envoie que des identifiants de nœuds proposés
//! par la dernière recherche (dormants, gros fichiers, doublons) ou des actions système de la
//! liste fermée ; les sélections passent ensuite par le même plan `safety` que les
//! recommandations (`cleanup::prepare`).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use safety::allow::{Allowlist, Authority, DupGroup, DupMember, DuplicateSelection, Selection as AllowSelection};
use safety::check::Target;
use safety::commands::{oldest_deletable, oldest_not_after, Drive, OfficialCommand, OutputFile, ShadowCopy, ShadowId};
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
    /// Points de restauration lus (en administrateur) : seule base de « supprimer le plus ancien ».
    restore: Option<RestoreFacts>,
}

/// Début de la session de Lumen (secondes depuis 1970) : un point de restauration créé depuis
/// n'est jamais supprimé.
static SESSION_START: OnceLock<i64> = OnceLock::new();

pub fn mark_session_start() {
    SESSION_START.get_or_init(unix_secs);
}

fn session_start() -> i64 {
    *SESSION_START.get_or_init(unix_secs)
}

fn unix_secs() -> i64 {
    crate::cleanup::unix_ms() / 1000
}

/// `20261021-120005` (UTC), pour nommer un fichier.
pub(crate) fn stamp(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    // Jours depuis 1970 → date civile (algorithme de Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
}

/// Clichés instantanés et points de restauration d'un lecteur, lus en administrateur.
#[derive(Debug, Clone)]
pub(crate) struct RestoreFacts {
    pub drive: char,
    pub shadows: Vec<ShadowCopy>,
    /// Description et date (secondes) des points de restauration.
    pub points: Vec<(String, i64)>,
}

impl RestoreFacts {
    /// JSON écrit par la commande de lecture (`shadows`, `points`), BOM toléré.
    #[cfg_attr(not(windows), allow(dead_code))] // lu seulement sous Windows (et dans les tests)
    pub(crate) fn parse(drive: char, json: &str) -> Result<RestoreFacts, String> {
        let v: serde_json::Value = serde_json::from_str(json.trim_start_matches('\u{feff}')).map_err(|e| e.to_string())?;
        let list = |key: &str| -> Vec<serde_json::Value> {
            match v.get(key) {
                Some(serde_json::Value::Array(a)) => a.clone(),
                Some(o @ serde_json::Value::Object(_)) => vec![o.clone()],
                _ => Vec::new(),
            }
        };
        let mut shadows = Vec::new();
        for s in list("shadows") {
            let id = s.get("id").and_then(|x| x.as_str()).ok_or("cliché sans identifiant")?;
            let created = s.get("created").and_then(serde_json::Value::as_i64).ok_or("cliché sans date")?;
            shadows.push(ShadowCopy { id: ShadowId::parse(id).map_err(|e| e.to_string())?, created });
        }
        let points = list("points")
            .iter()
            .filter_map(|p| Some((p.get("description")?.as_str()?.to_owned(), p.get("created")?.as_i64()?)))
            .collect();
        Ok(RestoreFacts { drive, shadows, points })
    }

    /// Description du point de restauration correspondant à un cliché (même date, à 10 min près).
    fn description_of(&self, created: i64) -> Option<String> {
        self.points.iter().filter(|(_, c)| (c - created).abs() <= 600).min_by_key(|(_, c)| (c - created).abs()).map(|(d, _)| d.clone())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePointDto {
    pub id: String,
    /// Millisecondes depuis 1970.
    pub created: i64,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestorePointsDto {
    pub drive: String,
    pub count: usize,
    pub points: Vec<RestorePointDto>,
    /// Le point qui serait supprimé (le plus ancien), s'il y en a un.
    pub oldest: Option<RestorePointDto>,
    pub deletable: bool,
    /// Pourquoi il ne peut pas l'être.
    pub refusal: Option<String>,
}

pub(crate) fn restore_dto(f: &RestoreFacts, not_after: i64) -> RestorePointsDto {
    let dto = |s: &ShadowCopy| RestorePointDto { id: s.id.as_str().to_owned(), created: s.created * 1000, description: f.description_of(s.created) };
    let mut points: Vec<RestorePointDto> = f.shadows.iter().map(dto).collect();
    points.sort_by_key(|p| p.created);
    let verdict = oldest_deletable(&f.shadows, not_after);
    RestorePointsDto {
        drive: format!("{}:", f.drive),
        count: f.shadows.len(),
        oldest: f.shadows.iter().min_by_key(|s| s.created).map(dto),
        points,
        deletable: verdict.is_ok(),
        refusal: verdict.err().map(|e| e.to_string()),
    }
}

/// Ce que les actions système doivent connaître du PC.
pub(crate) struct SystemContext<'a> {
    pub drive: char,
    pub hiberfil: Option<u64>,
    /// Points de restauration lus en administrateur (obligatoire pour « supprimer le plus ancien »).
    pub restore: Option<&'a RestoreFacts>,
    /// Date limite du point supprimé (7 jours et début de session).
    pub not_after: i64,
    /// Journal de DISM (`%LOCALAPPDATA%\Lumen\logs\dism-<horodatage>.log`).
    pub dism_log: Option<OutputFile>,
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
    /// Caches de shaders (onglet Jeux) : refusé au moment de l'action si un jeu ou un launcher
    /// tourne.
    Shaders,
    /// Artefacts de projets cochés dans l'onglet Dev (nœuds couverts par les règles d'artefacts).
    Artifacts { ids: Vec<i64> },
    /// `docker image prune` (images sans étiquette listées), si Docker tourne.
    DockerPrune,
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
    /// Protection du système activée sur le lecteur système (`None` : inconnue).
    pub restore_protection: Option<bool>,
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
            restore_protection: f.restore_protection,
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
            restore_protection: None,
            free: None,
        }
    }

    #[cfg(windows)]
    pub fn read_restore_points(app: &AppHandle) -> Result<RestoreFacts, LumenError> {
        let drive_char = sinks().system_drive.chars().next().unwrap_or('C');
        let drive = Drive::parse(&drive_char.to_string()).map_err(|e| LumenError::Plan(e.to_string()))?;
        let dir = lumen_dir(app, "").ok_or_else(|| LumenError::Plan("dossier de Lumen introuvable".into()))?;
        let out = OutputFile::new(&dir, "restore-points.json").map_err(|e| LumenError::Plan(e.to_string()))?;
        let file = out.display_string();
        let run = safety::win::read_restore_points(drive, out, &AtomicBool::new(false));
        if run.status != safety::exec::CommandStatus::Succeeded {
            let why = run.command.exit_meaning(match run.status {
                safety::exec::CommandStatus::Failed(c) => c,
                _ => -1,
            });
            return Err(LumenError::Plan(format!(
                "lecture des points de restauration impossible ({})",
                why.map_or_else(|| crate::journal::status_name(&run.status).to_owned(), str::to_owned)
            )));
        }
        let json = std::fs::read_to_string(&file).map_err(|e| LumenError::Plan(e.to_string()))?;
        RestoreFacts::parse(drive_char, &json).map_err(LumenError::Plan)
    }

    #[cfg(not(windows))]
    pub fn read_restore_points(_: &AppHandle) -> Result<RestoreFacts, LumenError> {
        Err(LumenError::Plan("points de restauration : Windows uniquement".into()))
    }

    #[cfg(windows)]
    pub fn open_system_protection() -> Result<(), String> {
        safety::win::open_system_protection()
    }

    #[cfg(not(windows))]
    pub fn open_system_protection() -> Result<(), String> {
        Err("Windows uniquement".into())
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
    Ok(Selection { targets, allow, commands: Vec::new(), rule_names: names(label.0, label.1), elevated: false, guard: None })
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
    Ok(Selection { targets, allow, commands: Vec::new(), rule_names: names("Doublons", "Duplicates"), elevated: false, guard: None })
}

/// Actions système : commandes officielles élevées, mesurées sur le lecteur système, après un
/// point de restauration obligatoire s'il est demandé.
pub(crate) fn select_system(ctx: &SystemContext<'_>, actions: &[SystemAction], restore_point: bool) -> Result<Selection, LumenError> {
    let drive = ctx.drive;
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
            SystemAction::Components => (OfficialCommand::DismComponentCleanup(ctx.dism_log.clone()), 0),
            SystemAction::HibernateOff => (OfficialCommand::HibernateOff, ctx.hiberfil.unwrap_or(0)),
            SystemAction::HibernateReduced => (OfficialCommand::HibernateReduced, 0),
            SystemAction::OldestRestorePoint => {
                // Jamais le dernier point, jamais un point récent ou créé pendant la session :
                // décidé ici sur la liste lue, et revérifié par le script au moment de l'action.
                let facts = ctx
                    .restore
                    .filter(|f| f.drive.eq_ignore_ascii_case(&drive))
                    .ok_or_else(|| LumenError::Plan("lis d'abord les points de restauration (autorisation administrateur)".into()))?;
                let oldest = oldest_deletable(&facts.shadows, ctx.not_after).map_err(|e| LumenError::Plan(e.to_string()))?;
                (OfficialCommand::ShadowDeleteOldest { drive: d, shadow: oldest.id.clone(), not_after: ctx.not_after }, 0)
            }
            SystemAction::WindowsOld => (OfficialCommand::DiskCleanup(d), 0),
        };
        commands.push(PlannedCommand::action(command, Some(root.clone()), bytes));
    }
    Ok(Selection { targets: Vec::new(), allow: Allowlist::new(), commands, rule_names: HashMap::new(), elevated: true, guard: None })
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

/// `%LOCALAPPDATA%\Lumen\<sub>`, créé s'il manque.
fn lumen_dir(app: &AppHandle, sub: &str) -> Option<WinPath> {
    let dir = app.path().local_data_dir().ok()?.join("Lumen").join(sub);
    std::fs::create_dir_all(&dir).ok()?;
    WinPath::parse(dir.to_str()?).ok()
}

/// Points de restauration du lecteur système, lus en administrateur (une demande UAC, lecture
/// seule) : date du point qui serait supprimé, et s'il peut l'être.
#[tauri::command]
pub async fn restore_points_read(app: AppHandle) -> Result<RestorePointsDto, LumenError> {
    let worker = app.clone();
    let facts = tauri::async_runtime::spawn_blocking(move || platform::read_restore_points(&worker))
        .await
        .map_err(|e| LumenError::Plan(e.to_string()))??;
    let dto = restore_dto(&facts, oldest_not_after(unix_secs(), session_start()));
    app.state::<AdvancedState>().lock()?.restore = Some(facts);
    Ok(dto)
}

/// Ouvre « Protection du système » de Windows, pour activer les points de restauration.
#[tauri::command]
pub fn open_system_protection() -> Result<(), LumenError> {
    platform::open_system_protection().map_err(LumenError::Plan)
}

/// Gouffres système : tailles et réglages lus sans droits d'administrateur.
#[tauri::command]
pub fn system_sinks() -> SinksDto {
    platform::sinks()
}

/// Plan d'une sélection du nettoyage avancé, à confirmer (rien n'est fait ici).
#[tauri::command]
pub fn advanced_plan(
    app: AppHandle,
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
        let restore = state.lock()?.restore.clone();
        let dism_log = lumen_dir(&app, "logs").and_then(|dir| OutputFile::new(&dir, &format!("dism-{}.log", stamp(unix_secs()))).ok());
        let ctx = SystemContext { drive, hiberfil: sinks.hiberfil, restore: restore.as_ref(), not_after: oldest_not_after(unix_secs(), session_start()), dism_log };
        let sel = select_system(&ctx, actions, *restore_point)?;
        let volume_key = crate::timeline::volume_key(&format!("{drive}:"));
        return cleanup::prepare(&clean, &sel, mode, "Gouffres système".into(), ("system", volume_key), &|p| p.to_owned());
    }
    let entry = crate::scanning::entry_of(&scans, scan_id)?;
    if let AdvancedRequest::Shaders = request {
        let applied = entry.applied.as_ref().ok_or_else(|| LumenError::Plan("règles non appliquées".into()))?;
        let mut sel = cleanup::select(applied, &entry.scan, &[crate::games::SHADERS_RULE.to_owned()])?;
        sel.guard = Some(safety::guard::ProcessGuard::games(crate::games::game_dirs(&app.state::<crate::games::GamesState>())));
        let volume_key = crate::timeline::volume_key(&entry.volume);
        return cleanup::prepare(&clean, &sel, mode, "Caches de shaders".into(), ("shaders", volume_key), &|p| applied.display(p));
    }
    if let AdvancedRequest::Artifacts { ids } = &request {
        let applied = entry.applied.as_ref().ok_or_else(|| LumenError::Plan("règles non appliquées".into()))?;
        let sel = crate::dev::select_artifacts(applied, &entry.scan, ids)?;
        let volume_key = crate::timeline::volume_key(&entry.volume);
        return cleanup::prepare(&clean, &sel, mode, "Artefacts de projets".into(), ("artifacts", volume_key), &|p| applied.display(p));
    }
    if let AdvancedRequest::DockerPrune = request {
        let sel = crate::dev::select_docker()?;
        return cleanup::prepare(&clean, &sel, mode, "Images Docker sans étiquette".into(), ("docker", None), &|p| p.to_owned());
    }
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
        AdvancedRequest::System { .. } | AdvancedRequest::Shaders | AdvancedRequest::Artifacts { .. } | AdvancedRequest::DockerPrune => {
            return Err(LumenError::Plan("requête inattendue".into()))
        }
    };
    drop(inner);
    if sel.targets.is_empty() {
        return Err(LumenError::Plan("aucun élément choisi".into()));
    }
    let volume_key = crate::timeline::volume_key(&entry.volume);
    cleanup::prepare(&clean, &sel, mode, title.into(), (kind, volume_key), &display)
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

    const NOW: i64 = 1_800_000_000;
    const DAY: i64 = 86_400;

    fn facts(created: &[i64]) -> RestoreFacts {
        let shadows = created.iter().enumerate().map(|(i, &c)| ShadowCopy { id: ShadowId::parse(&format!("{{0000000{i}-0000-0000-0000-000000000000}}")).unwrap(), created: c }).collect();
        RestoreFacts { drive: 'C', shadows, points: vec![("Lumen : avant nettoyage du système".into(), NOW - 30), ("Windows Update".into(), NOW - 40 * DAY)] }
    }

    fn ctx(restore: Option<&RestoreFacts>) -> SystemContext<'_> {
        let logs = WinPath::parse("C:\\Users\\Jean Dupont\\AppData\\Local\\Lumen\\logs").unwrap();
        SystemContext {
            drive: 'C',
            hiberfil: Some(6 << 30),
            restore,
            not_after: oldest_not_after(NOW, NOW - 3600),
            dism_log: Some(OutputFile::new(&logs, &format!("dism-{}.log", stamp(NOW))).unwrap()),
        }
    }

    #[test]
    fn un_seul_point_juste_cree_jamais_supprime() {
        // Lumen vient de créer le seul point de restauration : « supprimer le plus ancien » est refusé.
        let one = facts(&[NOW - 30]);
        let dto = restore_dto(&one, oldest_not_after(NOW, NOW - 3600));
        assert!(!dto.deletable && dto.count == 1);
        assert_eq!(dto.oldest.as_ref().unwrap().created, (NOW - 30) * 1000);
        assert_eq!(dto.oldest.unwrap().description.as_deref(), Some("Lumen : avant nettoyage du système"));
        let err = select_system(&ctx(Some(&one)), &[SystemAction::OldestRestorePoint], true).err().unwrap();
        assert!(err.to_string().contains("au moins 2"), "{err}");
        // Deux points dont le plus ancien a 3 jours : gardé.
        let recent = facts(&[NOW - 30, NOW - 3 * DAY]);
        assert!(select_system(&ctx(Some(&recent)), &[SystemAction::OldestRestorePoint], true).is_err());
        // Pas lus : rien n'est supposé.
        assert!(select_system(&ctx(None), &[SystemAction::OldestRestorePoint], true).is_err());
        // Le plus ancien a 40 jours, un autre reste : celui-là précisément, date limite transmise.
        let ok = facts(&[NOW - 30, NOW - 40 * DAY]);
        let dto = restore_dto(&ok, oldest_not_after(NOW, NOW - 3600));
        assert!(dto.deletable && dto.refusal.is_none());
        assert_eq!(dto.oldest.as_ref().unwrap().description.as_deref(), Some("Windows Update"));
        let sel = select_system(&ctx(Some(&ok)), &[SystemAction::OldestRestorePoint], true).unwrap();
        match &sel.commands[1].command {
            OfficialCommand::ShadowDeleteOldest { shadow, not_after, .. } => {
                assert_eq!(shadow, &ok.shadows[1].id);
                assert_eq!(*not_after, NOW - 7 * DAY);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn lecture_des_points_de_restauration() {
        let json = "\u{feff}{\"shadows\":[{\"id\":\"{0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9}\",\"created\":1700000000}],\"points\":{\"description\":\"Installé X\",\"created\":1700000100}}";
        let f = RestoreFacts::parse('C', json).unwrap();
        assert_eq!((f.shadows.len(), f.points.len()), (1, 1));
        assert_eq!(f.description_of(1_700_000_000).as_deref(), Some("Installé X"));
        assert!(RestoreFacts::parse('C', "{\"shadows\":[{\"id\":\"'; rm\",\"created\":1}]}").is_err());
        assert_eq!(RestoreFacts::parse('C', "{\"shadows\":[],\"points\":[]}").unwrap().shadows.len(), 0);
        assert_eq!(stamp(1_792_929_605), "20261025-120005");
        assert_eq!(stamp(0), "19700101-000000");
    }

    #[test]
    fn actions_systeme_elevees_apres_le_point_de_restauration() {
        let old = facts(&[NOW - 30, NOW - 40 * DAY]);
        let sel = select_system(&ctx(Some(&old)), &[SystemAction::Components, SystemAction::HibernateOff, SystemAction::OldestRestorePoint, SystemAction::WindowsOld], true).unwrap();
        assert!(sel.elevated && sel.targets.is_empty());
        let first = &sel.commands[0];
        assert_eq!((first.command.clone(), first.role, first.required), (OfficialCommand::RestorePointCreate, CommandRole::Prerequisite, true));
        let lines: Vec<String> = sel.commands[1..].iter().map(|c| c.command.args().join(" ")).collect();
        assert_eq!(
            lines[0],
            format!("/Online /Cleanup-Image /StartComponentCleanup /LogPath:C:\\Users\\Jean Dupont\\AppData\\Local\\Lumen\\logs\\dism-{}.log", stamp(NOW)),
            "jamais /ResetBase, sortie gardée"
        );
        assert!(matches!(sel.commands[3].command, OfficialCommand::ShadowDeleteOldest { .. }), "jamais /all");
        assert!(sel.commands[1..].iter().all(|c| c.role == CommandRole::Action && c.measured.as_ref().map(WinPath::display_string).as_deref() == Some("C:\\")));
        assert_eq!(sel.commands[2].bytes, 6 << 30, "gain estimé de l'hibernation : hiberfil.sys");
        assert!(select_system(&ctx(None), &[], true).is_err());
        assert!(select_system(&ctx(None), &[SystemAction::HibernateOff, SystemAction::HibernateReduced], false).is_err());
        let sans = select_system(&ctx(None), &[SystemAction::Components], false).unwrap();
        assert_eq!(sans.commands.len(), 1);
    }
}
