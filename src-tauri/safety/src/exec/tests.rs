//! Tests portables de l'exécution : sonde et effets en mémoire. Les mêmes scénarios tournent sur
//! de vraies fixtures dans `win/tests.rs` et dans les tests de bout en bout de la CI Windows.

use std::cell::{Cell, RefCell};
use std::sync::atomic::AtomicBool;

use super::*;
use crate::allow::{Allowlist, Authority, RuleGrant, Scope};
use crate::check::Checker;
use crate::commands::OfficialCommand;
use crate::denylist::{test_layout, DenyList, DenyReason};
use crate::plan::{execute, CommandRole, Confirmation, DeleteMode, PlannedCommand};
use crate::probe::memory::MemoryProbe;

const HOUR: i64 = 3600 * TICKS_PER_SECOND;
/// Les fichiers de la sonde datent de 133e15 ; « maintenant » est 100 h plus tard.
const NOW: i64 = 133_000_000_000_000_000 + 100 * HOUR;
const CACHE: &str = "C:\\Users\\Arthur\\cache";
const TEMP: &str = "C:\\Users\\Arthur\\AppData\\Local\\Temp";

struct World {
    probe: MemoryProbe,
    deny: DenyList,
    allow: Allowlist,
}

impl World {
    fn new() -> World {
        let probe = MemoryProbe::new();
        for d in ["C:\\Users", "C:\\Users\\Arthur", CACHE, "C:\\Users\\Arthur\\AppData", "C:\\Users\\Arthur\\AppData\\Local", TEMP, "C:\\Windows"] {
            probe.dir(d);
        }
        probe.file("C:\\Windows\\explorer.exe", 1000);
        let mut allow = Allowlist::new();
        allow.grant(RuleGrant { rule_id: "cache".into(), root: WinPath::parse(CACHE).unwrap(), scope: Scope::Contents, min_age_secs: None });
        allow.grant(RuleGrant { rule_id: "temp".into(), root: WinPath::parse(TEMP).unwrap(), scope: Scope::Contents, min_age_secs: Some(48 * 3600) });
        World { probe, deny: DenyList::new(&test_layout()), allow }
    }

    fn checker(&self) -> Checker<'_, MemoryProbe> {
        Checker { probe: &self.probe, deny: &self.deny, allow: &self.allow, now: NOW }
    }

    fn plan(&self, raws: &[&str], rule: &str, mode: DeleteMode) -> Plan {
        let targets: Vec<Target> = raws.iter().map(|r| self.probe.target(r, Authority::Rule(rule.into()))).collect();
        Plan::build(&self.checker(), &targets, mode)
    }

    fn run(&self, plan: &Plan, effects: &Fake<'_>) -> Report {
        let ok = Confirmation { fingerprint: plan.fingerprint(), permanent_acknowledged: true };
        execute(plan, &ok, &self.checker(), effects, &AtomicBool::new(false), |_| {}).unwrap()
    }
}

/// Effets en mémoire : la Corbeille retire de la sonde et garde la trace de ce qui est parti.
struct Fake<'a> {
    probe: &'a MemoryProbe,
    recycled: RefCell<Vec<String>>,
    deleted: RefCell<Vec<(String, u64)>>,
    free: Cell<u64>,
    fail: Option<&'static str>,
    commands: RefCell<Vec<OfficialCommand>>,
}

impl<'a> Fake<'a> {
    fn new(probe: &'a MemoryProbe) -> Fake<'a> {
        Fake { probe, recycled: RefCell::default(), deleted: RefCell::default(), free: Cell::new(1_000_000), fail: None, commands: RefCell::default() }
    }

    fn take(&self, unit: &Unit) -> Result<(), RemoveError> {
        if self.fail.is_some_and(|f| unit.path.display_string().ends_with(f)) {
            return Err(RemoveError::Failed("accès refusé".into()));
        }
        self.probe.remove(&unit.path);
        self.free.set(self.free.get() + unit.bytes);
        Ok(())
    }
}

impl Effects for Fake<'_> {
    fn recycle(&self, units: &[Unit]) -> Vec<Result<Recycled, RemoveError>> {
        units
            .iter()
            .map(|u| {
                self.take(u)?;
                self.recycled.borrow_mut().push(u.path.display_string());
                Ok(Recycled { bin_path: Some(format!("C:\\$Recycle.Bin\\S-1\\$R{}", self.recycled.borrow().len())) })
            })
            .collect()
    }

    fn delete(&self, unit: &Unit) -> Result<(), RemoveError> {
        self.take(unit)?;
        self.deleted.borrow_mut().push((unit.path.display_string(), unit.children.iter().map(ExpectedNode::count).sum()));
        Ok(())
    }

    fn free_bytes(&self, _: &WinPath) -> Option<u64> {
        Some(self.free.get())
    }

    fn run_command(&self, command: &OfficialCommand, _: &AtomicBool) -> CommandRun {
        self.commands.borrow_mut().push(command.clone());
        self.free.set(self.free.get() + 500);
        CommandRun { program: Some("C:\\Program Files\\nodejs\\node.exe".into()), ..CommandRun::new(command, CommandStatus::Succeeded) }
    }
}

fn p(s: &str) -> WinPath {
    WinPath::parse(s).unwrap()
}

/// Cache avec un dossier `sous` (2 fichiers) et un fichier à la racine.
fn cache(w: &World) {
    w.probe.dir("C:\\Users\\Arthur\\cache\\sous");
    w.probe.file("C:\\Users\\Arthur\\cache\\sous\\a.bin", 100);
    w.probe.file("C:\\Users\\Arthur\\cache\\sous\\b.bin", 50);
    w.probe.file("C:\\Users\\Arthur\\cache\\index.json", 10);
}

#[test]
fn dossier_intact_part_d_un_bloc() {
    let w = World::new();
    cache(&w);
    let plan = w.plan(&["C:\\Users\\Arthur\\cache\\sous", "C:\\Users\\Arthur\\cache\\index.json"], "cache", DeleteMode::Trash);
    let fx = Fake::new(&w.probe);
    let r = w.run(&plan, &fx);
    assert_eq!(*fx.recycled.borrow(), ["C:\\Users\\Arthur\\cache\\sous", "C:\\Users\\Arthur\\cache\\index.json"]);
    assert!(r.items.iter().all(|i| i.outcome == ItemOutcome::Done), "{r:#?}");
    assert_eq!(r.freed_estimate(), 160);
    assert_eq!(r.measured_gain(), Some(160));
    assert!(r.items[0].units[0].result.as_ref().unwrap().bin_path.is_some());
    assert!(w.probe.exists(CACHE));
}

#[test]
fn fichier_modifie_apres_le_plan_saute_jamais_supprime() {
    let w = World::new();
    cache(&w);
    let plan = w.plan(&["C:\\Users\\Arthur\\cache\\index.json"], "cache", DeleteMode::Trash);
    w.probe.update(&p("C:\\Users\\Arthur\\cache\\index.json"), |e| e.facts.size += 1);
    let fx = Fake::new(&w.probe);
    let r = w.run(&plan, &fx);
    assert!(matches!(r.items[0].outcome, ItemOutcome::Skipped(_)), "{r:#?}");
    assert!(fx.recycled.borrow().is_empty());
    assert!(w.probe.exists("C:\\Users\\Arthur\\cache\\index.json"));
}

#[test]
fn contenu_modifie_seul_l_intact_part() {
    let w = World::new();
    cache(&w);
    let plan = w.plan(&["C:\\Users\\Arthur\\cache\\sous"], "cache", DeleteMode::Trash);
    w.probe.update(&p("C:\\Users\\Arthur\\cache\\sous\\b.bin"), |e| e.facts.modified += 1);
    let fx = Fake::new(&w.probe);
    let r = w.run(&plan, &fx);
    let item = &r.items[0];
    assert_eq!(item.outcome, ItemOutcome::Partial);
    assert_eq!(*fx.recycled.borrow(), ["C:\\Users\\Arthur\\cache\\sous\\a.bin"]);
    assert_eq!(item.issues, [Issue { path: "C:\\Users\\Arthur\\cache\\sous\\b.bin".into(), reason: IssueReason::Changed(Change::Modified) }]);
    assert!(w.probe.exists("C:\\Users\\Arthur\\cache\\sous\\b.bin"));
    assert!(w.probe.exists("C:\\Users\\Arthur\\cache\\sous"), "le dossier n'est jamais traité d'un bloc s'il a changé");
}

#[test]
fn element_apparu_depuis_l_analyse_laisse() {
    let w = World::new();
    cache(&w);
    let plan = w.plan(&["C:\\Users\\Arthur\\cache\\sous"], "cache", DeleteMode::Trash);
    w.probe.file("C:\\Users\\Arthur\\cache\\sous\\nouveau.txt", 3);
    let fx = Fake::new(&w.probe);
    let r = w.run(&plan, &fx);
    assert_eq!(r.items[0].outcome, ItemOutcome::Partial);
    assert_eq!(r.items[0].issues[0].reason, IssueReason::New);
    assert!(w.probe.exists("C:\\Users\\Arthur\\cache\\sous\\nouveau.txt"));
    assert!(!w.probe.exists("C:\\Users\\Arthur\\cache\\sous\\a.bin"));
}

#[test]
fn jonction_apparue_dans_un_dossier_jamais_suivie() {
    let w = World::new();
    cache(&w);
    w.probe.dir("C:\\Users\\Arthur\\cache\\sous\\lib");
    w.probe.file("C:\\Users\\Arthur\\cache\\sous\\lib\\x.js", 1);
    let plan = w.plan(&["C:\\Users\\Arthur\\cache\\sous"], "cache", DeleteMode::Permanent);
    // Entre le plan et l'action, `lib` devient une jonction vers Windows.
    w.probe.remove(&p("C:\\Users\\Arthur\\cache\\sous\\lib"));
    w.probe.junction("C:\\Users\\Arthur\\cache\\sous\\lib", "C:\\Windows");
    let fx = Fake::new(&w.probe);
    let r = w.run(&plan, &fx);
    assert_eq!(r.items[0].outcome, ItemOutcome::Partial);
    assert_eq!(r.items[0].issues[0].reason, IssueReason::ReparsePoint);
    assert!(w.probe.exists("C:\\Windows\\explorer.exe"));
    assert!(w.probe.exists("C:\\Users\\Arthur\\cache\\sous\\lib"));
    let deleted: Vec<String> = fx.deleted.borrow().iter().map(|(p, _)| p.clone()).collect();
    assert_eq!(deleted, ["C:\\Users\\Arthur\\cache\\sous\\a.bin", "C:\\Users\\Arthur\\cache\\sous\\b.bin"]);
}

#[test]
fn secret_dans_un_cache_jamais_supprime() {
    let w = World::new();
    cache(&w);
    w.probe.file("C:\\Users\\Arthur\\cache\\sous\\id_rsa", 1);
    let plan = w.plan(&["C:\\Users\\Arthur\\cache\\sous"], "cache", DeleteMode::Trash);
    let fx = Fake::new(&w.probe);
    let r = w.run(&plan, &fx);
    assert_eq!(r.items[0].issues[0].reason, IssueReason::Denied(DenyReason::Secret));
    assert!(w.probe.exists("C:\\Users\\Arthur\\cache\\sous\\id_rsa"));
}

#[test]
fn temp_seuls_les_anciens_partent() {
    let w = World::new();
    w.probe.dir("C:\\Users\\Arthur\\AppData\\Local\\Temp\\setup");
    w.probe.file("C:\\Users\\Arthur\\AppData\\Local\\Temp\\setup\\vieux.log", 10);
    let recent = w.probe.file("C:\\Users\\Arthur\\AppData\\Local\\Temp\\setup\\recent.log", 10);
    let setup = p("C:\\Users\\Arthur\\AppData\\Local\\Temp\\setup");
    let plan = w.plan(&["C:\\Users\\Arthur\\AppData\\Local\\Temp\\setup"], "temp", DeleteMode::Trash);
    assert_eq!(plan.items().len(), 1);
    // Une appli réécrit un fichier du dossier : il redevient récent (même identité, même taille).
    w.probe.update(&recent, |e| e.facts.modified = NOW - HOUR);
    // Le plan a figé l'ancienne date : le changement est détecté avant même l'âge minimum.
    let fx = Fake::new(&w.probe);
    let r = w.run(&plan, &fx);
    assert!(w.probe.exists("C:\\Users\\Arthur\\AppData\\Local\\Temp\\setup\\recent.log"));
    assert!(!w.probe.exists("C:\\Users\\Arthur\\AppData\\Local\\Temp\\setup\\vieux.log"));
    assert!(w.probe.exists("C:\\Users\\Arthur\\AppData\\Local\\Temp\\setup"));
    assert_eq!(r.items[0].outcome, ItemOutcome::Partial);
    let _ = setup;

    // Un fichier déjà récent au moment du plan : refusé dès le plan, mais aussi au recontrôle.
    let w = World::new();
    w.probe.dir("C:\\Users\\Arthur\\AppData\\Local\\Temp\\d");
    let young = w.probe.file("C:\\Users\\Arthur\\AppData\\Local\\Temp\\d\\jeune.tmp", 1);
    w.probe.update(&young, |e| e.facts.modified = NOW - HOUR);
    w.probe.update(&p("C:\\Users\\Arthur\\AppData\\Local\\Temp\\d"), |e| e.facts.modified -= HOUR);
    let plan = w.plan(&["C:\\Users\\Arthur\\AppData\\Local\\Temp\\d"], "temp", DeleteMode::Trash);
    let fx = Fake::new(&w.probe);
    let r = w.run(&plan, &fx);
    assert_eq!(r.items[0].issues[0].reason, IssueReason::TooRecent);
    assert!(w.probe.exists("C:\\Users\\Arthur\\AppData\\Local\\Temp\\d\\jeune.tmp"));
}

#[test]
fn verrouille_laisse_et_signale() {
    let w = World::new();
    cache(&w);
    let plan = w.plan(&["C:\\Users\\Arthur\\cache\\sous"], "cache", DeleteMode::Trash);
    w.probe.update(&p("C:\\Users\\Arthur\\cache\\sous\\a.bin"), |e| e.locked = true);
    let fx = Fake::new(&w.probe);
    let r = w.run(&plan, &fx);
    assert_eq!(r.items[0].issues[0].reason, IssueReason::Locked);
    assert_eq!(*fx.recycled.borrow(), ["C:\\Users\\Arthur\\cache\\sous\\b.bin"]);
}

#[test]
fn echec_partiel_on_continue_puis_bilan() {
    let w = World::new();
    cache(&w);
    let plan = w.plan(&["C:\\Users\\Arthur\\cache\\index.json", "C:\\Users\\Arthur\\cache\\sous"], "cache", DeleteMode::Trash);
    let mut fx = Fake::new(&w.probe);
    fx.fail = Some("index.json");
    let r = w.run(&plan, &fx);
    assert_eq!(r.items[0].outcome, ItemOutcome::Failed("accès refusé".into()));
    assert_eq!(r.items[1].outcome, ItemOutcome::Done);
    assert_eq!(r.freed_estimate(), 150);
}

#[test]
fn annulation_entre_deux_elements() {
    let w = World::new();
    cache(&w);
    let plan = w.plan(&["C:\\Users\\Arthur\\cache\\index.json", "C:\\Users\\Arthur\\cache\\sous"], "cache", DeleteMode::Trash);
    let fx = Fake::new(&w.probe);
    let cancel = AtomicBool::new(false);
    let ok = Confirmation { fingerprint: plan.fingerprint(), permanent_acknowledged: false };
    let r = execute(&plan, &ok, &w.checker(), &fx, &cancel, |e| {
        if matches!(e, Event::ItemDone { index: 0, .. }) {
            cancel.store(true, Ordering::SeqCst);
        }
    })
    .unwrap();
    assert!(r.cancelled);
    assert_eq!(r.items[0].outcome, ItemOutcome::Done);
    assert_eq!(r.items[1].outcome, ItemOutcome::Cancelled);
    assert!(w.probe.exists("C:\\Users\\Arthur\\cache\\sous\\a.bin"));
}

#[test]
fn commandes_d_abord_jamais_elevees() {
    let w = World::new();
    cache(&w);
    let plan = w.plan(&["C:\\Users\\Arthur\\cache\\sous"], "cache", DeleteMode::Trash).with_commands(vec![
        PlannedCommand { command: OfficialCommand::NpmCacheClean, role: CommandRole::Action, measured: Some(p(CACHE)), bytes: 500 },
        PlannedCommand { command: OfficialCommand::GradleStop, role: CommandRole::Prerequisite, measured: None, bytes: 0 },
        PlannedCommand { command: OfficialCommand::DismComponentCleanup, role: CommandRole::Action, measured: None, bytes: 0 },
    ]);
    let fx = Fake::new(&w.probe);
    let mut order = Vec::new();
    let ok = Confirmation { fingerprint: plan.fingerprint(), permanent_acknowledged: false };
    let r = execute(&plan, &ok, &w.checker(), &fx, &AtomicBool::new(false), |e| order.push(e)).unwrap();
    assert_eq!(*fx.commands.borrow(), [OfficialCommand::GradleStop, OfficialCommand::NpmCacheClean]);
    assert_eq!(r.commands[2].status, CommandStatus::NeedsElevation);
    assert_eq!(r.commands[0].role, CommandRole::Prerequisite);
    assert!(matches!(order[0], Event::Command { index: 0, .. }));
    assert!(order.iter().position(|e| matches!(e, Event::Item { .. })) > order.iter().position(|e| matches!(e, Event::CommandDone { index: 2, .. })));
    // Gain réel : espace libre avant / après, commandes comprises.
    assert_eq!(r.measured_gain(), Some(150 + 1000));
}

#[test]
fn definitif_transmet_le_contenu_verifie() {
    let w = World::new();
    cache(&w);
    let plan = w.plan(&["C:\\Users\\Arthur\\cache\\sous"], "cache", DeleteMode::Permanent);
    let fx = Fake::new(&w.probe);
    let r = w.run(&plan, &fx);
    assert_eq!(*fx.deleted.borrow(), [("C:\\Users\\Arthur\\cache\\sous".to_owned(), 2)]);
    assert!(fx.recycled.borrow().is_empty());
    assert_eq!(r.items[0].outcome, ItemOutcome::Done);
}

#[test]
fn element_disparu_ou_remplace() {
    let w = World::new();
    cache(&w);
    let plan = w.plan(&["C:\\Users\\Arthur\\cache\\index.json", "C:\\Users\\Arthur\\cache\\sous"], "cache", DeleteMode::Trash);
    w.probe.remove(&p("C:\\Users\\Arthur\\cache\\index.json"));
    w.probe.replace(&p("C:\\Users\\Arthur\\cache\\sous"));
    let fx = Fake::new(&w.probe);
    let r = w.run(&plan, &fx);
    assert!(matches!(r.items[0].outcome, ItemOutcome::Skipped(_)));
    assert!(matches!(r.items[1].outcome, ItemOutcome::Skipped(_)), "{:?}", r.items[1]);
    assert!(fx.recycled.borrow().is_empty());
}
