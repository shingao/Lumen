//! Déplacement intelligent (jalon 9) : IPC autour de `safety::relocate`, journal `moves` et
//! suivi des jonctions créées par Lumen.
//!
//! Le front n'envoie qu'un id d'analyse, un id de nœud et une lettre de disque : le chemin vient
//! de l'analyse, la cible est calculée par `safety` (`<X>:\Lumen\Déplacés\<lettre>\…`). Chaque
//! déplacement est journalisé dès son début ; un déplacement resté « en cours » (Lumen fermé
//! pendant la copie ou avant la confirmation) est remis en état au prochain affichage de la
//! liste, sans jamais supprimer l'original.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use safety::path::WinPath;
use safety::relocate::{Failure, Finished, Health, Prepared, Recovered, Refusal, Staged, Step, Undone};

use crate::cleanup::{unix_ms, with_journal};
use crate::error::LumenError;
use crate::scanning::ScanState;

/// Avancement envoyé au plus toutes les 100 ms.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

#[derive(Default)]
pub struct MoveState(Mutex<Inner>);

#[derive(Default)]
struct Inner {
    next_plan: u32,
    /// Seul le dernier déplacement vérifié peut être lancé.
    plan: Option<(u32, Prepared)>,
    /// Déplacements en attente de la confirmation de suppression de l'original.
    staged: HashMap<i64, Staged>,
    /// Déplacement ou annulation en cours (id du journal).
    running: Option<i64>,
}

fn lock(state: &MoveState) -> Result<std::sync::MutexGuard<'_, Inner>, LumenError> {
    state.0.lock().map_err(|_| LumenError::Move("état indisponible".into()))
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RefusalDto {
    pub code: &'static str,
    pub message: String,
    /// Programmes qui tiennent des fichiers du dossier (« Ferme Discord d'abord »).
    pub apps: Vec<String>,
}

pub(crate) fn refusal_dto(r: &Refusal) -> RefusalDto {
    let code = match r {
        Refusal::NotADirectory => "notADirectory",
        Refusal::Denied(_) => "denied",
        Refusal::TargetDenied(_) => "targetDenied",
        Refusal::AlreadyMoved => "alreadyMoved",
        Refusal::SameVolume => "sameVolume",
        Refusal::TargetUnavailable => "targetUnavailable",
        Refusal::TargetNotFixed => "targetNotFixed",
        Refusal::TargetSubst => "targetSubst",
        Refusal::TargetNotNtfs => "targetNotNtfs",
        Refusal::SourceNotNtfs => "sourceNotNtfs",
        Refusal::GameFolder => "gameFolder",
        Refusal::VirtualDisk => "virtualDisk",
        Refusal::Synced => "synced",
        Refusal::ReparsePoints => "reparsePoints",
        Refusal::InUse(_) => "inUse",
        Refusal::NotEnoughSpace { .. } => "notEnoughSpace",
        Refusal::TargetExists => "targetExists",
        Refusal::StagingExists => "stagingExists",
        Refusal::Empty => "empty",
        Refusal::Unreadable(_) => "unreadable",
    };
    let apps = match r {
        Refusal::InUse(a) => a.clone(),
        _ => Vec::new(),
    };
    RefusalDto { code, message: r.to_string(), apps }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DriveDto {
    pub letter: String,
    pub label: String,
    pub free: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveCheckDto {
    /// Présent seulement si le déplacement est possible.
    pub plan_id: Option<u32>,
    pub fingerprint: Option<String>,
    pub source: String,
    pub target: Option<String>,
    pub drive: Option<String>,
    /// Disques fixes NTFS proposés comme cible.
    pub drives: Vec<DriveDto>,
    pub bytes: u64,
    pub files: u64,
    /// Taille plus 10 %.
    pub needed: u64,
    pub free: u64,
    /// Dossier d'appli dans AppData : « l'appli peut ne plus se mettre à jour correctement ».
    pub review: bool,
    pub refusals: Vec<RefusalDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressDto {
    move_id: i64,
    step: &'static str,
    done: u64,
    total: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveRunDto {
    pub move_id: i64,
    /// `ready` (en attente de la confirmation de suppression), `refused`, `rolledBack`, `failed`.
    pub outcome: &'static str,
    pub step: Option<&'static str>,
    pub message: Option<String>,
    pub refusals: Vec<RefusalDto>,
    pub source: String,
    pub target: String,
    pub bytes: u64,
    pub files: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveRowDto {
    pub id: i64,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub undone_at: Option<i64>,
    pub source: String,
    pub target: String,
    pub bytes: u64,
    pub files: u64,
    pub state: String,
    pub step: Option<String>,
    pub message: Option<String>,
    /// Déplacements faits seulement : `ok`, `targetVolumeAbsent`, `targetMissing`, `junctionMissing`.
    pub health: Option<&'static str>,
}

fn health_name(h: Health) -> &'static str {
    match h {
        Health::Ok => "ok",
        Health::TargetVolumeAbsent => "targetVolumeAbsent",
        Health::TargetMissing => "targetMissing",
        Health::JunctionMissing => "junctionMissing",
    }
}

/// Journal des déplacements (même base que le journal des nettoyages).
pub(crate) mod store {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Row {
        pub id: i64,
        pub started_at: i64,
        pub finished_at: Option<i64>,
        pub undone_at: Option<i64>,
        pub source: String,
        pub target: String,
        pub bytes: u64,
        pub files: u64,
        pub state: String,
        pub step: Option<String>,
        pub message: Option<String>,
    }

    pub struct NewMove<'a> {
        pub source: &'a str,
        pub target: &'a str,
        pub source_volume_key: Option<&'a str>,
        pub target_volume_key: Option<&'a str>,
        pub bytes: u64,
        pub files: u64,
        pub at: i64,
    }

    pub fn insert(c: &Connection, m: &NewMove<'_>) -> rusqlite::Result<i64> {
        c.execute(
            "INSERT INTO moves (started_at, source, target, source_volume_key, target_volume_key, bytes, files, state, step) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'running', 'recheck')",
            params![m.at, m.source, m.target, m.source_volume_key, m.target_volume_key, m.bytes as i64, m.files as i64],
        )?;
        Ok(c.last_insert_rowid())
    }

    /// Nouvel état ; `finished` pour un état terminal.
    pub fn update(c: &Connection, id: i64, state: &str, step: Option<&str>, message: Option<&str>, finished: Option<i64>) -> rusqlite::Result<()> {
        c.execute("UPDATE moves SET state = ?2, step = ?3, message = ?4, finished_at = COALESCE(?5, finished_at) WHERE id = ?1", params![id, state, step, message, finished])?;
        Ok(())
    }

    pub fn undone(c: &Connection, id: i64, at: i64) -> rusqlite::Result<()> {
        c.execute("UPDATE moves SET state = 'undone', undone_at = ?2, message = NULL WHERE id = ?1", params![id, at])?;
        Ok(())
    }

    fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
        Ok(Row {
            id: r.get(0)?,
            started_at: r.get(1)?,
            finished_at: r.get(2)?,
            undone_at: r.get(3)?,
            source: r.get(4)?,
            target: r.get(5)?,
            bytes: r.get::<_, i64>(6)?.max(0) as u64,
            files: r.get::<_, i64>(7)?.max(0) as u64,
            state: r.get(8)?,
            step: r.get(9)?,
            message: r.get(10)?,
        })
    }

    const COLUMNS: &str = "id, started_at, finished_at, undone_at, source, target, bytes, files, state, step, message";

    pub fn list(c: &Connection) -> rusqlite::Result<Vec<Row>> {
        let mut stmt = c.prepare(&format!("SELECT {COLUMNS} FROM moves ORDER BY started_at DESC, id DESC LIMIT 500"))?;
        let rows = stmt.query_map([], row)?.collect();
        rows
    }

    pub fn get(c: &Connection, id: i64) -> rusqlite::Result<Option<Row>> {
        c.query_row(&format!("SELECT {COLUMNS} FROM moves WHERE id = ?1"), params![id], row).optional()
    }
}

/// Avancement d'une étape, limité en fréquence.
fn emitter(app: &AppHandle, move_id: i64) -> impl Fn(Step, u64, u64) + '_ {
    let last = Cell::new(Instant::now() - PROGRESS_EVERY);
    let last_step = Cell::new(None);
    move |step, done, total| {
        let now = Instant::now();
        if last_step.get() != Some(step) || now.duration_since(last.get()) >= PROGRESS_EVERY || done == total {
            last.set(now);
            last_step.set(Some(step));
            let _ = app.emit("move:progress", ProgressDto { move_id, step: step.name(), done, total });
        }
    }
}

fn drive_of(p: &WinPath) -> Option<char> {
    match p.prefix() {
        safety::path::Prefix::Drive(d) => Some(*d),
        safety::path::Prefix::Unc { .. } => None,
    }
}

/// Vérifie le déplacement d'un dossier de l'analyse vers un disque (le plus libre par défaut).
#[tauri::command]
pub fn move_check(
    app: AppHandle,
    scans: State<'_, ScanState>,
    state: State<'_, MoveState>,
    scan_id: u32,
    node_id: u32,
    drive: Option<String>,
) -> Result<MoveCheckDto, LumenError> {
    let entry = crate::scanning::entry_of(&scans, scan_id)?;
    entry.scan.node(node_id).ok_or(LumenError::UnknownNode)?;
    let raw = entry.scan.path(node_id);
    let source = WinPath::parse(&raw).map_err(|e| LumenError::Move(e.to_string()))?;
    let display = |p: &str| entry.applied.as_ref().map_or_else(|| p.to_owned(), |a| a.display(p));
    let source_drive = drive_of(&source);
    let drives: Vec<DriveDto> = platform::drives().into_iter().filter(|d| d.letter.chars().next() != source_drive).collect();
    let chosen = drive
        .and_then(|d| d.chars().next())
        .map(|c| c.to_ascii_uppercase())
        .or_else(|| drives.iter().max_by_key(|d| d.free).and_then(|d| d.letter.chars().next()));
    let base = MoveCheckDto {
        plan_id: None,
        fingerprint: None,
        source: display(&raw),
        target: None,
        drive: chosen.map(|c| format!("{c}:")),
        drives,
        bytes: 0,
        files: 0,
        needed: 0,
        free: 0,
        review: false,
        refusals: Vec::new(),
    };
    let Some(letter) = chosen else {
        return Ok(MoveCheckDto { refusals: vec![refusal_dto(&Refusal::TargetUnavailable)], ..base });
    };
    let games = crate::games::game_dirs_now(&app.state::<crate::games::GamesState>());
    match platform::prepare(&source, letter, &games)? {
        Ok(plan) => {
            let dto = MoveCheckDto {
                fingerprint: Some(plan.fingerprint()),
                target: Some(plan.target.display_string()),
                bytes: plan.bytes,
                files: plan.files,
                needed: plan.needed,
                free: plan.target_free,
                review: plan.review,
                ..base
            };
            let mut inner = lock(&state)?;
            inner.next_plan += 1;
            let id = inner.next_plan;
            inner.plan = Some((id, plan));
            Ok(MoveCheckDto { plan_id: Some(id), ..dto })
        }
        Err(refusals) => Ok(MoveCheckDto {
            target: safety::relocate::target_for(&source, letter).map(|t| t.display_string()),
            refusals: refusals.iter().map(refusal_dto).collect(),
            ..base
        }),
    }
}

fn run_dto(move_id: i64, plan: &Prepared, outcome: &'static str) -> MoveRunDto {
    MoveRunDto {
        move_id,
        outcome,
        step: None,
        message: None,
        refusals: Vec::new(),
        source: plan.source.display_string(),
        target: plan.target.display_string(),
        bytes: plan.bytes,
        files: plan.files,
    }
}

/// Lance le déplacement vérifié (empreinte du plan affiché) : recontrôle, copie, vérification,
/// jonction et test. S'arrête avant toute suppression : `move_confirm` décide.
#[tauri::command]
pub async fn move_start(app: AppHandle, plan_id: u32, fingerprint: String) -> Result<MoveRunDto, LumenError> {
    let worker = app.clone();
    tauri::async_runtime::spawn_blocking(move || start(&worker, plan_id, &fingerprint)).await.map_err(|e| LumenError::Move(e.to_string()))?
}

fn start(app: &AppHandle, plan_id: u32, fingerprint: &str) -> Result<MoveRunDto, LumenError> {
    let state = app.state::<MoveState>();
    let plan = {
        let mut inner = lock(&state)?;
        if inner.running.is_some() {
            return Err(LumenError::Move("un déplacement est déjà en cours".into()));
        }
        match inner.plan.take() {
            Some((id, p)) if id == plan_id => p,
            _ => return Err(LumenError::UnknownPlan),
        }
    };
    let source_key = drive_of(&plan.source).and_then(|d| crate::timeline::volume_key(&format!("{d}:")));
    let target_key = drive_of(&plan.target).and_then(|d| crate::timeline::volume_key(&format!("{d}:")));
    let (src, dst) = (plan.source.display_string(), plan.target.display_string());
    let new = store::NewMove { source: &src, target: &dst, source_volume_key: source_key.as_deref(), target_volume_key: target_key.as_deref(), bytes: plan.bytes, files: plan.files, at: unix_ms() };
    let id = with_journal(app, |j| store::insert(j.conn(), &new).map_err(Into::into))?;
    lock(&state)?.running = Some(id);
    let games = crate::games::game_dirs_now(&app.state::<crate::games::GamesState>());
    let result = platform::run(&plan, fingerprint, &games, &emitter(app, id));
    let (dto, staged) = match result {
        Ok(staged) => {
            let _ = with_journal(app, |j| store::update(j.conn(), id, "running", Some("ready"), None, None).map_err(Into::into));
            (run_dto(id, &plan, "ready"), Some(staged))
        }
        Err(f) => {
            let (state_name, outcome, step, message, refusals) = match &f {
                Failure::FingerprintMismatch => ("failed", "refused", None, Some("le déplacement confirmé n'est pas celui affiché".to_owned()), Vec::new()),
                Failure::Refused(r) => ("rolledBack", "refused", Some(Step::Recheck), Some(r.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ; ")), r.iter().map(refusal_dto).collect()),
                Failure::Step { step, error, rolled_back: true, .. } => ("rolledBack", "rolledBack", Some(*step), Some(error.clone()), Vec::new()),
                Failure::Step { step, error, rollback_error, .. } => (
                    "failed",
                    "failed",
                    Some(*step),
                    Some(format!("{error} ; retour en arrière incomplet : {}", rollback_error.clone().unwrap_or_default())),
                    Vec::new(),
                ),
            };
            let step_name = step.map(Step::name);
            let _ = with_journal(app, |j| store::update(j.conn(), id, state_name, step_name, message.as_deref(), Some(unix_ms())).map_err(Into::into));
            (MoveRunDto { step: step_name, message, refusals, ..run_dto(id, &plan, outcome) }, None)
        }
    };
    {
        let mut inner = lock(&state)?;
        inner.running = None;
        if let Some(s) = staged {
            inner.staged.insert(id, s);
        }
    }
    write_junction_list(app);
    Ok(dto)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveFinishDto {
    /// `done`, `rolledBack`, `deleteFailed`, `rollbackFailed`.
    pub outcome: &'static str,
    pub message: Option<String>,
    /// Espace libéré sur le disque d'origine.
    pub freed: u64,
}

/// Après la jonction testée : `delete` = suppression définitive de l'original, confirmée
/// explicitement ; sinon retour en arrière.
#[tauri::command]
pub async fn move_confirm(app: AppHandle, move_id: i64, delete: bool) -> Result<MoveFinishDto, LumenError> {
    let worker = app.clone();
    tauri::async_runtime::spawn_blocking(move || confirm(&worker, move_id, delete)).await.map_err(|e| LumenError::Move(e.to_string()))?
}

fn confirm(app: &AppHandle, move_id: i64, delete: bool) -> Result<MoveFinishDto, LumenError> {
    let state = app.state::<MoveState>();
    let staged = lock(&state)?.staged.remove(&move_id).ok_or_else(|| LumenError::Move("déplacement inconnu ou déjà terminé".into()))?;
    let finished = platform::finish(&staged, delete);
    let (state_name, step, outcome, message, freed) = match &finished {
        Finished::Done => ("done", None, "done", None, staged.bytes),
        Finished::RolledBack => ("rolledBack", Some("confirm"), "rolledBack", None, 0),
        Finished::DeleteFailed(m) => ("failed", Some("delete"), "deleteFailed", Some(m.clone()), 0),
        Finished::RollbackFailed(m) => ("failed", Some("confirm"), "rollbackFailed", Some(m.clone()), 0),
    };
    with_journal(app, |j| store::update(j.conn(), move_id, state_name, step, message.as_deref(), Some(unix_ms())).map_err(Into::into))?;
    write_junction_list(app);
    Ok(MoveFinishDto { outcome, message, freed })
}

/// Déplacements du journal, avec l'état de chaque jonction ; les déplacements interrompus d'une
/// session précédente sont d'abord remis en état.
#[tauri::command]
pub async fn moves_list(app: AppHandle) -> Result<Vec<MoveRowDto>, LumenError> {
    let worker = app.clone();
    tauri::async_runtime::spawn_blocking(move || list(&worker)).await.map_err(|e| LumenError::Move(e.to_string()))?
}

fn list(app: &AppHandle) -> Result<Vec<MoveRowDto>, LumenError> {
    let state = app.state::<MoveState>();
    let rows = with_journal(app, |j| store::list(j.conn()).map_err(Into::into))?;
    let (busy, staged): (Option<i64>, Vec<i64>) = {
        let inner = lock(&state)?;
        (inner.running, inner.staged.keys().copied().collect())
    };
    let mut out = Vec::new();
    for mut r in rows {
        if r.state == "running" && busy != Some(r.id) && !staged.contains(&r.id) {
            let (state_name, message) = match platform::recover(&r.source, &r.target) {
                Recovered::RolledBack => ("rolledBack", "interrompu : remis comme avant".to_owned()),
                Recovered::Done => ("done", "interrompu après la suppression de l'original : terminé".to_owned()),
                Recovered::NeedsAttention(m) => ("failed", format!("interrompu : {m}")),
            };
            let _ = with_journal(app, |j| store::update(j.conn(), r.id, state_name, Some("recover"), Some(&message), Some(unix_ms())).map_err(Into::into));
            r.state = state_name.into();
            r.message = Some(message);
        }
        let health = (r.state == "done").then(|| health_name(platform::health(&r.source, &r.target)));
        out.push(MoveRowDto {
            id: r.id,
            started_at: r.started_at,
            finished_at: r.finished_at,
            undone_at: r.undone_at,
            source: r.source,
            target: r.target,
            bytes: r.bytes,
            files: r.files,
            state: r.state,
            step: r.step,
            message: r.message,
            health,
        });
    }
    write_junction_list(app);
    Ok(out)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveUndoDto {
    /// `done`, `refused`, `failed`.
    pub outcome: &'static str,
    pub message: Option<String>,
}

/// « Annuler le déplacement » : recopie depuis la cible, vérifie, retire la jonction, remet le
/// dossier ; la copie de la cible part à la Corbeille.
#[tauri::command]
pub async fn move_undo(app: AppHandle, move_id: i64) -> Result<MoveUndoDto, LumenError> {
    let worker = app.clone();
    tauri::async_runtime::spawn_blocking(move || undo(&worker, move_id)).await.map_err(|e| LumenError::Move(e.to_string()))?
}

fn undo(app: &AppHandle, move_id: i64) -> Result<MoveUndoDto, LumenError> {
    let state = app.state::<MoveState>();
    let row = with_journal(app, |j| store::get(j.conn(), move_id).map_err(Into::into))?.ok_or(LumenError::UnknownAction)?;
    if row.state != "done" {
        return Ok(MoveUndoDto { outcome: "refused", message: Some("seul un déplacement terminé s'annule".into()) });
    }
    {
        let mut inner = lock(&state)?;
        if inner.running.is_some() {
            return Err(LumenError::Move("un déplacement est déjà en cours".into()));
        }
        inner.running = Some(move_id);
    }
    let result = platform::undo(&row.source, &row.target, &emitter(app, move_id));
    lock(&state)?.running = None;
    let dto = match result {
        Undone::Done { recycle_error } => {
            with_journal(app, |j| store::undone(j.conn(), move_id, unix_ms()).map_err(Into::into))?;
            MoveUndoDto { outcome: "done", message: recycle_error.map(|e| format!("la copie reste sur le disque cible (Corbeille : {e})")) }
        }
        Undone::Refused(m) => MoveUndoDto { outcome: "refused", message: Some(m) },
        Undone::Failed { error, .. } => MoveUndoDto { outcome: "failed", message: Some(error) },
    };
    write_junction_list(app);
    Ok(dto)
}

/// Liste lue par la désinstallation (`installer/hooks.nsh`), qui prévient sans jamais y toucher.
pub const JUNCTIONS_FILE: &str = "jonctions.txt";

/// Jonctions que Lumen a pu laisser : déplacement terminé et non annulé, en attente de
/// confirmation ou en échec. Une ligne par jonction : `dossier → cible`.
fn junction_lines(rows: &[store::Row]) -> String {
    rows.iter()
        .filter(|r| r.undone_at.is_none() && matches!(r.state.as_str(), "running" | "done" | "failed"))
        .map(|r| format!("{}  →  {}\r\n", r.source, r.target))
        .collect()
}

/// Réécrit `%LOCALAPPDATA%\Lumen\jonctions.txt` (UTF-16 LE sans BOM, lu par NSIS) après chaque
/// changement et au démarrage.
pub(crate) fn write_junction_list(app: &AppHandle) {
    let Ok(rows) = with_journal(app, |j| store::list(j.conn()).map_err(Into::into)) else {
        return;
    };
    let Ok(dir) = app.path().local_data_dir() else {
        return;
    };
    let bytes: Vec<u8> = junction_lines(&rows).encode_utf16().flat_map(u16::to_le_bytes).collect();
    let _ = std::fs::write(dir.join("Lumen").join(JUNCTIONS_FILE), bytes);
}

mod platform {
    use super::*;

    #[cfg(windows)]
    fn context<R>(games: &[WinPath], f: impl FnOnce(&safety::relocate::MoveContext<'_>) -> R) -> Result<R, LumenError> {
        let layout = safety::win::system_layout().map_err(|e| LumenError::Move(e.to_string()))?;
        let deny = safety::denylist::DenyList::new(&layout);
        let synced = safety::win::relocate::synced_roots();
        let app_data = safety::win::relocate::app_data_roots();
        Ok(f(&safety::relocate::MoveContext { deny: &deny, game_dirs: games, synced_roots: &synced, app_data: &app_data }))
    }

    #[cfg(windows)]
    pub fn drives() -> Vec<DriveDto> {
        use safety::relocate::MoveProbe;
        crate::scanning::platform::volumes()
            .into_iter()
            .filter(|v| v.kind == "fixed" && v.file_system.eq_ignore_ascii_case("NTFS"))
            .filter(|v| v.letter.chars().next().and_then(|c| safety::win::relocate::WinMover.volume(c)).is_some_and(|i| !i.subst))
            .map(|v| DriveDto { letter: v.letter, label: v.label, free: v.free })
            .collect()
    }

    #[cfg(windows)]
    pub fn prepare(source: &WinPath, drive: char, games: &[WinPath]) -> Result<Result<Prepared, Vec<Refusal>>, LumenError> {
        context(games, |ctx| safety::relocate::prepare(source, drive, ctx, &safety::win::relocate::WinMover))
    }

    #[cfg(windows)]
    pub fn run(plan: &Prepared, fingerprint: &str, games: &[WinPath], progress: &dyn Fn(Step, u64, u64)) -> Result<Staged, Failure> {
        let mover = safety::win::relocate::WinMover;
        context(games, |ctx| safety::relocate::run(plan, fingerprint, ctx, &mover, &mover, progress))
            .unwrap_or_else(|e| Err(Failure::Refused(vec![Refusal::Unreadable(e.to_string())])))
    }

    #[cfg(windows)]
    pub fn finish(staged: &Staged, delete: bool) -> Finished {
        let mover = safety::win::relocate::WinMover;
        safety::relocate::finish(staged, delete, &mover, &mover)
    }

    #[cfg(windows)]
    fn paths(source: &str, target: &str) -> Option<(WinPath, WinPath)> {
        Some((WinPath::parse(source).ok()?, WinPath::parse(target).ok()?))
    }

    #[cfg(windows)]
    pub fn recover(source: &str, target: &str) -> Recovered {
        let mover = safety::win::relocate::WinMover;
        match paths(source, target) {
            Some((s, t)) => safety::relocate::recover(&s, &t, &mover, &mover),
            None => Recovered::NeedsAttention("chemin illisible".into()),
        }
    }

    #[cfg(windows)]
    pub fn health(source: &str, target: &str) -> Health {
        match paths(source, target) {
            Some((s, t)) => safety::relocate::health(&s, &t, &safety::win::relocate::WinMover),
            None => Health::TargetMissing,
        }
    }

    #[cfg(windows)]
    pub fn undo(source: &str, target: &str, progress: &dyn Fn(Step, u64, u64)) -> Undone {
        let mover = safety::win::relocate::WinMover;
        match paths(source, target) {
            Some((s, t)) => safety::relocate::undo(&s, &t, &mover, &mover, progress),
            None => Undone::Refused("chemin illisible".into()),
        }
    }

    // Hors Windows (développement) : rien n'est déplacé.
    #[cfg(not(windows))]
    pub fn drives() -> Vec<DriveDto> {
        Vec::new()
    }

    #[cfg(not(windows))]
    pub fn prepare(_: &WinPath, _: char, _: &[WinPath]) -> Result<Result<Prepared, Vec<Refusal>>, LumenError> {
        Err(LumenError::Move("disponible sous Windows uniquement".into()))
    }

    #[cfg(not(windows))]
    pub fn run(_: &Prepared, _: &str, _: &[WinPath], _: &dyn Fn(Step, u64, u64)) -> Result<Staged, Failure> {
        Err(Failure::Refused(vec![Refusal::TargetUnavailable]))
    }

    #[cfg(not(windows))]
    pub fn finish(_: &Staged, _: bool) -> Finished {
        Finished::RollbackFailed("disponible sous Windows uniquement".into())
    }

    #[cfg(not(windows))]
    pub fn recover(_: &str, _: &str) -> Recovered {
        Recovered::NeedsAttention("disponible sous Windows uniquement".into())
    }

    #[cfg(not(windows))]
    pub fn health(_: &str, _: &str) -> Health {
        Health::TargetVolumeAbsent
    }

    #[cfg(not(windows))]
    pub fn undo(_: &str, _: &str, _: &dyn Fn(Step, u64, u64)) -> Undone {
        Undone::Refused("disponible sous Windows uniquement".into())
    }
}

#[cfg(test)]
mod tests {
    use super::store::{get, insert, undone, update, NewMove};
    use super::*;
    use crate::journal::Journal;

    #[test]
    fn journal_des_deplacements() {
        let j = Journal::in_memory().unwrap();
        let c = j.conn();
        let m = NewMove { source: "C:\\Users\\Arthur\\Vidéos", target: "D:\\Lumen\\Déplacés\\C\\Users\\Arthur\\Vidéos", source_volume_key: Some("guid:C"), target_volume_key: None, bytes: 42, files: 3, at: 1000 };
        let id = insert(c, &m).unwrap();
        let r = get(c, id).unwrap().unwrap();
        assert_eq!((r.state.as_str(), r.step.as_deref(), r.bytes, r.files), ("running", Some("recheck"), 42, 3));
        update(c, id, "running", Some("ready"), None, None).unwrap();
        update(c, id, "done", None, None, Some(2000)).unwrap();
        let r = get(c, id).unwrap().unwrap();
        assert_eq!((r.state.as_str(), r.finished_at), ("done", Some(2000)));
        undone(c, id, 3000).unwrap();
        let id2 = insert(c, &NewMove { at: 4000, ..m }).unwrap();
        update(c, id2, "rolledBack", Some("verify"), Some("contenu différent"), Some(4100)).unwrap();
        let all = store::list(c).unwrap();
        assert_eq!(all.iter().map(|r| (r.id, r.state.as_str())).collect::<Vec<_>>(), [(id2, "rolledBack"), (id, "undone")]);
        assert_eq!(all[1].undone_at, Some(3000));
        // Un état hors de la liste est refusé par la base.
        assert!(update(c, id, "inconnu", None, None, None).is_err());
    }

    #[test]
    fn jonctions_listees_pour_la_desinstallation() {
        let j = Journal::in_memory().unwrap();
        let c = j.conn();
        let m = NewMove { source: "C:\\Users\\Arthur\\Vidéos", target: "D:\\Lumen\\Déplacés\\C\\Users\\Arthur\\Vidéos", source_volume_key: None, target_volume_key: None, bytes: 1, files: 1, at: 1000 };
        let done = insert(c, &m).unwrap();
        update(c, done, "done", None, None, Some(1100)).unwrap();
        let back = insert(c, &NewMove { source: "C:\\Jeux", at: 2000, ..m }).unwrap();
        update(c, back, "rolledBack", None, None, Some(2100)).unwrap();
        let cancelled = insert(c, &NewMove { source: "C:\\Musique", at: 3000, ..m }).unwrap();
        update(c, cancelled, "done", None, None, Some(3100)).unwrap();
        undone(c, cancelled, 3200).unwrap();
        // En attente de confirmation : la jonction existe déjà.
        insert(c, &NewMove { source: "C:\\Photos", at: 4000, ..m }).unwrap();
        let lines = junction_lines(&store::list(c).unwrap());
        assert_eq!(lines, "C:\\Photos  →  D:\\Lumen\\Déplacés\\C\\Users\\Arthur\\Vidéos\r\nC:\\Users\\Arthur\\Vidéos  →  D:\\Lumen\\Déplacés\\C\\Users\\Arthur\\Vidéos\r\n");
    }

    #[test]
    fn refus_codes_et_programmes() {
        let r = refusal_dto(&Refusal::InUse(vec!["Discord".into()]));
        assert_eq!((r.code, r.apps.as_slice(), r.message.as_str()), ("inUse", ["Discord".to_owned()].as_slice(), "ouvert par Discord"));
        assert_eq!(refusal_dto(&Refusal::NotEnoughSpace { needed: 1100, free: 10 }).code, "notEnoughSpace");
    }
}
