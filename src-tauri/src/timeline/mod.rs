//! Timeline : un snapshot agrégé à chaque analyse et un par jour tant que l'appli tourne (aucun
//! service ni tâche planifiée), courbe de l'espace utilisé par série, repères des nettoyages de
//! Lumen, projection et comparaison de deux dates.

pub mod diff;
pub mod projection;
pub mod retention;
pub mod snapshot;
pub mod store;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::cleanup::{unix_ms, with_journal};
use crate::error::LumenError;
use crate::journal::JournalError;
use crate::scanning::{ScanState, ScanTarget};
use projection::Projection;
use scan::Scan;
use store::{NewSnapshot, Series, VolumeIdentity};

/// Snapshot quotidien : une série est due si son dernier snapshot a plus de 20 h…
const DAILY_AFTER_MS: i64 = 20 * 3_600_000;
/// … et si l'utilisateur l'a analysée lui-même dans les 30 derniers jours.
const DAILY_ACTIVE_MS: i64 = 30 * retention::DAY_MS;
/// Lignes par liste de la comparaison.
const COMPARE_LIMIT: usize = 15;

/// Volume monté, avec son identité et son espace.
pub(crate) struct Present {
    identity: VolumeIdentity,
    total: u64,
    free: u64,
}

mod platform {
    use super::*;

    #[cfg(windows)]
    pub fn present() -> Vec<Present> {
        scan::win::volumes()
            .into_iter()
            .map(|v| Present {
                identity: VolumeIdentity::new(v.guid.as_deref(), v.serial, &v.file_system, &v.label, &format!("{}:", v.letter)),
                total: v.total,
                free: v.free,
            })
            .collect()
    }

    /// Hors de Windows (développement) : un seul volume, sans lettre.
    #[cfg(not(windows))]
    pub fn present() -> Vec<Present> {
        vec![Present { identity: VolumeIdentity::new(None, 0, "local", "local", ""), total: 0, free: 0 }]
    }
}

fn present_at<'a>(list: &'a [Present], letter: &str) -> Option<&'a Present> {
    list.iter().find(|p| p.identity.letter.eq_ignore_ascii_case(letter))
}

fn present_key<'a>(list: &'a [Present], key: &str) -> Option<&'a Present> {
    list.iter().find(|p| p.identity.key == key)
}

/// Clé stable du volume monté sous `letter` (« C: »), pour retenir le volume d'un nettoyage.
pub(crate) fn volume_key(letter: &str) -> Option<String> {
    present_at(&platform::present(), letter).map(|p| p.identity.key.clone())
}

fn method_name(m: scan::Method) -> &'static str {
    match m {
        scan::Method::Walk => "walk",
        scan::Method::Mft => "mft",
    }
}

fn sql(e: rusqlite::Error) -> JournalError {
    JournalError::from(e)
}

/// Snapshot d'une analyse terminée (`source` : `scan` ou `daily`).
pub(crate) fn record_scan(app: &AppHandle, scan: &Scan, source: &'static str) -> Result<i64, LumenError> {
    let (letter, root) = store::split_root(&scan.root_path);
    let present = platform::present();
    let v = present_at(&present, &letter).ok_or_else(|| LumenError::Timeline(format!("volume {letter} introuvable")))?;
    let taken = snapshot::from_scan(scan);
    let s = NewSnapshot {
        volume: &v.identity,
        scan_serial: scan.volume_serial,
        root: &root,
        method: method_name(scan.method),
        source,
        taken_at: unix_ms(),
        volume_total: Some(v.total),
        volume_free: Some(v.free),
        taken: &taken,
    };
    let id = with_journal(app, |j| store::insert(j.conn_mut(), &s).map_err(sql))?;
    let _ = app.emit("timeline:changed", ());
    Ok(id)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TargetDto {
    UserProfile,
    Volume { letter: String },
}

/// Cible d'analyse qui reproduit la racine d'une série, si son volume est monté.
fn target_of(root: &str, letter: Option<&str>, profile: Option<&(String, String)>) -> Option<TargetDto> {
    let letter = letter?;
    if root == "\\" {
        return Some(TargetDto::Volume { letter: letter.to_owned() });
    }
    profile.filter(|(pl, pr)| pl.eq_ignore_ascii_case(letter) && pr.eq_ignore_ascii_case(root)).map(|_| TargetDto::UserProfile)
}

/// Chemin complet actuel de la racine d'une série (lettre d'aujourd'hui).
fn base_path(letter: &str, root: &str) -> String {
    if root == "\\" {
        format!("{letter}\\")
    } else {
        format!("{letter}{root}")
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesDto {
    pub volume_id: i64,
    pub root: String,
    pub method: String,
    pub label: String,
    pub file_system: String,
    /// Lettre actuelle ; absent si le volume n'est pas monté.
    pub letter: Option<String>,
    /// Lettre au dernier snapshot.
    pub last_letter: String,
    pub profile: bool,
    pub target: Option<TargetDto>,
    pub count: u64,
    pub first: i64,
    pub last: i64,
}

fn profile_root() -> Option<(String, String)> {
    crate::scanning::platform::user_profile().map(|p| store::split_root(&p))
}

fn series_dto(s: &Series, present: &[Present], profile: Option<&(String, String)>) -> SeriesDto {
    let letter = present_key(present, &s.key).map(|p| p.identity.letter.clone());
    let target = target_of(&s.root, letter.as_deref(), profile);
    SeriesDto {
        volume_id: s.volume_id,
        root: s.root.clone(),
        method: s.method.clone(),
        label: s.label.clone(),
        file_system: s.fs.clone(),
        profile: matches!(target, Some(TargetDto::UserProfile)),
        letter,
        last_letter: s.letter.clone(),
        target,
        count: s.count,
        first: s.first,
        last: s.last,
    }
}

#[tauri::command]
pub fn timeline_series(app: AppHandle) -> Result<Vec<SeriesDto>, LumenError> {
    let all = with_journal(&app, |j| store::series(j.conn()).map_err(sql))?;
    let present = platform::present();
    let profile = profile_root();
    Ok(all.iter().map(|s| series_dto(s, &present, profile.as_ref())).collect())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesKey {
    pub volume_id: i64,
    pub root: String,
    pub method: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointDto {
    pub id: i64,
    pub at: i64,
    pub used: u64,
    pub source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkerDto {
    pub action_id: i64,
    pub at: i64,
    pub bytes: u64,
    pub title: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionDto {
    /// `tooFew` (moins de 5 points sur 30 jours), `flat` (pente nulle ou négative), `growing`.
    pub status: &'static str,
    pub points: usize,
    pub bytes_per_day: Option<f64>,
    pub days_to_full: Option<f64>,
}

impl From<Projection> for ProjectionDto {
    fn from(p: Projection) -> ProjectionDto {
        match p {
            Projection::TooFew { points } => ProjectionDto { status: "tooFew", points, bytes_per_day: None, days_to_full: None },
            Projection::Flat { bytes_per_day } => ProjectionDto { status: "flat", points: 0, bytes_per_day: Some(bytes_per_day), days_to_full: None },
            Projection::Growing { bytes_per_day, days_to_full, points } => ProjectionDto { status: "growing", points, bytes_per_day: Some(bytes_per_day), days_to_full },
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineDto {
    pub points: Vec<PointDto>,
    pub markers: Vec<MarkerDto>,
    pub projection: ProjectionDto,
    pub free: Option<u64>,
    pub total: Option<u64>,
    pub trend7d: Option<i64>,
    /// Chemin complet actuel de la racine (Explorer), si le volume est monté.
    pub base: Option<String>,
    pub target: Option<TargetDto>,
}

/// Données d'une série : points, repères des nettoyages, projection, espace libre.
pub(crate) fn timeline_of(conn: &rusqlite::Connection, key: &SeriesKey, present: &[Present], profile: Option<&(String, String)>, now: i64) -> rusqlite::Result<TimelineDto> {
    let pts = store::points(conn, key.volume_id, &key.root, &key.method)?;
    let first = pts.first().map_or(now, |p| p.taken_at);
    let markers: Vec<store::Marker> = store::markers(conn, key.volume_id, &key.root)?.into_iter().filter(|m| m.at >= first && m.at <= now).collect();
    let vkey: String = conn.query_row("SELECT key FROM volumes WHERE id = ?1", rusqlite::params![key.volume_id], |r| r.get(0))?;
    let mounted = present_key(present, &vkey);
    let last = pts.last();
    let free = mounted.map(|p| p.free).or_else(|| last.and_then(|p| p.volume_free));
    let total = mounted.map(|p| p.total).or_else(|| last.and_then(|p| p.volume_total));
    let cleanups: Vec<projection::Cleanup> = markers.iter().map(|m| projection::Cleanup { at: m.at, bytes: m.bytes }).collect();
    let points: Vec<projection::Point> = pts.iter().map(|p| projection::Point { at: p.taken_at, used: p.used }).collect();
    let letter = mounted.map(|p| p.identity.letter.clone());
    Ok(TimelineDto {
        projection: projection::project(&points, &cleanups, now, last.and_then(|p| p.volume_free)).into(),
        trend7d: store::trend_7d(&pts, now),
        points: pts.iter().map(|p| PointDto { id: p.id, at: p.taken_at, used: p.used, source: p.source.clone() }).collect(),
        markers: markers.into_iter().map(|m| MarkerDto { action_id: m.action_id, at: m.at, bytes: m.bytes, title: m.title, kind: m.kind }).collect(),
        free,
        total,
        base: letter.as_deref().map(|l| base_path(l, &key.root)),
        target: target_of(&key.root, letter.as_deref(), profile),
    })
}

#[tauri::command]
pub fn timeline_data(app: AppHandle, series: SeriesKey) -> Result<TimelineDto, LumenError> {
    let present = platform::present();
    let profile = profile_root();
    with_journal(&app, |j| timeline_of(j.conn(), &series, &present, profile.as_ref(), unix_ms()).map_err(sql))
}

/// Variation sur 7 jours de la cible affichée (KPI « +18 Go / 7 j »), série la plus récente.
#[tauri::command]
pub fn timeline_trend(app: AppHandle, target: ScanTarget) -> Result<Option<i64>, LumenError> {
    let (letter, root) = match &target {
        ScanTarget::UserProfile => profile_root().ok_or(LumenError::NoProfile)?,
        ScanTarget::Volume { letter } => (letter.to_ascii_uppercase(), "\\".to_owned()),
    };
    let Some(key) = volume_key(&letter) else { return Ok(None) };
    with_journal(&app, |j| {
        let all = store::series(j.conn()).map_err(sql)?;
        let Some(s) = all.iter().filter(|s| s.key == key && s.root.eq_ignore_ascii_case(&root)).max_by_key(|s| s.last) else { return Ok(None) };
        let pts = store::points(j.conn(), s.volume_id, &s.root, &s.method).map_err(sql)?;
        Ok(store::trend_7d(&pts, unix_ms()))
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapDto {
    pub id: i64,
    pub at: i64,
    pub used: u64,
    pub root: String,
    pub method: String,
    pub letter: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareDto {
    pub comparable: bool,
    /// Pourquoi deux snapshots ne se comparent pas : `volume`, `root` ou `method`.
    pub reason: Option<&'static str>,
    pub from: SnapDto,
    pub to: SnapDto,
    pub diff: Option<diff::Diff>,
    /// Seuil des dossiers gardés (le plus grand des deux snapshots).
    pub min_bytes: u64,
    pub base: Option<String>,
    pub target: Option<TargetDto>,
}

/// Pourquoi deux snapshots ne sont pas comparables (`None` : même série).
pub fn incomparable(a: &store::SnapshotInfo, b: &store::SnapshotInfo) -> Option<&'static str> {
    if a.volume_id != b.volume_id {
        Some("volume")
    } else if !a.root.eq_ignore_ascii_case(&b.root) {
        Some("root")
    } else if a.method != b.method {
        Some("method")
    } else {
        None
    }
}

pub(crate) fn compare_in(conn: &rusqlite::Connection, a: i64, b: i64, present: &[Present], profile: Option<&(String, String)>) -> Result<CompareDto, LumenError> {
    let get = |id| store::snapshot(conn, id).map_err(|e| LumenError::Timeline(e.to_string()))?.ok_or_else(|| LumenError::Timeline("snapshot introuvable".into()));
    let (mut x, mut y) = (get(a)?, get(b)?);
    if x.taken_at > y.taken_at {
        std::mem::swap(&mut x, &mut y);
    }
    let snap = |s: &store::SnapshotInfo| SnapDto { id: s.id, at: s.taken_at, used: s.used, root: s.root.clone(), method: s.method.clone(), letter: s.letter.clone() };
    let reason = incomparable(&x, &y);
    let rows = |id| store::rows(conn, id).map_err(|e| LumenError::Timeline(e.to_string()));
    let d = match reason {
        Some(_) => None,
        None => Some(diff::diff(&rows(x.id)?, &rows(y.id)?, x.used, y.used, COMPARE_LIMIT)),
    };
    let vkey: Option<String> = conn.query_row("SELECT key FROM volumes WHERE id = ?1", rusqlite::params![y.volume_id], |r| r.get(0)).ok();
    let letter = vkey.and_then(|k| present_key(present, &k).map(|p| p.identity.letter.clone()));
    Ok(CompareDto {
        comparable: reason.is_none(),
        reason,
        min_bytes: x.min_bytes.max(y.min_bytes),
        base: letter.as_deref().filter(|_| reason.is_none()).map(|l| base_path(l, &y.root)),
        target: target_of(&y.root, letter.as_deref(), profile).filter(|_| reason.is_none()),
        from: snap(&x),
        to: snap(&y),
        diff: d,
    })
}

#[tauri::command]
pub fn timeline_compare(app: AppHandle, from: i64, to: i64) -> Result<CompareDto, LumenError> {
    let present = platform::present();
    let profile = profile_root();
    with_journal(&app, |j| Ok(compare_in(j.conn(), from, to, &present, profile.as_ref())))?
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseDto {
    pub bytes: u64,
    pub snapshots: u64,
    pub series: u64,
}

impl From<store::DbStats> for DatabaseDto {
    fn from(s: store::DbStats) -> DatabaseDto {
        DatabaseDto { bytes: s.bytes, snapshots: s.snapshots, series: s.series }
    }
}

#[tauri::command]
pub fn timeline_database(app: AppHandle) -> Result<DatabaseDto, LumenError> {
    with_journal(&app, |j| store::stats(j.conn()).map(DatabaseDto::from).map_err(sql))
}

/// Purge de l'historique des snapshots (le journal des nettoyages est gardé).
#[tauri::command]
pub fn timeline_purge(app: AppHandle) -> Result<DatabaseDto, LumenError> {
    let dto = with_journal(&app, |j| store::purge(j.conn()).map(DatabaseDto::from).map_err(sql))?;
    let _ = app.emit("timeline:changed", ());
    Ok(dto)
}

// --- Snapshot quotidien ------------------------------------------------------------------------

#[derive(Default)]
struct DailyInner {
    running: Option<(String, Arc<AtomicBool>)>,
    last: Option<(i64, String, &'static str)>,
}

#[derive(Default)]
pub struct DailyState(Mutex<DailyInner>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyDto {
    /// `off`, `idle` (rien à faire), `running`, `waiting` (une analyse est en cours).
    pub state: &'static str,
    /// Série en cours, ou la dernière traitée.
    pub label: Option<String>,
    /// Dernier snapshot quotidien : date et issue (`done`, `failed`, `cancelled`).
    pub last_at: Option<i64>,
    pub last_outcome: Option<&'static str>,
    /// Séries analysées par la MFT : sans les droits d'administrateur, pas de snapshot
    /// quotidien (il ne demande jamais l'UAC).
    pub needs_admin: u32,
}

/// Série à reprendre en snapshot quotidien, et nombre de séries écartées faute de droits.
pub fn due<'a>(all: &'a [Series], mounted: &dyn Fn(&Series) -> bool, now: i64, elevated: bool) -> (Option<&'a Series>, u32) {
    let mut needs_admin = 0;
    let mut best: Option<&Series> = None;
    for s in all {
        let active = s.last_scan.is_some_and(|t| now - t <= DAILY_ACTIVE_MS);
        if !active || now - s.last < DAILY_AFTER_MS || !mounted(s) {
            continue;
        }
        if s.method == "mft" && !elevated {
            needs_admin += 1;
            continue;
        }
        if best.map_or(true, |b| s.last_scan > b.last_scan) {
            best = Some(s);
        }
    }
    (best, needs_admin)
}

fn daily_dto(inner: &DailyInner, state: &'static str, needs_admin: u32) -> DailyDto {
    DailyDto {
        state,
        label: inner.running.as_ref().map(|(l, _)| l.clone()).or_else(|| inner.last.as_ref().map(|(_, l, _)| l.clone())),
        last_at: inner.last.as_ref().map(|(t, _, _)| *t),
        last_outcome: inner.last.as_ref().map(|(_, _, o)| *o),
        needs_admin,
    }
}

/// Appelé par l'interface à intervalles réguliers (tant que l'appli tourne) : lance au plus un
/// snapshot quotidien en arrière-plan, sans toucher à l'analyse affichée.
#[tauri::command]
pub fn timeline_daily(app: AppHandle, state: State<'_, DailyState>, scans: State<'_, ScanState>, enabled: bool) -> Result<DailyDto, LumenError> {
    let mut inner = state.0.lock().map_err(|_| LumenError::Timeline("état indisponible".into()))?;
    if !enabled {
        if let Some((_, c)) = &inner.running {
            c.store(true, Ordering::Relaxed);
        }
        return Ok(daily_dto(&inner, "off", 0));
    }
    if inner.running.is_some() {
        return Ok(daily_dto(&inner, "running", 0));
    }
    if crate::scanning::is_running(&scans) {
        return Ok(daily_dto(&inner, "waiting", 0));
    }
    let all = with_journal(&app, |j| store::series(j.conn()).map_err(sql))?;
    let present = platform::present();
    let profile = profile_root();
    let mounted = |s: &Series| present_key(&present, &s.key).is_some_and(|p| target_of(&s.root, Some(&p.identity.letter), profile.as_ref()).is_some());
    let (pick, needs_admin) = due(&all, &mounted, unix_ms(), crate::scanning::platform::elevated());
    let Some(s) = pick else { return Ok(daily_dto(&inner, "idle", needs_admin)) };
    let Some(p) = present_key(&present, &s.key) else { return Ok(daily_dto(&inner, "idle", needs_admin)) };
    let root = match target_of(&s.root, Some(&p.identity.letter), profile.as_ref()) {
        Some(TargetDto::UserProfile) => crate::scanning::platform::user_profile().unwrap_or_default(),
        _ => base_path(&p.identity.letter, &s.root),
    };
    let label = base_path(&p.identity.letter, &s.root);
    let mft = s.method == "mft";
    let cancel = Arc::new(AtomicBool::new(false));
    inner.running = Some((label.clone(), cancel.clone()));
    let dto = daily_dto(&inner, "running", needs_admin);
    drop(inner);
    std::thread::Builder::new()
        .name("lumen-daily".into())
        .spawn(move || {
            let result = crate::scanning::platform::run(&root, mft, &cancel, &|_| {});
            let outcome = match result {
                Ok((scan, _)) if !cancel.load(Ordering::Relaxed) => match record_scan(&app, &scan, "daily") {
                    Ok(_) => "done",
                    Err(_) => "failed",
                },
                Ok(_) | Err(scan::ScanError::Cancelled) => "cancelled",
                Err(_) => "failed",
            };
            if let Ok(mut inner) = app.state::<DailyState>().0.lock() {
                inner.running = None;
                inner.last = Some((unix_ms(), label, outcome));
            }
        })
        .map_err(|e| LumenError::Timeline(e.to_string()))?;
    Ok(dto)
}

/// Une analyse lancée par l'utilisateur passe avant le snapshot quotidien.
pub(crate) fn cancel_daily(app: &AppHandle) {
    if let Ok(inner) = app.state::<DailyState>().0.lock() {
        if let Some((_, c)) = &inner.running {
            c.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests;
