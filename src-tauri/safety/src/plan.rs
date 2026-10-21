//! Plan d'action : la liste exacte de ce qui sera fait, validée élément par élément, avec une
//! empreinte BLAKE3 (éléments, contenu attendu de chaque dossier, commandes). Une exécution
//! n'est acceptée qu'avec la confirmation de cette empreinte, et tout est recontrôlé au moment
//! de l'action (voir [`crate::exec`]).

use std::sync::atomic::AtomicBool;

use crate::allow::{Allowlist, Authority};
use crate::bin::recycle_allowed;
use crate::check::{Checker, ExpectedNode, Refusal, SkipReason, Target, Validated, Verdict};
use crate::commands::OfficialCommand;
use crate::exec::{Effects, Event, Report};
use crate::path::WinPath;
use crate::probe::{FileId, Probe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteMode {
    /// Corbeille (par défaut), annulable.
    Trash,
    /// Suppression définitive : option explicite, double confirmation.
    Permanent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandRole {
    /// Lancée avant les suppressions (ex. `gradle --stop` avant de toucher aux caches Gradle).
    Prerequisite,
    /// La commande est l'action elle-même (ex. `npm cache clean --force`).
    Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCommand {
    pub command: OfficialCommand,
    pub role: CommandRole,
    /// Dossier dont la commande libère l'espace : son volume est mesuré avant et après.
    pub measured: Option<WinPath>,
    /// Taille estimée par le scan.
    pub bytes: u64,
    /// Prérequis obligatoire : s'il n'est pas rempli, rien d'autre n'est fait.
    pub required: bool,
}

impl PlannedCommand {
    pub fn action(command: OfficialCommand, measured: Option<WinPath>, bytes: u64) -> PlannedCommand {
        PlannedCommand { command, role: CommandRole::Action, measured, bytes, required: false }
    }

    pub fn prerequisite(command: OfficialCommand, required: bool) -> PlannedCommand {
        PlannedCommand { command, role: CommandRole::Prerequisite, measured: None, bytes: 0, required }
    }
}

#[derive(Debug, Clone)]
pub struct Plan {
    mode: DeleteMode,
    items: Vec<Validated>,
    commands: Vec<PlannedCommand>,
    /// Commandes « administrateur » autorisées (gouffres système) : lancées élevées.
    elevated: bool,
    /// Programmes qui ne doivent pas tourner au moment de l'action (jeux, launchers).
    guard: Option<crate::guard::ProcessGuard>,
    skipped: Vec<(WinPath, SkipReason)>,
    refused: Vec<(String, Refusal)>,
    fingerprint: [u8; 32],
}

impl Plan {
    /// Contrôle chaque cible ; seules les acceptées entrent dans le plan.
    pub fn build<P: Probe>(checker: &Checker<'_, P>, targets: &[Target], mode: DeleteMode) -> Plan {
        let mut items = Vec::new();
        let mut skipped = Vec::new();
        let mut refused = Vec::new();
        for t in targets {
            match checker.check(t) {
                Verdict::Accepted(v) => {
                    // Corbeille : pré-contrôle du volume du chemin réel ET de celui demandé (un
                    // lecteur SUBST ou réseau n'a pas de Corbeille). Sinon refus, jamais destruction.
                    if mode == DeleteMode::Trash {
                        if let Err(refusal) = recycle_ready(checker.probe, &v) {
                            refused.push((t.path.clone(), refusal));
                            continue;
                        }
                    }
                    // Un même élément ne figure qu'une fois, et jamais en plus d'un de ses parents.
                    if items.iter().any(|i: &Validated| v.path.starts_with(&i.path)) {
                        continue;
                    }
                    items.retain(|i: &Validated| !i.path.starts_with(&v.path));
                    items.push(v);
                }
                Verdict::Skipped { path, reason } => skipped.push((path, reason)),
                Verdict::Refused { path, refusal } => refused.push((path, refusal)),
            }
        }
        let items = protect_duplicates(checker.allow, items, &mut refused);
        let mut plan = Plan { mode, items, commands: Vec::new(), elevated: false, guard: None, skipped, refused, fingerprint: [0; 32] };
        plan.fingerprint = fingerprint(&plan);
        plan
    }

    /// Ajoute les commandes officielles (prérequis et actions) ; l'empreinte les couvre.
    pub fn with_commands(mut self, commands: Vec<PlannedCommand>) -> Plan {
        for c in commands {
            if !self.commands.iter().any(|k| k.command == c.command) {
                self.commands.push(c);
            }
        }
        // Prérequis d'abord, dans l'ordre donné.
        self.commands.sort_by_key(|c| c.role != CommandRole::Prerequisite);
        self.fingerprint = fingerprint(&self);
        self
    }

    /// Autorise le lancement élevé (UAC) des commandes « administrateur » du plan. L'empreinte
    /// le couvre : une confirmation donnée sans élévation ne vaut pas pour un plan élevé.
    pub fn with_elevation(mut self) -> Plan {
        self.elevated = true;
        self.fingerprint = fingerprint(&self);
        self
    }

    pub fn elevated(&self) -> bool {
        self.elevated
    }

    /// Refuse toute l'action si l'un de ces programmes tourne au moment de l'exécution (vérifié
    /// avant la première commande et le premier élément). L'empreinte la couvre.
    pub fn with_process_guard(mut self, guard: crate::guard::ProcessGuard) -> Plan {
        self.guard = Some(guard);
        self.fingerprint = fingerprint(&self);
        self
    }

    pub fn guard(&self) -> Option<&crate::guard::ProcessGuard> {
        self.guard.as_ref()
    }

    /// Actions système sans point de restauration obligatoire avant elles.
    pub fn without_restore_point(&self) -> bool {
        self.elevated
            && self.commands.iter().any(|c| c.command.needs_elevation())
            && !self.commands.iter().any(|c| c.required && c.command == crate::commands::OfficialCommand::RestorePointCreate)
    }

    pub fn mode(&self) -> DeleteMode {
        self.mode
    }

    pub fn items(&self) -> &[Validated] {
        &self.items
    }

    pub fn commands(&self) -> &[PlannedCommand] {
        &self.commands
    }

    pub fn skipped(&self) -> &[(WinPath, SkipReason)] {
        &self.skipped
    }

    pub fn refused(&self) -> &[(String, Refusal)] {
        &self.refused
    }

    /// Total affiché : éléments du plan et estimation des commandes qui sont des actions.
    pub fn total_bytes(&self) -> u64 {
        self.items.iter().map(|i| i.bytes).sum::<u64>()
            + self.commands.iter().filter(|c| c.role == CommandRole::Action).map(|c| c.bytes).sum::<u64>()
    }

    pub fn fingerprint(&self) -> [u8; 32] {
        self.fingerprint
    }

    pub fn fingerprint_hex(&self) -> String {
        hex(&self.fingerprint)
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty() && self.commands.is_empty()
    }
}

/// Doublons : jamais tous les exemplaires d'un groupe, jamais des liens physiques du même fichier
/// (en supprimer un ne libère rien, et l'« exemplaire gardé » serait le même fichier).
fn protect_duplicates(allow: &Allowlist, items: Vec<Validated>, refused: &mut Vec<(String, Refusal)>) -> Vec<Validated> {
    let group_of = |v: &Validated| match v.authority {
        Authority::Duplicates(id) => allow.duplicate_group(id, &v.path),
        _ => None,
    };
    let mut kept = Vec::with_capacity(items.len());
    for v in &items {
        let Some(group) = group_of(v) else {
            kept.push(v.clone());
            continue;
        };
        let mut ids: Vec<FileId> = group.members.iter().map(|m| m.expected.file_id).collect();
        ids.sort_by_key(|f| (f.volume_serial, f.id));
        ids.dedup();
        let refusal = if ids.len() < group.members.len() {
            Some(Refusal::HardLink)
        } else if group.members.iter().all(|m| items.iter().any(|i| i.path == m.path && group_of(i).is_some())) {
            Some(Refusal::LastCopy)
        } else {
            None
        };
        match refusal {
            Some(r) => refused.push((v.requested.display_string(), r)),
            None => kept.push(v.clone()),
        }
    }
    kept
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn recycle_ready<P: Probe>(probe: &P, v: &Validated) -> Result<(), Refusal> {
    for p in [&v.path, &v.requested] {
        let facts = probe.volume_facts(p).map_err(|e| Refusal::Probe(e.to_string()))?;
        recycle_allowed(&facts, v.bytes).map_err(Refusal::NoRecycleBin)?;
    }
    Ok(())
}

fn hash_str(h: &mut blake3::Hasher, s: &str) {
    h.update(&(s.len() as u64).to_le_bytes());
    h.update(s.as_bytes());
}

fn hash_nodes(h: &mut blake3::Hasher, nodes: &[ExpectedNode]) {
    h.update(&(nodes.len() as u64).to_le_bytes());
    for n in nodes {
        hash_str(h, &n.name);
        h.update(&n.expected.file_id.volume_serial.to_le_bytes());
        h.update(&n.expected.file_id.id.to_le_bytes());
        h.update(&[u8::from(n.expected.directory)]);
        h.update(&n.expected.size.to_le_bytes());
        h.update(&n.expected.modified.to_le_bytes());
        h.update(&n.bytes.to_le_bytes());
        hash_nodes(h, &n.children);
    }
}

fn fingerprint(plan: &Plan) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"lumen-plan-v2");
    h.update(&[match plan.mode {
        DeleteMode::Trash => 0,
        DeleteMode::Permanent => 1,
    }]);
    h.update(&(plan.items.len() as u64).to_le_bytes());
    for i in &plan.items {
        hash_str(&mut h, &i.path.verbatim());
        hash_str(&mut h, &i.requested.verbatim());
        h.update(&i.facts.file_id.volume_serial.to_le_bytes());
        h.update(&i.facts.file_id.id.to_le_bytes());
        h.update(&i.facts.size.to_le_bytes());
        h.update(&i.facts.modified.to_le_bytes());
        h.update(&i.bytes.to_le_bytes());
        h.update(&i.hash.unwrap_or([0; 32]));
        match &i.contents {
            Some(nodes) => {
                h.update(&[1]);
                hash_nodes(&mut h, nodes);
            }
            None => {
                h.update(&[0]);
            }
        }
    }
    h.update(&[u8::from(plan.elevated)]);
    match &plan.guard {
        Some(g) => {
            h.update(&[1]);
            h.update(&(g.names().len() as u64).to_le_bytes());
            for n in g.names() {
                hash_str(&mut h, n);
            }
            h.update(&(g.dirs().len() as u64).to_le_bytes());
            for d in g.dirs() {
                hash_str(&mut h, &d.verbatim());
            }
        }
        None => {
            h.update(&[0]);
        }
    }
    h.update(&(plan.commands.len() as u64).to_le_bytes());
    for c in &plan.commands {
        hash_str(&mut h, c.command.program().binary());
        for a in c.command.args() {
            hash_str(&mut h, &a);
        }
        h.update(&[u8::from(c.role == CommandRole::Prerequisite), u8::from(c.required)]);
        hash_str(&mut h, &c.measured.as_ref().map(WinPath::verbatim).unwrap_or_default());
        h.update(&c.bytes.to_le_bytes());
    }
    *h.finalize().as_bytes()
}

/// Ce que la simulation montre avant toute confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DryRun {
    pub items: usize,
    pub bytes: u64,
    pub skipped_locked: usize,
    pub refused: usize,
    /// Une ligne par élément, en français.
    pub lines: Vec<String>,
}

pub fn simulate(plan: &Plan) -> DryRun {
    let verb = match plan.mode {
        DeleteMode::Trash => "irait à la Corbeille",
        DeleteMode::Permanent => "serait supprimé définitivement",
    };
    let mut lines: Vec<String> =
        plan.commands.iter().map(|c| format!("{} {} — serait lancé", c.command.program().binary(), c.command.args().join(" "))).collect();
    lines.extend(plan.items.iter().map(|i| format!("{} — {verb}", i.path)));
    lines.extend(plan.skipped.iter().map(|(p, _)| format!("{p} — ignoré : ouvert par un autre programme")));
    lines.extend(plan.refused.iter().map(|(p, r)| format!("{p} — refusé : {r}")));
    DryRun {
        items: plan.items.len(),
        bytes: plan.total_bytes(),
        skipped_locked: plan.skipped.len(),
        refused: plan.refused.len(),
        lines,
    }
}

/// Accord explicite de l'utilisateur sur un plan précis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Confirmation {
    pub fingerprint: [u8; 32],
    /// Deuxième confirmation, obligatoire pour une suppression définitive.
    pub permanent_acknowledged: bool,
    /// « Continuer sans point de restauration » : obligatoire pour des actions système lancées
    /// sans point de restauration préalable (protection désactivée, ou choix de l'utilisateur).
    pub without_restore_point: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ExecError {
    #[error("le plan a changé depuis sa confirmation : rien n'a été fait")]
    FingerprintMismatch,
    #[error("la suppression définitive demande une seconde confirmation")]
    PermanentNotAcknowledged,
    #[error("rien à faire")]
    EmptyPlan,
    #[error("actions système sans point de restauration : il faut confirmer « Continuer sans point de restauration »")]
    RestorePointNotWaived,
}

/// Vérifie qu'une confirmation porte sur ce plan précis (empreinte, seconde confirmation).
pub fn verify(plan: &Plan, confirmation: &Confirmation) -> Result<(), ExecError> {
    if confirmation.fingerprint != plan.fingerprint {
        return Err(ExecError::FingerprintMismatch);
    }
    if plan.mode == DeleteMode::Permanent && !confirmation.permanent_acknowledged {
        return Err(ExecError::PermanentNotAcknowledged);
    }
    if plan.is_empty() {
        return Err(ExecError::EmptyPlan);
    }
    if plan.without_restore_point() && !confirmation.without_restore_point {
        return Err(ExecError::RestorePointNotWaived);
    }
    Ok(())
}

/// Point d'entrée unique de l'exécution réelle : confirmation vérifiée, puis recontrôle complet
/// de chaque élément au moment de l'action ; tout ce qui a changé est sauté et signalé.
pub fn execute<P: Probe, E: Effects>(
    plan: &Plan,
    confirmation: &Confirmation,
    checker: &Checker<'_, P>,
    effects: &E,
    cancel: &AtomicBool,
    on_event: impl FnMut(Event),
) -> Result<Report, ExecError> {
    verify(plan, confirmation)?;
    Ok(crate::exec::run(plan, checker, effects, cancel, on_event))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allow::{Allowlist, Authority, RuleGrant, Scope};
    use crate::denylist::{test_layout, DenyList};
    use crate::probe::memory::MemoryProbe;

    fn setup() -> (MemoryProbe, DenyList, Allowlist) {
        let probe = MemoryProbe::new();
        for d in ["C:\\Users", "C:\\Users\\Arthur", "C:\\Users\\Arthur\\cache", "C:\\Users\\Arthur\\cache\\sous"] {
            probe.dir(d);
        }
        probe.file("C:\\Users\\Arthur\\cache\\a.bin", 100);
        probe.file("C:\\Users\\Arthur\\cache\\sous\\b.bin", 50);
        let mut allow = Allowlist::new();
        allow.grant(RuleGrant {
            rule_id: "cache".into(),
            root: WinPath::parse("C:\\Users\\Arthur\\cache").unwrap(),
            scope: Scope::Contents,
            min_age_secs: None,
        });
        (probe, DenyList::new(&test_layout()), allow)
    }

    fn target(probe: &MemoryProbe, raw: &str) -> Target {
        probe.target(raw, Authority::Rule("cache".into()))
    }

    fn plan_of(raws: &[&str], mode: DeleteMode) -> Plan {
        let (probe, deny, allow) = setup();
        let checker = Checker { probe: &probe, deny: &deny, allow: &allow, now: i64::MAX };
        let targets: Vec<Target> = raws.iter().map(|r| target(&probe, r)).collect();
        Plan::build(&checker, &targets, mode)
    }

    #[test]
    fn simulation_et_refus() {
        let plan = plan_of(&["C:\\Users\\Arthur\\cache\\a.bin", "C:\\Users\\Arthur"], DeleteMode::Trash);
        let dry = simulate(&plan);
        assert_eq!((dry.items, dry.bytes, dry.refused), (1, 100, 1));
        assert!(dry.lines[0].ends_with("irait à la Corbeille"));
        assert!(dry.lines[1].contains("refusé"));
    }

    #[test]
    fn pas_de_doublon_ni_enfant_d_un_parent_deja_present() {
        let plan = plan_of(
            &["C:\\Users\\Arthur\\cache\\sous\\b.bin", "C:\\Users\\Arthur\\cache\\sous", "C:\\Users\\Arthur\\cache\\sous"],
            DeleteMode::Trash,
        );
        assert_eq!(plan.items().len(), 1);
        assert_eq!(plan.items()[0].path.display_string(), "C:\\Users\\Arthur\\cache\\sous");
    }

    #[test]
    fn empreinte_stable_et_sensible() {
        let a = plan_of(&["C:\\Users\\Arthur\\cache\\a.bin"], DeleteMode::Trash);
        let b = plan_of(&["C:\\Users\\Arthur\\cache\\a.bin"], DeleteMode::Trash);
        let c = plan_of(&["C:\\Users\\Arthur\\cache\\a.bin"], DeleteMode::Permanent);
        let d = plan_of(&["C:\\Users\\Arthur\\cache\\a.bin", "C:\\Users\\Arthur\\cache\\sous"], DeleteMode::Trash);
        assert_eq!(a.fingerprint(), b.fingerprint());
        assert_ne!(a.fingerprint(), c.fingerprint());
        assert_ne!(a.fingerprint(), d.fingerprint());
        assert_eq!(a.fingerprint_hex().len(), 64);
    }

    #[test]
    fn confirmation_de_l_empreinte_exigee() {
        let plan = plan_of(&["C:\\Users\\Arthur\\cache\\a.bin"], DeleteMode::Trash);
        assert_eq!(verify(&plan, &Confirmation { fingerprint: plan.fingerprint(), permanent_acknowledged: false, without_restore_point: false }), Ok(()));
        let wrong = Confirmation { fingerprint: [0; 32], permanent_acknowledged: true, without_restore_point: false };
        assert_eq!(verify(&plan, &wrong), Err(ExecError::FingerprintMismatch));
        let empty = plan_of(&[], DeleteMode::Trash);
        assert_eq!(verify(&empty, &Confirmation { fingerprint: empty.fingerprint(), permanent_acknowledged: false, without_restore_point: false }), Err(ExecError::EmptyPlan));
    }

    #[test]
    fn dossier_sans_contenu_connu_refuse() {
        let (probe, deny, allow) = setup();
        let checker = Checker { probe: &probe, deny: &deny, allow: &allow, now: i64::MAX };
        let mut t = target(&probe, "C:\\Users\\Arthur\\cache\\sous");
        t.contents = None;
        let plan = Plan::build(&checker, &[t], DeleteMode::Trash);
        assert_eq!(plan.refused()[0].1, Refusal::ContentsUnknown);
    }

    #[test]
    fn empreinte_couvre_contenu_et_commandes() {
        use crate::commands::OfficialCommand;
        let (probe, deny, allow) = setup();
        let checker = Checker { probe: &probe, deny: &deny, allow: &allow, now: i64::MAX };
        let t = target(&probe, "C:\\Users\\Arthur\\cache\\sous");
        let a = Plan::build(&checker, std::slice::from_ref(&t), DeleteMode::Trash);
        let mut changed = t.clone();
        changed.contents.as_mut().unwrap()[0].expected.size += 1;
        let b = Plan::build(&checker, &[changed], DeleteMode::Trash);
        assert_ne!(a.fingerprint(), b.fingerprint());
        let cmd = PlannedCommand::prerequisite(OfficialCommand::GradleStop, false);
        let action = PlannedCommand::action(OfficialCommand::NpmCacheClean, None, 7);
        let c = Plan::build(&checker, &[t], DeleteMode::Trash).with_commands(vec![action.clone(), cmd.clone(), cmd]);
        assert_ne!(a.fingerprint(), c.fingerprint());
        // Prérequis en tête, sans doublon ; l'action compte dans le total.
        assert_eq!(c.commands().iter().map(|k| k.role).collect::<Vec<_>>(), [CommandRole::Prerequisite, CommandRole::Action]);
        assert_eq!(c.total_bytes(), a.total_bytes() + 7);
    }

    #[test]
    fn corbeille_indisponible_refusee_jamais_detruite() {
        use crate::bin::{DriveKind, RecycleBlock, VolumeFacts};
        let mut probe = MemoryProbe::new();
        for d in ["E:\\Clé", "Z:\\Partage"] {
            probe.dir(d);
        }
        probe.file("E:\\Clé\\a.bin", 10);
        probe.file("Z:\\Partage\\b.bin", 10);
        let fixed = VolumeFacts { kind: DriveKind::Fixed, subst: false, recycle_disabled: false, max_bytes: None };
        probe.volume('E', VolumeFacts { kind: DriveKind::Removable, ..fixed });
        probe.volume('Z', VolumeFacts { subst: true, ..fixed });
        let mut allow = Allowlist::new();
        for root in ["E:\\Clé", "Z:\\Partage"] {
            allow.grant(RuleGrant { rule_id: "cache".into(), root: WinPath::parse(root).unwrap(), scope: Scope::Contents, min_age_secs: None });
        }
        let deny = DenyList::new(&test_layout());
        let checker = Checker { probe: &probe, deny: &deny, allow: &allow, now: i64::MAX };
        let targets: Vec<Target> = ["E:\\Clé\\a.bin", "Z:\\Partage\\b.bin"].iter().map(|r| target(&probe, r)).collect();

        let trash = Plan::build(&checker, &targets, DeleteMode::Trash);
        assert!(trash.items().is_empty());
        let reasons: Vec<&Refusal> = trash.refused().iter().map(|(_, r)| r).collect();
        assert_eq!(
            reasons,
            [&Refusal::NoRecycleBin(RecycleBlock::NotFixed(DriveKind::Removable)), &Refusal::NoRecycleBin(RecycleBlock::Subst)]
        );
        // La suppression définitive, elle, est un choix explicite : pas de pré-contrôle de Corbeille.
        assert_eq!(Plan::build(&checker, &targets, DeleteMode::Permanent).items().len(), 2);
    }

    #[test]
    fn definitif_exige_une_seconde_confirmation() {
        let plan = plan_of(&["C:\\Users\\Arthur\\cache\\a.bin"], DeleteMode::Permanent);
        let once = Confirmation { fingerprint: plan.fingerprint(), permanent_acknowledged: false, without_restore_point: false };
        assert_eq!(verify(&plan, &once), Err(ExecError::PermanentNotAcknowledged));
    }
}
