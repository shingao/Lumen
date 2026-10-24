use super::memory::{MemFs, Node, Op};
use super::*;
use crate::denylist::test_layout;
use std::cell::RefCell;

const SRC: &str = "C:\\Users\\Arthur\\Videos\\Rushes";
const DST: &str = "D:\\Lumen\\Déplacés\\C\\Users\\Arthur\\Videos\\Rushes";

fn p(raw: &str) -> WinPath {
    WinPath::parse(raw).unwrap()
}

struct World {
    fs: MemFs,
    deny: DenyList,
    games: Vec<WinPath>,
    synced: Vec<WinPath>,
    app_data: Vec<WinPath>,
}

impl World {
    fn new() -> World {
        let fs = MemFs::new();
        fs.dir(SRC);
        fs.file(&format!("{SRC}\\a.mp4"), &[1; 4000]);
        fs.file(&format!("{SRC}\\b.mp4"), &[2; 3000]);
        fs.dir(&format!("{SRC}\\jour 2"));
        fs.file(&format!("{SRC}\\jour 2\\c.mov"), &[3; 2000]);
        fs.file(&format!("{SRC}\\jour 2\\vide.txt"), &[]);
        fs.dir(&format!("{SRC}\\jour 2\\profond"));
        fs.file(&format!("{SRC}\\jour 2\\profond\\d.wav"), &[4; 1000]);
        World {
            fs,
            deny: DenyList::new(&test_layout()),
            games: vec![p("D:\\SteamLibrary\\steamapps\\common\\Celeste"), p("C:\\Users\\Arthur\\Jeux\\Hades")],
            synced: vec![p("C:\\Users\\Arthur\\OneDrive")],
            app_data: vec![p("C:\\Users\\Arthur\\AppData\\Local"), p("C:\\Users\\Arthur\\AppData\\LocalLow"), p("C:\\Users\\Arthur\\AppData\\Roaming")],
        }
    }

    fn ctx(&self) -> MoveContext<'_> {
        MoveContext { deny: &self.deny, game_dirs: &self.games, synced_roots: &self.synced, app_data: &self.app_data }
    }

    fn prepare(&self, raw: &str, to: char) -> Result<Prepared, Vec<Refusal>> {
        prepare(&p(raw), to, &self.ctx(), &self.fs)
    }

    fn run(&self, plan: &Prepared) -> Result<Staged, Failure> {
        run(plan, &plan.fingerprint(), &self.ctx(), &self.fs, &self.fs, &|_, _, _| {})
    }

    fn refusals(&self, raw: &str, to: char) -> Vec<Refusal> {
        self.prepare(raw, to).err().unwrap_or_default()
    }
}

fn original() -> Vec<(String, Node)> {
    World::new().fs.snapshot(SRC)
}

/// Rien n'a bougé : le dossier d'origine intact (contenu et métadonnées), ni copie, ni jonction,
/// ni original mis de côté, et l'espace du disque cible rendu.
fn assert_untouched(w: &World, d_free: u64) {
    assert!(matches!(w.fs.node(SRC), Some(Node::Dir { meta: 7 })), "{:?}", w.fs.node(SRC));
    assert_eq!(w.fs.snapshot(SRC), original());
    assert_eq!(w.fs.node(DST), None, "la copie est retirée");
    assert_eq!(w.fs.node(&format!("{SRC}{STAGING_SUFFIX}")), None);
    assert_eq!(w.fs.free('D'), d_free);
}

#[test]
fn chemin_cible_et_marge() {
    assert_eq!(target_for(&p(SRC), 'd'), Some(p(DST)));
    assert_eq!(target_for(&p("E:\\Données\\X"), 'D'), Some(p("D:\\Lumen\\Déplacés\\E\\Données\\X")));
    assert_eq!(needed_space(1000), 1100);
    assert_eq!(needed_space(1001), 1102, "marge arrondie vers le haut");
    assert_eq!(needed_space(0), 0);
    assert_eq!(staging_of(&p(SRC)), Some(p("C:\\Users\\Arthur\\Videos\\Rushes.lumen-deplacement")));
}

#[test]
fn deplacement_complet_puis_suppression_de_l_original() {
    let w = World::new();
    let plan = w.prepare(SRC, 'D').unwrap();
    assert_eq!((plan.files, plan.bytes, plan.review), (5, 10_000, false));
    assert_eq!(plan.target, p(DST));
    let steps = RefCell::new(Vec::new());
    let staged = run(&plan, &plan.fingerprint(), &w.ctx(), &w.fs, &w.fs, &|s, _, _| {
        let mut v = steps.borrow_mut();
        if v.last() != Some(&s) {
            v.push(s);
        }
    })
    .unwrap();
    assert_eq!(*steps.borrow(), [Step::Recheck, Step::Copy, Step::Verify, Step::SetAside, Step::Junction, Step::Test]);
    // Avant la confirmation : la jonction est en place, l'original est mis de côté, intact.
    assert_eq!(w.fs.junction_target(&p(SRC)), Some(p(DST)));
    assert_eq!(w.fs.snapshot(&format!("{SRC}{STAGING_SUFFIX}")), original());
    // Copie fidèle : mêmes contenus, mêmes métadonnées (dossiers compris).
    assert_eq!(w.fs.snapshot(DST), original());
    assert!(matches!(w.fs.node(DST), Some(Node::Dir { meta: 7 })));
    // À travers la jonction, les applis voient le même contenu au même chemin.
    assert_eq!(w.fs.snapshot(SRC), original());
    assert_eq!(finish(&staged, true, &w.fs, &w.fs), Finished::Done);
    assert_eq!(w.fs.node(&format!("{SRC}{STAGING_SUFFIX}")), None, "original supprimé définitivement");
    assert_eq!(w.fs.snapshot(SRC), original());
    assert_eq!(health(&p(SRC), &p(DST), &w.fs), Health::Ok);
    assert_eq!(w.fs.free('D'), (1 << 40) - 10_000);
    // Ordre des effets : jamais de suppression avant la jonction testée.
    let log = w.fs.log.borrow();
    let junction = log.iter().position(|l| l.starts_with("junction")).unwrap();
    let delete = log.iter().position(|l| l.starts_with("delete")).unwrap();
    assert!(junction < delete);
}

#[test]
fn chaque_refus() {
    let w = World::new();
    let only = |raw: &str, to: char| w.refusals(raw, to);
    assert_eq!(only(SRC, 'C'), [Refusal::SameVolume]);
    assert_eq!(only(SRC, 'Z'), [Refusal::TargetUnavailable]);
    assert_eq!(only(&format!("{SRC}\\a.mp4"), 'D'), [Refusal::NotADirectory]);
    assert_eq!(only("C:\\Users\\Arthur\\Absent", 'D'), [Refusal::NotADirectory]);

    let fixed = VolumeInfo { kind: DriveKind::Fixed, subst: false, ntfs: true, free: 1 << 40 };
    w.fs.volumes.borrow_mut().insert('E', VolumeInfo { kind: DriveKind::Removable, ..fixed });
    w.fs.volumes.borrow_mut().insert('F', VolumeInfo { kind: DriveKind::Remote, ..fixed });
    w.fs.volumes.borrow_mut().insert('S', VolumeInfo { subst: true, ..fixed });
    w.fs.volumes.borrow_mut().insert('X', VolumeInfo { ntfs: false, ..fixed });
    assert_eq!(only(SRC, 'E'), [Refusal::TargetNotFixed]);
    assert_eq!(only(SRC, 'F'), [Refusal::TargetNotFixed], "jamais un lecteur réseau");
    assert_eq!(only(SRC, 'S'), [Refusal::TargetSubst]);
    assert_eq!(only(SRC, 'X'), [Refusal::TargetNotNtfs]);

    // Denylist : le dossier, un parent d'une zone protégée, ou un élément du dossier.
    w.fs.dir("C:\\Windows\\Temp");
    w.fs.file("C:\\Windows\\Temp\\x.tmp", b"x");
    assert_eq!(only("C:\\Windows\\Temp", 'D'), [Refusal::Denied(DenyReason::Windows)]);
    w.fs.file("C:\\Users\\Arthur\\x.txt", b"x");
    assert!(only("C:\\Users\\Arthur", 'D').contains(&Refusal::Denied(DenyReason::ProfileRoot)));
    w.fs.dir("C:\\Users\\Arthur\\Jeu indé\\Saves");
    w.fs.file("C:\\Users\\Arthur\\Jeu indé\\Saves\\slot1.dat", b"x");
    w.fs.file("C:\\Users\\Arthur\\Jeu indé\\data.pak", b"x");
    assert_eq!(only("C:\\Users\\Arthur\\Jeu indé", 'D'), [Refusal::Denied(DenyReason::GameSave)]);

    // Dossier de jeu, et dossier qui contient un jeu.
    w.fs.file("C:\\Users\\Arthur\\Jeux\\Hades\\Hades.exe", b"x");
    assert_eq!(only("C:\\Users\\Arthur\\Jeux\\Hades", 'D'), [Refusal::GameFolder]);
    assert_eq!(only("C:\\Users\\Arthur\\Jeux", 'D'), [Refusal::GameFolder]);

    // Disques virtuels de WSL / Docker.
    w.fs.file("C:\\Users\\Arthur\\AppData\\Local\\Docker\\wsl\\disk\\docker_data.vhdx", b"x");
    assert_eq!(only("C:\\Users\\Arthur\\AppData\\Local\\Docker", 'D'), [Refusal::VirtualDisk]);

    // OneDrive, racine synchronisée ou fichiers cloud.
    w.fs.file("C:\\Users\\Arthur\\OneDrive\\Photos\\p.jpg", b"x");
    assert_eq!(only("C:\\Users\\Arthur\\OneDrive\\Photos", 'D'), [Refusal::Synced]);
    w.fs.dir("C:\\Données\\Drive");
    w.fs.cloud_file("C:\\Données\\Drive\\doc.pdf");
    assert_eq!(only("C:\\Données\\Drive", 'D'), [Refusal::Synced]);

    // Liens : dans le dossier, sur un parent, ou le dossier lui-même (déjà déplacé).
    w.fs.dir("C:\\Données\\Liens");
    w.fs.file("C:\\Données\\Liens\\a.bin", b"x");
    w.fs.link("C:\\Données\\Liens\\ailleurs", "C:\\Données\\Drive");
    assert_eq!(only("C:\\Données\\Liens", 'D'), [Refusal::ReparsePoints]);
    w.fs.dir("E:\\Vrai");
    w.fs.file("E:\\Vrai\\Sous\\f.bin", b"x");
    w.fs.link("C:\\Données\\Raccourci", "E:\\Vrai");
    assert_eq!(only("C:\\Données\\Raccourci\\Sous", 'D'), [Refusal::ReparsePoints]);
    w.fs.dir("D:\\Lumen\\Déplacés\\C\\Données\\Fait");
    w.fs.link("C:\\Données\\Fait", "D:\\Lumen\\Déplacés\\C\\Données\\Fait");
    assert_eq!(only("C:\\Données\\Fait", 'D'), [Refusal::AlreadyMoved]);

    // Ouvert par un programme : on dit lequel.
    w.fs.hold(&format!("{SRC}\\jour 2\\c.mov"), "Discord");
    w.fs.hold(&format!("{SRC}\\a.mp4"), "Premiere Pro");
    w.fs.hold(&format!("{SRC}\\b.mp4"), "Discord");
    assert_eq!(only(SRC, 'D'), [Refusal::InUse(vec!["Premiere Pro".into(), "Discord".into()])]);
    assert_eq!(Refusal::InUse(vec!["Discord".into()]).to_string(), "ouvert par Discord");
    w.fs.holders.borrow_mut().clear();

    // Espace libre : la taille plus 10 %.
    w.fs.set_free('D', 10_999);
    assert_eq!(only(SRC, 'D'), [Refusal::NotEnoughSpace { needed: 11_000, free: 10_999 }]);
    w.fs.set_free('D', 11_000);
    assert!(w.prepare(SRC, 'D').is_ok());

    // Copie déjà là, déplacement précédent interrompu, dossier vide.
    w.fs.dir(DST);
    assert_eq!(only(SRC, 'D'), [Refusal::TargetExists]);
    w.fs.recycle(&p(DST)).unwrap();
    w.fs.dir(&format!("{SRC}{STAGING_SUFFIX}"));
    assert_eq!(only(SRC, 'D'), [Refusal::StagingExists]);
    w.fs.dir("C:\\Données\\Vide\\Sous");
    assert_eq!(only("C:\\Données\\Vide", 'D'), [Refusal::Empty]);

    // Plusieurs raisons : toutes annoncées.
    let mut r = only("C:\\Users\\Arthur\\Jeux\\Hades", 'E');
    r.sort_by_key(|x| x.to_string());
    assert_eq!(r.len(), 2, "{r:?}");
}

#[test]
fn source_hors_ntfs_refusee() {
    let w = World::new();
    w.fs.volumes.borrow_mut().get_mut(&'C').unwrap().ntfs = false;
    assert_eq!(w.refusals(SRC, 'D'), [Refusal::SourceNotNtfs]);
}

#[test]
fn dossier_d_appli_dans_appdata_a_verifier() {
    let w = World::new();
    w.fs.file("C:\\Users\\Arthur\\AppData\\Local\\JetBrains\\Toolbox\\apps\\idea.jar", &[9; 500]);
    let plan = w.prepare("C:\\Users\\Arthur\\AppData\\Local\\JetBrains", 'D').unwrap();
    assert!(plan.review, "l'appli peut ne plus se mettre à jour correctement");
    assert!(!w.prepare(SRC, 'D').unwrap().review);
}

#[test]
fn echec_a_chaque_etape_puis_retour_en_arriere() {
    let cases: &[(Op, u32, Step)] = &[
        (Op::CreateDir, 1, Step::Copy),
        (Op::CreateDir, 6, Step::Copy),
        (Op::CreateDir, 7, Step::Copy),
        (Op::CopyFile, 1, Step::Copy),
        (Op::CopyFile, 4, Step::Copy),
        (Op::CopyDirMeta, 1, Step::Copy),
        (Op::CopyDirMeta, 3, Step::Copy),
        (Op::CorruptCopy, 3, Step::Verify),
        (Op::Rename, 1, Step::SetAside),
        (Op::CreateJunction, 1, Step::Junction),
        (Op::WrongJunction, 1, Step::Test),
    ];
    for &(op, nth, step) in cases {
        let w = World::new();
        let free = w.fs.free('D');
        let plan = w.prepare(SRC, 'D').unwrap();
        w.fs.fail_on(op, nth);
        match w.run(&plan) {
            Err(Failure::Step { step: s, rolled_back: true, rollback_error: None, .. }) => assert_eq!(s, step, "{op:?} {nth}"),
            other => panic!("{op:?} {nth} : {other:?}"),
        }
        assert_untouched(&w, free);
    }
}

#[test]
fn test_de_lecture_a_travers_la_jonction() {
    // La copie change après la vérification (un autre programme écrit sur D:) : le test de
    // lecture à travers la jonction le voit, et tout revient en arrière.
    struct Sneaky<'a>(&'a MemFs);
    impl MoveProbe for Sneaky<'_> {
        fn volume(&self, d: char) -> Option<VolumeInfo> {
            self.0.volume(d)
        }
        fn kind(&self, p: &WinPath) -> Option<PathKind> {
            self.0.kind(p)
        }
        fn tree(&self, d: &WinPath) -> Result<Vec<TreeEntry>, ProbeError> {
            self.0.tree(d)
        }
        fn holders(&self, f: &[WinPath]) -> Vec<String> {
            self.0.holders(f)
        }
        fn hash(&self, f: &WinPath) -> Result<[u8; 32], ProbeError> {
            if self.0.junction_target(&WinPath::parse(SRC).unwrap()).is_some() && f.is_strictly_under(&WinPath::parse(SRC).unwrap()) {
                return Ok([0; 32]);
            }
            self.0.hash(f)
        }
        fn junction_target(&self, l: &WinPath) -> Option<WinPath> {
            self.0.junction_target(l)
        }
    }
    let w = World::new();
    let free = w.fs.free('D');
    let plan = w.prepare(SRC, 'D').unwrap();
    let r = run(&plan, &plan.fingerprint(), &w.ctx(), &Sneaky(&w.fs), &w.fs, &|_, _, _| {});
    assert!(matches!(r, Err(Failure::Step { step: Step::Test, rolled_back: true, .. })), "{r:?}");
    assert_untouched(&w, free);
}

#[test]
fn cible_pleine() {
    // Remplie entre la confirmation et l'action : le recontrôle refuse, rien n'est fait.
    let w = World::new();
    let plan = w.prepare(SRC, 'D').unwrap();
    w.fs.set_free('D', 5_000);
    assert!(matches!(w.run(&plan), Err(Failure::Refused(r)) if matches!(r[..], [Refusal::NotEnoughSpace { .. }])));
    assert_untouched(&w, 5_000);
    // Remplie pendant la copie (un autre programme écrit sur D:) : retour en arrière.
    let w = World::new();
    let plan = w.prepare(SRC, 'D').unwrap();
    let r = run(&plan, &plan.fingerprint(), &w.ctx(), &w.fs, &w.fs, &|step, done, _| {
        if step == Step::Copy && done > 0 {
            w.fs.set_free('D', 0);
        }
    });
    assert!(matches!(&r, Err(Failure::Step { step: Step::Copy, rolled_back: true, error, .. }) if error.contains("plein")), "{r:?}");
    assert_eq!(w.fs.node(DST), None);
    assert_eq!(w.fs.snapshot(SRC), original());
}

#[test]
fn recontrole_au_moment_d_agir() {
    let w = World::new();
    let plan = w.prepare(SRC, 'D').unwrap();
    assert_eq!(run(&plan, "autre", &w.ctx(), &w.fs, &w.fs, &|_, _, _| {}), Err(Failure::FingerprintMismatch));
    // Ouvert entre-temps : refusé, rien n'est fait.
    w.fs.hold(&format!("{SRC}\\a.mp4"), "Discord");
    assert_eq!(w.run(&plan), Err(Failure::Refused(vec![Refusal::InUse(vec!["Discord".into()])])));
    w.fs.holders.borrow_mut().clear();
    // Contenu changé : refusé.
    w.fs.file(&format!("{SRC}\\nouveau.mp4"), b"x");
    assert!(matches!(w.run(&plan), Err(Failure::Refused(_))));
    assert_eq!(w.fs.node(DST), None);
}

#[test]
fn confirmation_refusee_retour_en_arriere() {
    let w = World::new();
    let free = w.fs.free('D');
    let plan = w.prepare(SRC, 'D').unwrap();
    let staged = w.run(&plan).unwrap();
    assert_eq!(finish(&staged, false, &w.fs, &w.fs), Finished::RolledBack);
    assert_untouched(&w, free);
}

#[test]
fn retour_en_arriere_impossible_ne_supprime_rien() {
    let w = World::new();
    let plan = w.prepare(SRC, 'D').unwrap();
    // La jonction échoue au test, puis la remise en place de l'original échoue aussi.
    w.fs.fail_on(Op::WrongJunction, 1);
    w.fs.fail_on(Op::Rename, 2);
    match w.run(&plan) {
        Err(Failure::Step { step: Step::Test, rolled_back: false, rollback_error: Some(e), .. }) => assert!(e.contains("remise en place"), "{e}"),
        other => panic!("{other:?}"),
    }
    // Rien n'est perdu : l'original mis de côté et la copie sont tous deux là.
    assert_eq!(w.fs.snapshot(&format!("{SRC}{STAGING_SUFFIX}")), original());
    assert_eq!(w.fs.snapshot(DST), original());
    assert!(!w.fs.log.borrow().iter().any(|l| l.starts_with("delete")));
}

#[test]
fn suppression_finale_gardee() {
    let w = World::new();
    let plan = w.prepare(SRC, 'D').unwrap();
    let staged = w.run(&plan).unwrap();
    // La suppression échoue : la jonction et la copie restent, l'original aussi.
    w.fs.fail_on(Op::DeleteTree, 1);
    assert!(matches!(finish(&staged, true, &w.fs, &w.fs), Finished::DeleteFailed(_)));
    assert_eq!(w.fs.snapshot(&format!("{SRC}{STAGING_SUFFIX}")), original());
    assert_eq!(w.fs.snapshot(SRC), original());
    // La jonction a été remplacée entre-temps : rien n'est supprimé.
    w.fs.remove_junction(&p(SRC), Some(&p(DST))).unwrap();
    w.fs.dir(SRC);
    assert!(matches!(finish(&staged, true, &w.fs, &w.fs), Finished::DeleteFailed(_)));
    assert_eq!(w.fs.snapshot(&format!("{SRC}{STAGING_SUFFIX}")), original());
}

fn moved() -> World {
    let w = World::new();
    let plan = w.prepare(SRC, 'D').unwrap();
    let staged = w.run(&plan).unwrap();
    assert_eq!(finish(&staged, true, &w.fs, &w.fs), Finished::Done);
    w
}

#[test]
fn annuler_le_deplacement() {
    let w = moved();
    let r = undo(&p(SRC), &p(DST), &w.fs, &w.fs, &|_, _, _| {});
    assert_eq!(r, Undone::Done { recycle_error: None });
    assert!(matches!(w.fs.node(SRC), Some(Node::Dir { meta: 7 })), "un vrai dossier, métadonnées comprises");
    assert_eq!(w.fs.snapshot(SRC), original());
    assert_eq!(w.fs.node(DST), None);
    assert_eq!(*w.fs.recycled.borrow(), [p(DST)], "la copie de la cible part à la Corbeille");
    assert_eq!(w.fs.node(&format!("{SRC}{RETURN_SUFFIX}")), None);
    assert_eq!(undo(&p(SRC), &p(DST), &w.fs, &w.fs, &|_, _, _| {}), Undone::Refused("l'emplacement d'origine n'est plus la jonction créée par Lumen".into()));
}

#[test]
fn annulation_qui_echoue_ne_perd_rien() {
    for (op, nth) in [(Op::CreateDir, 1), (Op::CopyFile, 2), (Op::CorruptCopy, 1), (Op::RemoveJunction, 1), (Op::Rename, 1)] {
        let w = moved();
        w.fs.fail_on(op, nth);
        let r = undo(&p(SRC), &p(DST), &w.fs, &w.fs, &|_, _, _| {});
        assert!(matches!(r, Undone::Failed { .. }), "{op:?} {r:?}");
        // La jonction mène toujours à la copie, intacte ; la recopie partielle est retirée.
        assert_eq!(w.fs.junction_target(&p(SRC)), Some(p(DST)), "{op:?}");
        assert_eq!(w.fs.snapshot(DST), original());
        assert_eq!(w.fs.node(&format!("{SRC}{RETURN_SUFFIX}")), None, "{op:?}");
    }
    // Disque d'origine trop plein pour recopier, ou fichier ouvert : refusé avant toute copie.
    let w = moved();
    w.fs.set_free('C', 100);
    assert!(matches!(undo(&p(SRC), &p(DST), &w.fs, &w.fs, &|_, _, _| {}), Undone::Refused(_)));
    w.fs.set_free('C', 1 << 40);
    w.fs.hold(&format!("{DST}\\a.mp4"), "VLC");
    assert_eq!(undo(&p(SRC), &p(DST), &w.fs, &w.fs, &|_, _, _| {}), Undone::Refused("ouvert par VLC".into()));
}

#[test]
fn suivi_des_jonctions() {
    let w = moved();
    assert_eq!(health(&p(SRC), &p(DST), &w.fs), Health::Ok);
    let d = w.fs.volumes.borrow_mut().remove(&'D').unwrap();
    assert_eq!(health(&p(SRC), &p(DST), &w.fs), Health::TargetVolumeAbsent);
    w.fs.volumes.borrow_mut().insert('D', d);
    w.fs.recycle(&p(DST)).unwrap();
    assert_eq!(health(&p(SRC), &p(DST), &w.fs), Health::TargetMissing);
    let w = moved();
    w.fs.remove_junction(&p(SRC), Some(&p(DST))).unwrap();
    assert_eq!(health(&p(SRC), &p(DST), &w.fs), Health::JunctionMissing);
}

#[test]
fn empreinte_du_plan() {
    let w = World::new();
    let a = w.prepare(SRC, 'D').unwrap();
    w.fs.file(&format!("{SRC}\\jour 2\\vide.txt"), b"plus vide");
    let b = w.prepare(SRC, 'D').unwrap();
    assert_ne!(a.fingerprint(), b.fingerprint());
    assert_eq!(a.fingerprint().len(), 64);
}

#[test]
fn original_modifie_jamais_supprime() {
    let w = World::new();
    let plan = w.prepare(SRC, 'D').unwrap();
    let staged = w.run(&plan).unwrap();
    // Un programme a écrit dans l'original mis de côté après la copie.
    w.fs.file(&format!("{SRC}{STAGING_SUFFIX}\\écrit après.txt"), b"x");
    assert!(matches!(finish(&staged, true, &w.fs, &w.fs), Finished::DeleteFailed(_)));
    assert!(w.fs.node(&format!("{SRC}{STAGING_SUFFIX}\\écrit après.txt")).is_some());
}

#[test]
fn reprise_d_un_deplacement_interrompu() {
    // Interrompu à chaque étape (Lumen fermé) : la reprise remet tout comme avant.
    for (op, nth, stuck) in [(Op::CopyFile, 3, Op::DeleteTree), (Op::CopyDirMeta, 2, Op::DeleteTree), (Op::CreateJunction, 1, Op::Rename), (Op::WrongJunction, 1, Op::Rename)] {
        let w = World::new();
        let free = w.fs.free('D');
        let plan = w.prepare(SRC, 'D').unwrap();
        // Panne puis retour en arrière impossible : l'état intermédiaire reste sur le disque.
        w.fs.fail_on(op, nth);
        w.fs.fail_on(stuck, if stuck == Op::Rename { 2 } else { 1 });
        assert!(matches!(w.run(&plan), Err(Failure::Step { rolled_back: false, .. })), "{op:?}");
        assert_eq!(recover(&p(SRC), &p(DST), &w.fs, &w.fs), Recovered::RolledBack, "{op:?}");
        assert_untouched(&w, free);
    }
    // Interrompu en attendant la confirmation.
    let w = World::new();
    let free = w.fs.free('D');
    let plan = w.prepare(SRC, 'D').unwrap();
    w.run(&plan).unwrap();
    assert_eq!(recover(&p(SRC), &p(DST), &w.fs, &w.fs), Recovered::RolledBack);
    assert_untouched(&w, free);
    // Interrompu juste après la suppression de l'original : le déplacement est fini.
    let w = moved();
    assert_eq!(recover(&p(SRC), &p(DST), &w.fs, &w.fs), Recovered::Done);
    assert_eq!(w.fs.snapshot(SRC), original());
    // État inconnu : rien n'est touché.
    let w = World::new();
    w.fs.recycle(&p(SRC)).unwrap();
    assert!(matches!(recover(&p(SRC), &p(DST), &w.fs, &w.fs), Recovered::NeedsAttention(_)));
}
