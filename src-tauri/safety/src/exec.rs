//! Exécution réelle d'un plan confirmé (jalon 5), en logique pure : les effets (Corbeille,
//! suppression, commandes, espace libre) passent par [`Effects`], implémenté par `safety::win`.
//!
//! Au moment de l'action, rien n'est tenu pour acquis :
//!
//! 1. chaque élément du plan repasse les 6 étapes de [`Checker`] ;
//! 2. un dossier est relu élément par élément (handle ouvert sur le dossier lui-même, identité
//!    vérifiée) et chaque élément qu'il contient repasse à son tour liste blanche, denylist, liens,
//!    cloud, identité / taille / date et verrou, contre ce que le scan avait vu ;
//! 3. un élément apparu, modifié, protégé, lien ou verrouillé est laissé et signalé ; ses parents
//!    ne sont alors jamais traités d'un bloc : seuls les sous-ensembles intacts le sont.
//!
//! Les noms relus viennent du dossier réel déjà canonisé : ce sont des noms longs, avec leur
//! casse réelle, sans `..` ; l'étape de canonisation est donc acquise par construction.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::allow::Authority;
use crate::bin::recycle_allowed;
use crate::check::{Change, Checker, Expected, ExpectedNode, Refusal, SkipReason, Target, Validated, Verdict};
use crate::commands::OfficialCommand;
use crate::denylist::DenyReason;
use crate::path::{fold, WinPath};
use crate::plan::{CommandRole, DeleteMode, Plan};
use crate::probe::{Probe, ProbeError};

const TICKS_PER_SECOND: i64 = 10_000_000;

/// Ce que l'on supprime d'un bloc : un fichier, ou un dossier entièrement vérifié.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    pub path: WinPath,
    pub expected: Expected,
    pub bytes: u64,
    /// Dossier : son contenu vérifié, tel qu'il est sur le disque (rien d'autre n'y est).
    pub children: Vec<ExpectedNode>,
}

/// Où l'élément est parti dans la Corbeille (pour le restaurer plus tard).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Recycled {
    /// Chemin de l'élément dans la Corbeille (`…\$Recycle.Bin\<SID>\$R…`), si Windows l'a donné.
    pub bin_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RemoveError {
    #[error("{0}")]
    Failed(String),
    #[error("l'élément a changé juste avant la suppression : laissé en place")]
    Changed,
    #[error("ouvert par un autre programme")]
    Locked,
    #[error("Corbeille indisponible : {0}")]
    NoRecycleBin(String),
}

/// Les effets réels, isolés derrière une interface : `safety::win::WinEffects` sous Windows,
/// une implémentation en mémoire dans les tests.
pub trait Effects {
    /// Envoie des éléments recontrôlés à la Corbeille, en une opération ; un résultat par élément.
    fn recycle(&self, units: &[Unit]) -> Vec<Result<Recycled, RemoveError>>;
    /// Supprime définitivement un élément recontrôlé : chaque fichier et dossier est ouvert sans
    /// suivre les liens et n'est supprimé que si son identité est celle qui a été vérifiée.
    fn delete(&self, unit: &Unit) -> Result<(), RemoveError>;
    /// Espace libre du volume qui porte `path`, en octets.
    fn free_bytes(&self, path: &WinPath) -> Option<u64>;
    /// Lance une commande officielle (binaire résolu par son chemin complet, délai maximal,
    /// sorties capturées).
    fn run_command(&self, command: &OfficialCommand, cancel: &AtomicBool) -> CommandRun;
    /// Lance une commande « administrateur » de la liste fermée, élevée (UAC), par son chemin
    /// complet sous `%SystemRoot%` et avec ses arguments fixes ; seulement si le plan l'autorise.
    fn run_elevated(&self, command: &OfficialCommand, cancel: &AtomicBool) -> CommandRun;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandStatus {
    Succeeded,
    /// Code de sortie non nul.
    Failed(i32),
    TimedOut,
    /// Binaire introuvable aux emplacements connus.
    NotFound,
    /// Demande des droits d'administrateur : à lancer depuis le script, dans une console élevée.
    NeedsElevation,
    /// Élévation refusée par l'utilisateur (fenêtre UAC).
    Declined,
    Cancelled,
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRun {
    pub command: OfficialCommand,
    pub role: CommandRole,
    /// Binaire réellement lancé, par son chemin complet.
    pub program: Option<String>,
    pub args: Vec<String>,
    pub status: CommandStatus,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    /// Fichier de sortie de la commande (journal de DISM), rattaché au journal de Lumen.
    pub log: Option<String>,
}

impl CommandRun {
    /// La commande a-t-elle fait son travail ? (un point de restauration déjà récent compte.)
    pub fn satisfied(&self) -> bool {
        match self.status {
            CommandStatus::Succeeded => true,
            CommandStatus::Failed(code) => self.command.satisfied_by(code),
            _ => false,
        }
    }

    pub fn new(command: &OfficialCommand, status: CommandStatus) -> CommandRun {
        CommandRun {
            command: command.clone(),
            role: CommandRole::Action,
            program: None,
            args: command.args(),
            status,
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: 0,
            log: None,
        }
    }
}

/// Pourquoi un élément contenu dans un dossier a été laissé.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IssueReason {
    #[error("apparu depuis l'analyse")]
    New,
    #[error("modifié depuis l'analyse ({0:?})")]
    Changed(Change),
    #[error("aucune règle ni sélection ne l'autorise")]
    NotAllowed,
    #[error("zone protégée : {0}")]
    Denied(DenyReason),
    #[error("lien ou jonction : jamais suivi ni supprimé")]
    ReparsePoint,
    #[error("fichier dans le cloud : jamais touché")]
    Cloud,
    #[error("trop récent pour cette règle")]
    TooRecent,
    #[error("ouvert par un autre programme")]
    Locked,
    #[error("lecture impossible : {0}")]
    Probe(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub path: String,
    pub reason: IssueReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitReport {
    pub path: WinPath,
    pub bytes: u64,
    pub directory: bool,
    pub file_id: crate::probe::FileId,
    pub result: Result<Recycled, RemoveError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemOutcome {
    /// Tout l'élément est parti.
    Done,
    /// Une partie est partie ; le reste a changé ou a échoué (voir `issues` et `units`).
    Partial,
    /// Rien n'est parti : l'élément a changé, est verrouillé ou n'a plus rien d'intact.
    Skipped(String),
    /// Le recontrôle a refusé l'élément.
    Refused(Refusal),
    /// L'action a échoué pour tout l'élément.
    Failed(String),
    /// Annulé avant d'être traité.
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemReport {
    pub path: String,
    pub authority: Authority,
    /// Taille prévue par le plan.
    pub planned_bytes: u64,
    pub outcome: ItemOutcome,
    pub units: Vec<UnitReport>,
    pub issues: Vec<Issue>,
}

impl ItemReport {
    /// Somme estimée de ce qui est parti (tailles du scan des éléments réussis).
    pub fn freed(&self) -> u64 {
        self.units.iter().filter(|u| u.result.is_ok()).map(|u| u.bytes).sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VolumeSpace {
    pub volume: String,
    pub before: Option<u64>,
    pub after: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub mode: DeleteMode,
    pub fingerprint: [u8; 32],
    pub commands: Vec<CommandRun>,
    pub items: Vec<ItemReport>,
    pub volumes: Vec<VolumeSpace>,
    pub cancelled: bool,
    /// Arrêt avant les actions : un prérequis obligatoire n'a pas été rempli.
    pub aborted: Option<String>,
}

impl Report {
    /// Somme estimée (tailles du scan) de ce qui est parti.
    pub fn freed_estimate(&self) -> u64 {
        self.items.iter().map(ItemReport::freed).sum()
    }

    /// Gain réel mesuré par l'espace libre des volumes avant et après ; `None` si illisible.
    pub fn measured_gain(&self) -> Option<i64> {
        let mut total = 0i64;
        for v in &self.volumes {
            total += v.after? as i64 - v.before? as i64;
        }
        Some(total)
    }
}

/// Avancement, pour l'interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Command { index: usize, total: usize, command: OfficialCommand },
    CommandDone { index: usize, status: CommandStatus },
    Item { index: usize, total: usize, path: String, bytes: u64 },
    /// Nombre d'éléments recontrôlés dans l'élément en cours.
    Verified { index: usize, count: u64 },
    ItemDone { index: usize, outcome: ItemOutcome, freed: u64 },
}

enum DirResult {
    /// Rien n'a changé : le dossier peut partir d'un bloc. Contenu vérifié, tel qu'il est.
    Intact(Vec<ExpectedNode>),
    /// Seuls ces sous-ensembles intacts peuvent partir.
    Partial(Vec<Unit>),
}

struct Inspector<'a, 'c, P: Probe, F: FnMut(Event)> {
    checker: &'a Checker<'c, P>,
    authority: &'a Authority,
    min_age_secs: Option<u64>,
    issues: Vec<Issue>,
    verified: u64,
    index: usize,
    cancel: &'a AtomicBool,
    cancelled: bool,
    on_event: &'a mut F,
}

impl<P: Probe, F: FnMut(Event)> Inspector<'_, '_, P, F> {
    fn issue(&mut self, path: &WinPath, reason: IssueReason) {
        self.issues.push(Issue { path: path.display_string(), reason });
    }

    fn tick(&mut self) {
        self.verified += 1;
        if self.verified % 256 == 0 {
            (self.on_event)(Event::Verified { index: self.index, count: self.verified });
        }
        if self.cancel.load(Ordering::SeqCst) {
            self.cancelled = true;
        }
    }

    fn dir(&mut self, path: &WinPath, expected: &Expected, children: &[ExpectedNode]) -> DirResult {
        let listing = match self.checker.probe.list(path) {
            Ok(l) => l,
            Err(e) => {
                self.issue(path, IssueReason::Probe(e.to_string()));
                return DirResult::Partial(Vec::new());
            }
        };
        if listing.file_id != expected.file_id {
            self.issue(path, IssueReason::Changed(Change::Identity));
            return DirResult::Partial(Vec::new());
        }
        let mut intact = true;
        let mut verified = Vec::new();
        let mut whole = Vec::new();
        let mut partial = Vec::new();
        for name in &listing.names {
            if self.cancelled {
                return DirResult::Partial(Vec::new());
            }
            let child = match path.join(name) {
                Ok(c) => c,
                Err(e) => {
                    self.issues.push(Issue { path: format!("{path}\\{name}"), reason: IssueReason::Probe(e.to_string()) });
                    intact = false;
                    continue;
                }
            };
            let folded = fold(name);
            let Some(e) = children.iter().find(|c| fold(&c.name) == folded) else {
                self.issue(&child, IssueReason::New);
                intact = false;
                continue;
            };
            self.tick();
            match self.entry(&child, e) {
                Err(reason) => {
                    self.issue(&child, reason);
                    intact = false;
                }
                Ok(EntryResult::File) => {
                    verified.push(ExpectedNode { children: Vec::new(), ..e.clone() });
                    whole.push(Unit { path: child, expected: e.expected, bytes: e.bytes, children: Vec::new() });
                }
                Ok(EntryResult::Dir(DirResult::Intact(inner))) => {
                    verified.push(ExpectedNode { children: inner.clone(), ..e.clone() });
                    whole.push(Unit { path: child, expected: e.expected, bytes: e.bytes, children: inner });
                }
                Ok(EntryResult::Dir(DirResult::Partial(units))) => {
                    intact = false;
                    partial.extend(units);
                }
            }
        }
        if intact && !self.cancelled {
            DirResult::Intact(verified)
        } else {
            whole.extend(partial);
            DirResult::Partial(whole)
        }
    }

    /// Les étapes 2 à 6 sur un élément contenu dans un dossier déjà vérifié.
    fn entry(&mut self, path: &WinPath, e: &ExpectedNode) -> Result<EntryResult, IssueReason> {
        let c = self.checker;
        c.allow.permits(path, self.authority).ok_or(IssueReason::NotAllowed)?;
        if let Some(reason) = c.deny.check(path) {
            return Err(IssueReason::Denied(reason));
        }
        let facts = c.probe.facts(path).map_err(probe_issue)?;
        if facts.attributes.reparse_point {
            return Err(IssueReason::ReparsePoint);
        }
        if facts.attributes.cloud {
            return Err(IssueReason::Cloud);
        }
        if let Some(reason) = c.deny.check_facts(facts.attributes.system, facts.attributes.directory, facts.contains_git) {
            return Err(IssueReason::Denied(reason));
        }
        if facts.file_id != e.expected.file_id {
            return Err(IssueReason::Changed(Change::Identity));
        }
        if facts.attributes.directory != e.expected.directory {
            return Err(IssueReason::Changed(Change::Kind));
        }
        if facts.attributes.directory {
            return Ok(EntryResult::Dir(self.dir(path, &e.expected, &e.children)));
        }
        if facts.size != e.expected.size {
            return Err(IssueReason::Changed(Change::Size));
        }
        if facts.modified != e.expected.modified {
            return Err(IssueReason::Changed(Change::Modified));
        }
        if let Some(min_age) = self.min_age_secs {
            if c.now.saturating_sub(facts.modified) < (min_age as i64).saturating_mul(TICKS_PER_SECOND) {
                return Err(IssueReason::TooRecent);
            }
        }
        if c.probe.is_locked(path).map_err(probe_issue)? {
            return Err(IssueReason::Locked);
        }
        Ok(EntryResult::File)
    }
}

enum EntryResult {
    File,
    Dir(DirResult),
}

fn probe_issue(e: ProbeError) -> IssueReason {
    IssueReason::Probe(e.to_string())
}

/// Volumes à mesurer : ceux des éléments et des dossiers visés par les commandes.
fn volumes(plan: &Plan) -> Vec<WinPath> {
    let mut out: Vec<WinPath> = Vec::new();
    let paths = plan.items().iter().map(|i| &i.path).chain(plan.commands().iter().filter_map(|c| c.measured.as_ref()));
    for p in paths {
        let root = p.volume_root();
        if !out.contains(&root) {
            out.push(root);
        }
    }
    out
}

/// Exécute un plan déjà confirmé (voir [`crate::plan::execute`], seul point d'entrée).
pub(crate) fn run<P: Probe, E: Effects, F: FnMut(Event)>(
    plan: &Plan,
    checker: &Checker<'_, P>,
    effects: &E,
    cancel: &AtomicBool,
    mut on_event: F,
) -> Report {
    let vols = volumes(plan);
    let before: Vec<Option<u64>> = vols.iter().map(|v| effects.free_bytes(v)).collect();
    let mut report =
        Report {
            mode: plan.mode(),
            fingerprint: plan.fingerprint(),
            commands: Vec::new(),
            items: Vec::new(),
            volumes: Vec::new(),
            cancelled: false,
            aborted: None,
        };

    // Commandes d'abord : les prérequis (gradle --stop…) doivent précéder les suppressions.
    let total = plan.commands().len();
    for (index, planned) in plan.commands().iter().enumerate() {
        if report.aborted.is_some() || cancel.load(Ordering::SeqCst) {
            report.cancelled |= report.aborted.is_none();
            let mut run = CommandRun::new(&planned.command, CommandStatus::Cancelled);
            run.role = planned.role;
            report.commands.push(run);
            continue;
        }
        on_event(Event::Command { index, total, command: planned.command.clone() });
        let mut run = match (planned.command.needs_elevation(), plan.elevated()) {
            (false, _) => effects.run_command(&planned.command, cancel),
            (true, true) => effects.run_elevated(&planned.command, cancel),
            (true, false) => CommandRun::new(&planned.command, CommandStatus::NeedsElevation),
        };
        run.role = planned.role;
        if planned.required && !run.satisfied() {
            report.aborted = Some(format!("prérequis non rempli ({}) : rien d'autre n'a été fait", planned.command.explanation()));
        }
        on_event(Event::CommandDone { index, status: run.status.clone() });
        report.commands.push(run);
    }

    let total = plan.items().len();
    // Exemplaires d'un groupe de doublons qui sont dans le plan : jamais comptés comme gardés.
    let planned: Vec<WinPath> = plan.items().iter().map(|i| i.path.clone()).collect();
    for (index, item) in plan.items().iter().enumerate() {
        let path = item.requested.display_string();
        if report.aborted.is_some() || report.cancelled || cancel.load(Ordering::SeqCst) {
            report.cancelled |= report.aborted.is_none();
            report.items.push(ItemReport {
                path,
                authority: item.authority.clone(),
                planned_bytes: item.bytes,
                outcome: ItemOutcome::Cancelled,
                units: Vec::new(),
                issues: Vec::new(),
            });
            continue;
        }
        on_event(Event::Item { index, total, path: path.clone(), bytes: item.bytes });
        let r = run_item(plan.mode(), item, &planned, index, checker, effects, cancel, &mut on_event);
        if r.outcome == ItemOutcome::Cancelled {
            report.cancelled = true;
        }
        on_event(Event::ItemDone { index, outcome: r.outcome.clone(), freed: r.freed() });
        report.items.push(r);
    }

    report.volumes = vols
        .iter()
        .zip(before)
        .map(|(v, before)| VolumeSpace { volume: v.display_string(), before, after: effects.free_bytes(v) })
        .collect();
    report
}

#[allow(clippy::too_many_arguments)]
fn run_item<P: Probe, E: Effects, F: FnMut(Event)>(
    mode: DeleteMode,
    item: &Validated,
    planned: &[WinPath],
    index: usize,
    checker: &Checker<'_, P>,
    effects: &E,
    cancel: &AtomicBool,
    on_event: &mut F,
) -> ItemReport {
    let mut report = ItemReport {
        path: item.requested.display_string(),
        authority: item.authority.clone(),
        planned_bytes: item.bytes,
        outcome: ItemOutcome::Done,
        units: Vec::new(),
        issues: Vec::new(),
    };

    // 1. Les 6 étapes, de nouveau, sur l'élément lui-même.
    let target = Target {
        path: item.requested.verbatim(),
        authority: item.authority.clone(),
        expected: item.expected,
        bytes: item.bytes,
        contents: item.contents.clone(),
        hash: item.hash,
    };
    let fresh = match checker.check(&target) {
        Verdict::Accepted(v) => v,
        Verdict::Skipped { reason: SkipReason::Locked, .. } => {
            report.outcome = ItemOutcome::Skipped(IssueReason::Locked.to_string());
            return report;
        }
        Verdict::Refused { refusal, .. } => {
            report.outcome = match refusal {
                Refusal::Changed(_) | Refusal::TooRecent | Refusal::NotFound => ItemOutcome::Skipped(refusal.to_string()),
                other => ItemOutcome::Refused(other),
            };
            return report;
        }
    };
    if mode == DeleteMode::Trash {
        for p in [&fresh.path, &fresh.requested] {
            let ready = checker
                .probe
                .volume_facts(p)
                .map_err(|e| Refusal::Probe(e.to_string()))
                .and_then(|f| recycle_allowed(&f, fresh.bytes).map_err(Refusal::NoRecycleBin));
            if let Err(refusal) = ready {
                report.outcome = ItemOutcome::Refused(refusal);
                return report;
            }
        }
    }

    // Doublon : un autre exemplaire du groupe doit être encore intact (même identité, même
    // empreinte), puis le contenu de celui-ci est relu : s'il a changé, il est sauté.
    if let (Authority::Duplicates(id), Some(hash)) = (&fresh.authority, fresh.hash) {
        let keeper_ok = checker.allow.duplicate_group(*id, &fresh.path).is_some_and(|g| {
            g.members.iter().filter(|m| m.path != fresh.path && !planned.contains(&m.path)).any(|m| {
                checker.probe.facts(&m.path).is_ok_and(|f| {
                    !f.attributes.reparse_point
                        && f.file_id == m.expected.file_id
                        && f.size == m.expected.size
                        && f.modified == m.expected.modified
                }) && checker.probe.content_hash(&m.path).is_ok_and(|h| h == m.hash)
            })
        });
        if !keeper_ok {
            report.outcome = ItemOutcome::Skipped("aucun autre exemplaire intact : celui-ci est gardé".into());
            return report;
        }
        match checker.probe.content_hash(&fresh.path) {
            Ok(h) if h == hash => {}
            Ok(_) => {
                report.outcome = ItemOutcome::Skipped("contenu modifié depuis la recherche de doublons".into());
                return report;
            }
            Err(e) => {
                report.outcome = ItemOutcome::Skipped(e.to_string());
                return report;
            }
        }
    }

    // 2. Contenu d'un dossier : élément par élément.
    let permit = checker.allow.permits(&fresh.path, &fresh.authority);
    let units = if fresh.facts.attributes.directory {
        let mut inspector = Inspector {
            checker,
            authority: &fresh.authority,
            min_age_secs: permit.and_then(|p| p.min_age_secs),
            issues: Vec::new(),
            verified: 0,
            index,
            cancel,
            cancelled: false,
            on_event,
        };
        let contents = fresh.contents.as_deref().unwrap_or(&[]);
        let result = inspector.dir(&fresh.path, &fresh.expected, contents);
        let verified = inspector.verified;
        let cancelled = inspector.cancelled;
        report.issues = std::mem::take(&mut inspector.issues);
        on_event(Event::Verified { index, count: verified });
        if cancelled {
            report.outcome = ItemOutcome::Cancelled;
            return report;
        }
        match result {
            DirResult::Intact(children) => {
                vec![Unit { path: fresh.path.clone(), expected: fresh.expected, bytes: fresh.bytes, children }]
            }
            DirResult::Partial(units) => units,
        }
    } else {
        vec![Unit { path: fresh.path.clone(), expected: fresh.expected, bytes: fresh.bytes, children: Vec::new() }]
    };

    if units.is_empty() {
        report.outcome = ItemOutcome::Skipped("rien d'intact depuis l'analyse".into());
        return report;
    }

    // 3. Action sur les seuls ensembles vérifiés.
    let results = match mode {
        DeleteMode::Trash => effects.recycle(&units),
        DeleteMode::Permanent => units.iter().map(|u| effects.delete(u).map(|()| Recycled::default())).collect(),
    };
    for (unit, result) in units.into_iter().zip(results) {
        report.units.push(UnitReport {
            directory: unit.expected.directory,
            file_id: unit.expected.file_id,
            path: unit.path,
            bytes: unit.bytes,
            result,
        });
    }
    let ok = report.units.iter().filter(|u| u.result.is_ok()).count();
    report.outcome = if ok == report.units.len() && report.issues.is_empty() {
        ItemOutcome::Done
    } else if ok > 0 {
        ItemOutcome::Partial
    } else {
        let first = report.units.iter().find_map(|u| u.result.as_ref().err()).map(ToString::to_string);
        ItemOutcome::Failed(first.unwrap_or_default())
    };
    report
}

#[cfg(test)]
pub(crate) mod tests;
