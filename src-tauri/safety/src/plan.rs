//! Plan d'action : la liste exacte de ce qui sera fait, validée élément par élément, avec une
//! empreinte BLAKE3. Une exécution n'est acceptée qu'avec la confirmation de cette empreinte.
//!
//! Jalon 2 : seule la simulation est disponible. `execute` refuse toute exécution réelle
//! (`REAL_EXECUTION_ENABLED = false`) ; elle sera branchée au jalon 5.

use crate::check::{Checker, Refusal, SkipReason, Target, Validated, Verdict};
use crate::path::WinPath;
use crate::probe::Probe;

/// Interrupteur unique de l'exécution réelle. Reste à `false` tant que le jalon 5 n'est pas validé.
pub const REAL_EXECUTION_ENABLED: bool = false;

// Garde-fou de compilation : passer l'interrupteur à `true` exige de retirer cette ligne (jalon 5).
const _: () = assert!(!REAL_EXECUTION_ENABLED, "exécution réelle interdite avant le jalon 5");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeleteMode {
    /// Corbeille (par défaut), annulable.
    Trash,
    /// Suppression définitive : option explicite, double confirmation.
    Permanent,
}

#[derive(Debug, Clone)]
pub struct Plan {
    mode: DeleteMode,
    items: Vec<Validated>,
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
        let fingerprint = fingerprint(mode, &items);
        Plan { mode, items, skipped, refused, fingerprint }
    }

    pub fn mode(&self) -> DeleteMode {
        self.mode
    }

    pub fn items(&self) -> &[Validated] {
        &self.items
    }

    pub fn skipped(&self) -> &[(WinPath, SkipReason)] {
        &self.skipped
    }

    pub fn refused(&self) -> &[(String, Refusal)] {
        &self.refused
    }

    pub fn total_bytes(&self) -> u64 {
        self.items.iter().map(|i| i.bytes).sum()
    }

    pub fn fingerprint(&self) -> [u8; 32] {
        self.fingerprint
    }

    pub fn fingerprint_hex(&self) -> String {
        self.fingerprint.iter().map(|b| format!("{b:02x}")).collect()
    }
}

fn fingerprint(mode: DeleteMode, items: &[Validated]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"lumen-plan-v1");
    h.update(&[match mode {
        DeleteMode::Trash => 0,
        DeleteMode::Permanent => 1,
    }]);
    for i in items {
        let path = i.path.verbatim();
        h.update(&(path.len() as u64).to_le_bytes());
        h.update(path.as_bytes());
        h.update(&i.facts.file_id.volume_serial.to_le_bytes());
        h.update(&i.facts.file_id.id.to_le_bytes());
        h.update(&i.facts.size.to_le_bytes());
        h.update(&i.facts.modified.to_le_bytes());
        h.update(&i.bytes.to_le_bytes());
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
    let mut lines: Vec<String> = plan.items.iter().map(|i| format!("{} — {verb}", i.path)).collect();
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ExecError {
    #[error("le plan a changé depuis sa confirmation : rien n'a été fait")]
    FingerprintMismatch,
    #[error("la suppression définitive demande une seconde confirmation")]
    PermanentNotAcknowledged,
    #[error("rien à faire")]
    EmptyPlan,
    #[error("l'exécution réelle n'est pas encore activée : seule la simulation est disponible")]
    RealExecutionDisabled,
}

/// Point d'entrée unique de l'exécution réelle. Au jalon 2, il vérifie la confirmation puis refuse.
pub fn execute(plan: &Plan, confirmation: &Confirmation) -> Result<(), ExecError> {
    if confirmation.fingerprint != plan.fingerprint {
        return Err(ExecError::FingerprintMismatch);
    }
    if plan.mode == DeleteMode::Permanent && !confirmation.permanent_acknowledged {
        return Err(ExecError::PermanentNotAcknowledged);
    }
    if plan.items.is_empty() {
        return Err(ExecError::EmptyPlan);
    }
    // REAL_EXECUTION_ENABLED est une constante vérifiée à la compilation : ni le front ni une
    // configuration ne peuvent activer l'exécution réelle. Le jalon 5 branchera safety::win ici.
    Err(ExecError::RealExecutionDisabled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allow::{Allowlist, Authority, RuleGrant, Scope};
    use crate::check::Expected;
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
        let facts = probe.facts(&WinPath::parse(raw).unwrap()).unwrap();
        Target { path: raw.into(), authority: Authority::Rule("cache".into()), expected: Expected::from_facts(&facts), bytes: facts.size }
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
    fn execution_reelle_desactivee_au_jalon_2() {
        let plan = plan_of(&["C:\\Users\\Arthur\\cache\\a.bin"], DeleteMode::Trash);
        let ok = Confirmation { fingerprint: plan.fingerprint(), permanent_acknowledged: false };
        assert_eq!(execute(&plan, &ok), Err(ExecError::RealExecutionDisabled));
        let wrong = Confirmation { fingerprint: [0; 32], permanent_acknowledged: true };
        assert_eq!(execute(&plan, &wrong), Err(ExecError::FingerprintMismatch));
    }

    #[test]
    fn definitif_exige_une_seconde_confirmation() {
        let plan = plan_of(&["C:\\Users\\Arthur\\cache\\a.bin"], DeleteMode::Permanent);
        let once = Confirmation { fingerprint: plan.fingerprint(), permanent_acknowledged: false };
        assert_eq!(execute(&plan, &once), Err(ExecError::PermanentNotAcknowledged));
    }
}
