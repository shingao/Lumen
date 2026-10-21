//! Nettoyage (jalon 5) : recommandations cochées → plan `safety` → confirmation → exécution →
//! journal. Le front n'envoie que des identifiants de règles et le mode (Corbeille ou
//! définitif) ; les chemins, identités et contenus attendus viennent de l'analyse, les
//! autorisations des règles elles-mêmes (cible développée, portée, âge minimum).
//! Le nettoyage avancé (jalon 6, `crate::advanced`) prépare ses sélections et passe par le même
//! plan, la même exécution et le même journal.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use rules::pattern::path_components;
use rules::schema::{Action, Rule, Scope as RuleScope};
use safety::allow::{Allowlist, Authority, RuleGrant, Scope};
use safety::check::{Expected, ExpectedNode, Target};
use safety::exec::{CommandStatus, Event, ItemOutcome, Report};
use safety::path::{fold, WinPath};
use safety::plan::{CommandRole, DeleteMode, Plan, PlannedCommand};
use safety::probe::FileId;
use scan::{Kind, NodeId, Scan};

use crate::analysis::Applied;
use crate::error::LumenError;
use crate::journal::{self, ActionMeta, ActionRow, CommandRow, ItemRow, Journal};

/// Éléments détaillés dans la confirmation et le bilan, au plus (les totaux restent exacts).
const MAX_LISTED: usize = 500;
const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;

/// Ce que la sélection demande à `safety`.
pub struct Selection {
    pub targets: Vec<Target>,
    pub allow: Allowlist,
    pub commands: Vec<PlannedCommand>,
    /// Règle de chaque cible, pour l'affichage.
    pub rule_names: HashMap<String, (String, String)>,
    /// Commandes « administrateur » autorisées (gouffres système, une demande UAC chacune).
    #[cfg_attr(not(windows), allow(dead_code))]
    pub elevated: bool,
}

pub(crate) fn expected(scan: &Scan, n: NodeId) -> Option<Expected> {
    let node = scan.node(n)?;
    let directory = node.kind == Kind::Dir || node.kind == Kind::CloudDir || node.attributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    Some(Expected {
        file_id: FileId { volume_serial: scan.volume_serial, id: scan.file_id(n) },
        directory,
        size: if directory { 0 } else { node.logical },
        modified: node.modified,
    })
}

pub(crate) fn bytes_of(scan: &Scan, n: NodeId) -> u64 {
    scan.node(n).map_or(0, |node| if node.flags & scan::model::F_HARDLINK_DUP != 0 { 0 } else { node.allocated })
}

/// Contenu d'un dossier tel que l'analyse l'a vu (liens et fichiers cloud compris : ils seront
/// laissés au moment de l'action).
fn tree(scan: &Scan, n: NodeId) -> Vec<ExpectedNode> {
    scan.children(n)
        .filter_map(|c| {
            let expected = expected(scan, c)?;
            let children = if scan.node(c).is_some_and(|x| x.kind == Kind::Dir) { tree(scan, c) } else { Vec::new() };
            Some(ExpectedNode { name: scan.name(c).to_owned(), expected, bytes: bytes_of(scan, c), children })
        })
        .collect()
}

/// Cible de la règle qui couvre `n` : le premier dossier (lui-même ou un parent) que la règle
/// vise, avec la portée de la règle. C'est cette autorisation que `safety` revérifie.
fn grant_for(applied: &Applied, scan: &Scan, rule: &Rule, n: NodeId) -> Option<RuleGrant> {
    let mut cur = n;
    loop {
        let path = WinPath::parse(&scan.path(cur)).ok()?;
        let hit = match &rule.project {
            Some(spec) => spec.artifacts.iter().any(|a| fold(&a.dir) == fold(scan.name(cur))),
            None => {
                let comps = path_components(&path);
                rule.targets.iter().filter_map(|t| applied.env.expand(t)).any(|p| p.matches(&comps))
            }
        };
        if hit {
            let (scope, min_age_secs) = match (&rule.project, rule.scope) {
                (Some(_), _) | (None, RuleScope::Whole) => (Scope::Whole, None),
                (None, RuleScope::Contents) => (Scope::Contents, None),
                (None, RuleScope::OlderThan { hours }) => (Scope::Contents, Some(u64::from(hours) * 3600)),
            };
            return Some(RuleGrant { rule_id: rule.id.clone(), root: path, scope, min_age_secs });
        }
        if cur == scan.root() {
            return None;
        }
        cur = scan.node(cur)?.parent;
    }
}

/// Cibles, autorisations et commandes des recommandations cochées.
pub fn select(applied: &Applied, scan: &Scan, ids: &[String]) -> Result<Selection, LumenError> {
    let set = applied.set().ok_or_else(|| LumenError::UnknownReco(ids.join(", ")))?;
    let mut sel = Selection { targets: Vec::new(), allow: Allowlist::new(), commands: Vec::new(), rule_names: HashMap::new(), elevated: false };
    for id in ids {
        let reco = applied.analysis.recos.iter().find(|r| &set.rules[r.rule].id == id).ok_or_else(|| LumenError::UnknownReco(id.clone()))?;
        let rule = &set.rules[reco.rule];
        sel.rule_names.insert(rule.id.clone(), (rule.name.fr.clone(), rule.name.en.clone()));
        let role = match rule.action {
            Action::Command => CommandRole::Action,
            Action::Delete => CommandRole::Prerequisite,
        };
        let measured = reco.covered.first().and_then(|&c| WinPath::parse(&scan.path(c)).ok());
        for (i, c) in rule.commands.iter().filter_map(|c| c.official(applied.env.system_drive())).enumerate() {
            // Gain estimé porté par la première commande de la règle seulement.
            let bytes = if role == CommandRole::Action && i == 0 { reco.allocated } else { 0 };
            sel.commands.push(PlannedCommand { command: c, role, measured: measured.clone(), bytes, required: false });
        }
        if rule.action == Action::Command {
            continue; // Mesure seulement : la commande officielle fait le travail.
        }
        for &n in &reco.covered {
            let Some(grant) = grant_for(applied, scan, rule, n) else { continue };
            sel.allow.grant(grant);
            let Some(exp) = expected(scan, n) else { continue };
            let contents = exp.directory.then(|| tree(scan, n));
            sel.targets.push(Target { path: scan.path(n), authority: Authority::Rule(rule.id.clone()), expected: exp, bytes: bytes_of(scan, n), contents, hash: None });
        }
    }
    Ok(sel)
}

// --- DTO -------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ModeDto {
    Trash,
    Permanent,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemDto {
    pub path: String,
    pub size: u64,
    pub directory: bool,
    pub rule: String,
    pub rule_name: (String, String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanCommandDto {
    pub line: String,
    pub explanation: String,
    pub prerequisite: bool,
    /// Prérequis obligatoire : s'il n'est pas rempli, rien d'autre n'est fait.
    pub required: bool,
    pub needs_elevation: bool,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeFreeDto {
    pub volume: String,
    pub free: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefusedDto {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanDto {
    pub plan_id: u32,
    pub fingerprint: String,
    pub mode: ModeDto,
    pub total: u64,
    /// Éléments à supprimer + commandes qui sont des actions.
    pub count: usize,
    pub items: Vec<PlanItemDto>,
    pub more_items: usize,
    pub commands: Vec<PlanCommandDto>,
    pub volumes: Vec<VolumeFreeDto>,
    pub refused: Vec<RefusedDto>,
    pub skipped_locked: usize,
    /// Commandes « administrateur » lancées par Lumen (une demande UAC chacune).
    pub elevated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressDto {
    pub plan_id: u32,
    /// `command`, `item`, `verify`.
    pub phase: &'static str,
    pub index: usize,
    pub total: usize,
    pub label: String,
    pub verified: u64,
    pub freed: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IssueDto {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemReportDto {
    pub path: String,
    pub planned: u64,
    pub freed: u64,
    pub outcome: &'static str,
    pub reason: Option<String>,
    pub issues: Vec<IssueDto>,
    pub more_issues: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandReportDto {
    pub line: String,
    pub status: &'static str,
    /// A fait son travail (un point de restauration déjà récent compte).
    pub satisfied: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportDto {
    pub plan_id: u32,
    pub action_id: Option<i64>,
    pub status: &'static str,
    pub mode: ModeDto,
    pub freed: u64,
    pub measured: Option<i64>,
    pub done: usize,
    pub partial: usize,
    pub skipped: usize,
    pub refused: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub items: Vec<ItemReportDto>,
    pub commands: Vec<CommandReportDto>,
    /// Arrêt avant les actions (prérequis obligatoire non rempli).
    pub aborted: Option<String>,
    pub journal_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalItemDto {
    #[serde(flatten)]
    pub item: ItemRow,
    pub restorable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalDetailDto {
    pub action: ActionRow,
    pub items: Vec<JournalItemDto>,
    pub commands: Vec<CommandRow>,
}

fn command_line(c: &safety::commands::OfficialCommand) -> String {
    format!("{} {}", c.program().binary(), c.args().join(" "))
}

fn report_dto(plan_id: u32, r: &Report, action_id: Option<i64>, journal_error: Option<String>, display: &dyn Fn(&str) -> String) -> ReportDto {
    let count = |f: fn(&ItemOutcome) -> bool| r.items.iter().filter(|i| f(&i.outcome)).count();
    ReportDto {
        plan_id,
        action_id,
        status: journal::report_status(r),
        mode: match r.mode {
            DeleteMode::Trash => ModeDto::Trash,
            DeleteMode::Permanent => ModeDto::Permanent,
        },
        freed: r.freed_estimate(),
        measured: r.measured_gain(),
        done: count(|o| *o == ItemOutcome::Done),
        partial: count(|o| *o == ItemOutcome::Partial),
        skipped: count(|o| matches!(o, ItemOutcome::Skipped(_))),
        refused: count(|o| matches!(o, ItemOutcome::Refused(_))),
        failed: count(|o| matches!(o, ItemOutcome::Failed(_))),
        cancelled: count(|o| *o == ItemOutcome::Cancelled),
        items: r
            .items
            .iter()
            .take(MAX_LISTED)
            .map(|i| ItemReportDto {
                path: display(&i.path),
                planned: i.planned_bytes,
                freed: i.freed(),
                outcome: journal::outcome_name(&i.outcome),
                reason: journal::outcome_reason(&i.outcome),
                issues: i.issues.iter().take(50).map(|x| IssueDto { path: display(&x.path), reason: x.reason.to_string() }).collect(),
                more_issues: i.issues.len().saturating_sub(50),
            })
            .collect(),
        commands: r
            .commands
            .iter()
            .map(|c| CommandReportDto {
                line: command_line(&c.command),
                status: journal::status_name(&c.status),
                satisfied: c.satisfied(),
                detail: match &c.status {
                    // Commande élevée : sorties non capturables, signification du code de sortie.
                    CommandStatus::Failed(code) => Some(match c.command.exit_meaning(*code) {
                        Some(m) => m.to_owned(),
                        None => format!("code {code} — {}", c.stderr.lines().last().unwrap_or("")),
                    }),
                    CommandStatus::Error(e) => Some(e.clone()),
                    CommandStatus::Succeeded => c.command.exit_meaning(0).map(str::to_owned),
                    _ if !c.stderr.is_empty() => Some(c.stderr.clone()),
                    _ => None,
                },
            })
            .collect(),
        aborted: r.aborted.clone(),
        journal_error,
    }
}

// --- État et commandes IPC ---------------------------------------------------------------------

struct Prepared {
    plan: Plan,
    // Autorisations revérifiées à l'exécution (Windows).
    #[cfg_attr(not(windows), allow(dead_code))]
    allow: Allowlist,
    title: String,
    /// Type de nettoyage, pour le journal.
    kind: &'static str,
}

#[derive(Default)]
struct Inner {
    next_id: u32,
    plans: HashMap<u32, Arc<Prepared>>,
    running: Option<(u32, Arc<AtomicBool>)>,
}

#[derive(Default)]
pub struct CleanState(Mutex<Inner>);

#[derive(Default)]
pub struct JournalState(Mutex<Option<Journal>>);

pub(crate) fn unix_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn now_filetime() -> i64 {
    (unix_ms() / 1000 + 11_644_473_600) * 10_000_000
}

pub(crate) fn with_journal<T>(app: &AppHandle, f: impl FnOnce(&mut Journal) -> Result<T, journal::JournalError>) -> Result<T, LumenError> {
    let state = app.state::<JournalState>();
    let mut guard = state.0.lock().map_err(|_| LumenError::Journal("journal indisponible".into()))?;
    if guard.is_none() {
        let dir = app.path().local_data_dir().map_err(|e| LumenError::Journal(e.to_string()))?;
        *guard = Some(Journal::open(&dir.join("Lumen").join("lumen.db")).map_err(|e| LumenError::Journal(e.to_string()))?);
    }
    match guard.as_mut() {
        Some(j) => f(j).map_err(|e| LumenError::Journal(e.to_string())),
        None => Err(LumenError::Journal("journal indisponible".into())),
    }
}

mod platform {
    use super::*;

    #[cfg(windows)]
    pub fn build(sel: &Selection, mode: DeleteMode) -> Result<Plan, LumenError> {
        let layout = safety::win::system_layout().map_err(|e| LumenError::Plan(e.to_string()))?;
        let deny = safety::denylist::DenyList::new(&layout);
        let checker = safety::check::Checker { probe: &safety::win::WinProbe, deny: &deny, allow: &sel.allow, now: now_filetime() };
        let plan = Plan::build(&checker, &sel.targets, mode).with_commands(sel.commands.clone());
        Ok(if sel.elevated { plan.with_elevation() } else { plan })
    }

    #[cfg(windows)]
    pub fn free(volume: &WinPath) -> Option<u64> {
        safety::exec::Effects::free_bytes(&safety::win::WinEffects::new(), volume)
    }

    #[cfg(windows)]
    pub fn script(plan: &Plan, title: &str) -> String {
        let fx = safety::win::WinEffects::new();
        safety::script::render(title, plan, |c| fx.resolve(c).map(|(e, a)| (e.display_string(), a)), unix_ms() / 1000)
    }

    #[cfg(windows)]
    pub fn execute(p: &Prepared, confirmation: &safety::plan::Confirmation, cancel: &AtomicBool, on: impl FnMut(Event)) -> Result<Report, LumenError> {
        let layout = safety::win::system_layout().map_err(|e| LumenError::Plan(e.to_string()))?;
        let deny = safety::denylist::DenyList::new(&layout);
        let checker = safety::check::Checker { probe: &safety::win::WinProbe, deny: &deny, allow: &p.allow, now: now_filetime() };
        safety::plan::execute(&p.plan, confirmation, &checker, &safety::win::WinEffects::new(), cancel, on).map_err(|e| LumenError::Plan(e.to_string()))
    }

    #[cfg(windows)]
    pub fn restorable(item: &ItemRow) -> bool {
        match (&item.recycle_item, item.volume_serial, item.file_id, item.restored_at) {
            (Some(bin), Some(v), Some(id), None) => WinPath::parse(&item.path)
                .map(|p| safety::win::in_recycle_bin(bin, &p, FileId { volume_serial: v, id }))
                .unwrap_or(false),
            _ => false,
        }
    }

    #[cfg(windows)]
    pub fn restore(item: &ItemRow) -> Result<(), LumenError> {
        let (Some(bin), Some(v), Some(id)) = (&item.recycle_item, item.volume_serial, item.file_id) else {
            return Err(LumenError::Restore("élément absent de la Corbeille".into()));
        };
        let original = WinPath::parse(&item.path).map_err(|e| LumenError::Restore(e.to_string()))?;
        safety::win::restore(bin, &original, FileId { volume_serial: v, id }).map_err(|e| LumenError::Restore(e.to_string()))
    }

    #[cfg(not(windows))]
    pub fn build(_: &Selection, _: DeleteMode) -> Result<Plan, LumenError> {
        Err(LumenError::Plan("nettoyage disponible sous Windows uniquement".into()))
    }

    #[cfg(not(windows))]
    pub fn free(_: &WinPath) -> Option<u64> {
        None
    }

    #[cfg(not(windows))]
    pub fn script(plan: &Plan, title: &str) -> String {
        safety::script::render(title, plan, |_| None, unix_ms() / 1000)
    }

    #[cfg(not(windows))]
    pub fn execute(_: &Prepared, _: &safety::plan::Confirmation, _: &AtomicBool, _: impl FnMut(Event)) -> Result<Report, LumenError> {
        Err(LumenError::Plan("nettoyage disponible sous Windows uniquement".into()))
    }

    #[cfg(not(windows))]
    pub fn restorable(_: &ItemRow) -> bool {
        false
    }

    #[cfg(not(windows))]
    pub fn restore(_: &ItemRow) -> Result<(), LumenError> {
        Err(LumenError::Restore("Windows uniquement".into()))
    }
}

fn plan_dto(id: u32, p: &Prepared, sel: &Selection, display: &dyn Fn(&str) -> String) -> PlanDto {
    let plan = &p.plan;
    let mut volumes: Vec<WinPath> = Vec::new();
    for v in plan.items().iter().map(|i| i.path.volume_root()).chain(plan.commands().iter().filter_map(|c| c.measured.as_ref().map(WinPath::volume_root))) {
        if !volumes.contains(&v) {
            volumes.push(v);
        }
    }
    PlanDto {
        plan_id: id,
        fingerprint: plan.fingerprint_hex(),
        mode: match plan.mode() {
            DeleteMode::Trash => ModeDto::Trash,
            DeleteMode::Permanent => ModeDto::Permanent,
        },
        total: plan.total_bytes(),
        count: plan.items().len() + plan.commands().iter().filter(|c| c.role == CommandRole::Action).count(),
        items: plan
            .items()
            .iter()
            .take(MAX_LISTED)
            .map(|i| {
                let rule = match &i.authority {
                    Authority::Rule(r) => r.clone(),
                    Authority::Selection(s) | Authority::Duplicates(s) => s.to_string(),
                };
                PlanItemDto {
                    path: display(&i.requested.display_string()),
                    size: i.bytes,
                    directory: i.facts.attributes.directory,
                    rule_name: sel.rule_names.get(&rule).cloned().unwrap_or_default(),
                    rule,
                }
            })
            .collect(),
        more_items: plan.items().len().saturating_sub(MAX_LISTED),
        commands: plan
            .commands()
            .iter()
            .map(|c| PlanCommandDto {
                line: command_line(&c.command),
                explanation: c.command.explanation().to_owned(),
                prerequisite: c.role == CommandRole::Prerequisite,
                required: c.required,
                needs_elevation: c.command.needs_elevation(),
                size: c.bytes,
            })
            .collect(),
        volumes: volumes.iter().map(|v| VolumeFreeDto { volume: v.display_string(), free: platform::free(v) }).collect(),
        refused: plan.refused().iter().take(MAX_LISTED).map(|(path, r)| RefusedDto { path: display(path), reason: r.to_string() }).collect(),
        skipped_locked: plan.skipped().len(),
        elevated: plan.elevated(),
    }
}

/// Plan des recommandations cochées, à confirmer (rien n'est fait ici).
#[tauri::command]
pub fn plan_create(
    scans: State<'_, crate::scanning::ScanState>,
    state: State<'_, CleanState>,
    scan_id: u32,
    recos: Vec<String>,
    mode: ModeDto,
) -> Result<PlanDto, LumenError> {
    let entry = crate::scanning::entry_of(&scans, scan_id)?;
    let applied = entry.applied.as_ref().ok_or_else(|| LumenError::Plan("règles non appliquées".into()))?;
    let sel = select(applied, &entry.scan, &recos)?;
    let mode = delete_mode(mode);
    let names: Vec<&str> = recos.iter().filter_map(|r| sel.rule_names.get(r)).map(|(fr, _)| fr.as_str()).collect();
    let title = if names.len() <= 3 { names.join(", ") } else { format!("{} recommandations", names.len()) };
    prepare(&state, &sel, mode, title, "recos", &|p| applied.display(p))
}

pub(crate) fn delete_mode(mode: ModeDto) -> DeleteMode {
    match mode {
        ModeDto::Trash => DeleteMode::Trash,
        ModeDto::Permanent => DeleteMode::Permanent,
    }
}

/// Construit le plan d'une sélection et le garde pour la confirmation (rien n'est fait ici).
pub(crate) fn prepare(
    state: &CleanState,
    sel: &Selection,
    mode: DeleteMode,
    title: String,
    kind: &'static str,
    display: &dyn Fn(&str) -> String,
) -> Result<PlanDto, LumenError> {
    let plan = platform::build(sel, mode)?;
    let prepared = Prepared { plan, allow: sel.allow.clone(), title, kind };
    let mut inner = state.0.lock().map_err(|_| LumenError::Plan("état indisponible".into()))?;
    inner.next_id += 1;
    let id = inner.next_id;
    let dto = plan_dto(id, &prepared, sel, display);
    // Seul le dernier plan affiché peut être confirmé.
    inner.plans.clear();
    inner.plans.insert(id, Arc::new(prepared));
    Ok(dto)
}

/// Script PowerShell équivalent au plan (simulation par défaut, garde-fous embarqués).
#[tauri::command]
pub fn plan_script(state: State<'_, CleanState>, plan_id: u32) -> Result<String, LumenError> {
    let p = state.0.lock().ok().and_then(|i| i.plans.get(&plan_id).cloned()).ok_or(LumenError::UnknownPlan)?;
    Ok(platform::script(&p.plan, &p.title))
}

fn parse_fingerprint(hex: &str) -> Option<[u8; 32]> {
    let bytes: Vec<u8> = (0..hex.len()).step_by(2).filter_map(|i| hex.get(i..i + 2)).filter_map(|b| u8::from_str_radix(b, 16).ok()).collect();
    bytes.try_into().ok().filter(|_| hex.len() == 64)
}

/// Exécution réelle d'un plan confirmé : empreinte du plan affiché, seconde confirmation pour
/// le définitif. Avancement par `clean:progress`, bilan par `clean:done`.
#[tauri::command]
pub fn plan_execute(app: AppHandle, state: State<'_, CleanState>, plan_id: u32, fingerprint: String, permanent_acknowledged: bool) -> Result<(), LumenError> {
    let fingerprint = parse_fingerprint(&fingerprint).ok_or(LumenError::Plan("empreinte invalide".into()))?;
    let confirmation = safety::plan::Confirmation { fingerprint, permanent_acknowledged };
    let cancel = Arc::new(AtomicBool::new(false));
    let prepared = {
        let mut inner = state.0.lock().map_err(|_| LumenError::Plan("état indisponible".into()))?;
        if inner.running.is_some() {
            return Err(LumenError::CleanRunning);
        }
        let p = inner.plans.get(&plan_id).cloned().ok_or(LumenError::UnknownPlan)?;
        safety::plan::verify(&p.plan, &confirmation).map_err(|e| LumenError::Plan(e.to_string()))?;
        inner.running = Some((plan_id, cancel.clone()));
        p
    };
    std::thread::Builder::new()
        .name("lumen-clean".into())
        .spawn(move || {
            let started = unix_ms();
            let mut freed = 0u64;
            let mut label = String::new();
            let progress = |e: Event| {
                let (phase, index, total, verified) = match e {
                    Event::Command { index, total, command } => {
                        label = command_line(&command);
                        ("command", index, total, 0)
                    }
                    Event::Item { index, total, path, .. } => {
                        label = path;
                        ("item", index, total, 0)
                    }
                    Event::Verified { index, count } => ("verify", index, 0, count),
                    Event::ItemDone { index, freed: f, .. } => {
                        freed += f;
                        ("item", index + 1, 0, 0)
                    }
                    Event::CommandDone { index, .. } => ("command", index + 1, 0, 0),
                };
                let _ = app.emit("clean:progress", ProgressDto { plan_id, phase, index, total, label: label.clone(), verified, freed });
            };
            let result = platform::execute(&prepared, &confirmation, &cancel, progress);
            let display = |p: &str| p.to_owned();
            let dto = match result {
                Ok(report) => {
                    let script = platform::script(&prepared.plan, &prepared.title);
                    let meta = ActionMeta { kind: prepared.kind, started_at: started, finished_at: unix_ms(), title: &prepared.title, planned_bytes: prepared.plan.total_bytes(), script: &script };
                    let (id, err) = match with_journal(&app, |j| j.record(&meta, &report)) {
                        Ok(id) => (Some(id), None),
                        Err(e) => (None, Some(e.to_string())),
                    };
                    Ok(report_dto(plan_id, &report, id, err, &display))
                }
                Err(e) => Err(e.to_string()),
            };
            if let Ok(mut inner) = app.state::<CleanState>().0.lock() {
                inner.running = None;
                // Un plan exécuté ne peut pas l'être une seconde fois.
                inner.plans.remove(&plan_id);
            }
            match dto {
                Ok(d) => {
                    let _ = app.emit("clean:done", d);
                }
                Err(message) => {
                    let _ = app.emit("clean:failed", serde_json::json!({ "planId": plan_id, "message": message }));
                }
            }
        })
        .map_err(|e| LumenError::Startup(e.to_string()))?;
    Ok(())
}

/// Annulation : prise en compte entre deux éléments (ou pendant le recontrôle d'un dossier).
#[tauri::command]
pub fn plan_cancel(state: State<'_, CleanState>, plan_id: u32) {
    if let Ok(inner) = state.0.lock() {
        if let Some((id, cancel)) = &inner.running {
            if *id == plan_id {
                cancel.store(true, Ordering::SeqCst);
            }
        }
    }
}

#[tauri::command]
pub fn journal_list(app: AppHandle) -> Result<Vec<ActionRow>, LumenError> {
    with_journal(&app, |j| j.actions(200))
}

#[tauri::command]
pub fn journal_detail(app: AppHandle, action_id: i64) -> Result<JournalDetailDto, LumenError> {
    let (action, items, commands) = with_journal(&app, |j| {
        let action = j.actions(u32::MAX)?.into_iter().find(|a| a.id == action_id);
        Ok((action, j.items(action_id, MAX_LISTED as u32)?, j.commands(action_id)?))
    })?;
    let action = action.ok_or(LumenError::UnknownAction)?;
    let items = items.into_iter().map(|item| JournalItemDto { restorable: platform::restorable(&item), item }).collect();
    Ok(JournalDetailDto { action, items, commands })
}

#[tauri::command]
pub fn journal_script(app: AppHandle, action_id: i64) -> Result<String, LumenError> {
    with_journal(&app, |j| j.script(action_id))?.ok_or(LumenError::UnknownAction)
}

/// « Restaurer depuis la Corbeille » : seulement un élément envoyé par Lumen, encore dans la
/// Corbeille, vers son emplacement d'origine libre.
#[tauri::command]
pub fn journal_restore(app: AppHandle, item_id: i64) -> Result<(), LumenError> {
    let item = with_journal(&app, |j| j.item(item_id))?.ok_or(LumenError::UnknownAction)?;
    if item.restored_at.is_some() {
        return Err(LumenError::Restore("déjà restauré".into()));
    }
    platform::restore(&item)?;
    with_journal(&app, |j| j.mark_restored(item_id, unix_ms()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tests::applied;

    #[test]
    fn selection_cibles_autorisations_et_commandes() {
        let (a, scan) = applied();
        let sel = select(&a, &scan, &["caches-shaders".into(), "cache-npm".into()]).unwrap();
        // Shaders : contenu de D3DSCache, autorisé par la cible de la règle (portée « contenu »).
        let paths: Vec<&str> = sel.targets.iter().map(|t| t.path.as_str()).collect();
        assert_eq!(paths, ["C:\\Users\\Arthur\\AppData\\Local\\D3DSCache\\a.bin", "C:\\Users\\Arthur\\AppData\\Local\\D3DSCache\\b.bin"]);
        let root = WinPath::parse("C:\\Users\\Arthur\\AppData\\Local\\D3DSCache").unwrap();
        let shaders = Authority::Rule("caches-shaders".into());
        assert!(sel.allow.permits(&root.join("a.bin").unwrap(), &shaders).is_some());
        assert!(sel.allow.permits(&root, &shaders).is_none(), "le dossier lui-même reste");
        assert!(sel.targets.iter().all(|t| t.contents.is_none() && t.expected.size > 0));
        // npm : commande officielle, mesurée sur le cache, jamais de suppression par Lumen.
        assert!(sel.targets.iter().all(|t| t.authority == shaders));
        let cmds: Vec<(&str, CommandRole, u64)> = sel.commands.iter().map(|c| (c.command.explanation(), c.role, c.bytes)).collect();
        assert_eq!(cmds[0], ("Outil officiel : vide le cache npm", CommandRole::Action, 1000));
        assert_eq!(cmds[1].2, 0, "le gain n'est compté qu'une fois");
        assert_eq!(sel.commands[0].measured.as_ref().unwrap().display_string(), "C:\\Users\\Arthur\\AppData\\Local\\npm-cache");
        assert!(matches!(select(&a, &scan, &["inconnue".into()]), Err(LumenError::UnknownReco(_))));
    }

    #[test]
    fn contenu_attendu_des_dossiers() {
        let (a, scan) = applied();
        let shaders = scan.find("AppData\\Local\\D3DSCache").unwrap();
        let t = tree(&scan, shaders);
        assert_eq!(t.iter().map(|n| (n.name.as_str(), n.bytes, n.expected.directory)).collect::<Vec<_>>(), [("a.bin", 300, false), ("b.bin", 200, false)]);
        let rule = a.set().unwrap().rules.iter().find(|r| r.id == "caches-shaders").unwrap();
        let g = grant_for(&a, &scan, rule, scan.find("AppData\\Local\\D3DSCache\\a.bin").unwrap()).unwrap();
        assert_eq!((g.root.display_string().as_str(), g.scope), ("C:\\Users\\Arthur\\AppData\\Local\\D3DSCache", Scope::Contents));
    }

    #[test]
    fn empreinte_hexadecimale() {
        let hex = "ab".repeat(32);
        assert_eq!(parse_fingerprint(&hex), Some([0xAB; 32]));
        assert_eq!(parse_fingerprint("ab"), None);
        assert_eq!(parse_fingerprint(&"zz".repeat(32)), None);
    }
}
