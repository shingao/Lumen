//! Tests Windows de `safety`, sur de vraies fixtures créées dans un dossier temporaire :
//! jonctions, liens symboliques, noms 8.3, `\\?\`, file ID NTFS, verrous, attribut SYSTEM,
//! chemins longs Unicode, Corbeille via IFileOperation. Aucune sonde simulée ici.
//!
//! La liste de ces tests est vérifiée par la CI (`scripts/check-safety-tests.mjs`) : un test
//! absent, ignoré ou non exécuté sous Windows fait échouer le build.
#![allow(clippy::disallowed_methods)] // Les fixtures créent et suppriment leurs propres fichiers.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::HSTRING;
use windows::Win32::Storage::FileSystem::{
    DefineDosDeviceW, GetLogicalDrives, SetFileAttributesW, DDD_REMOVE_DEFINITION, DEFINE_DOS_DEVICE_FLAGS,
    FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_SYSTEM,
};
use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};

use super::{long_path, profile_list, recycle, short_path, system_layout, volume_facts, WinProbe};
use crate::bin::{DriveKind, RecycleBlock};
use crate::allow::{Allowlist, Authority, RuleGrant, Scope};
use crate::check::{Change, Checker, Expected, Refusal, SkipReason, Target, Verdict};
use crate::denylist::{DenyList, DenyReason};
use crate::path::WinPath;
use crate::plan::{execute, simulate, Confirmation, DeleteMode, ExecError, Plan};
use crate::probe::Probe;

static NEXT: AtomicU32 = AtomicU32::new(0);
const RULE: &str = "fixture-cache";

/// Dossier temporaire propre à un test, supprimé à la fin (les jonctions ne sont pas suivies).
struct Fixture {
    root: PathBuf,
    cache: PathBuf,
    deny: DenyList,
    allow: Allowlist,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("lumen-safety-{}-{}-{n}", std::process::id(), name));
        let _ = fs::remove_dir_all(&root);
        let cache = root.join("cache");
        fs::create_dir_all(&cache).unwrap();
        let layout = system_layout().unwrap();
        let mut allow = Allowlist::new();
        allow.grant(RuleGrant { rule_id: RULE.into(), root: canonical(&cache), scope: Scope::Contents, min_age_secs: None });
        Fixture { root, cache, deny: DenyList::new(&layout), allow }
    }

    fn checker(&self) -> Checker<'_, WinProbe> {
        Checker { probe: &WinProbe, deny: &self.deny, allow: &self.allow, now: i64::MAX }
    }

    /// Cible « telle que le scan l'aurait vue » maintenant.
    fn target(&self, raw: &str) -> Target {
        let facts = WinProbe.facts(&WinProbe.canonicalize(&wp(raw)).unwrap()).unwrap();
        Target { path: raw.into(), authority: Authority::Rule(RULE.into()), expected: Expected::from_facts(&facts), bytes: facts.size }
    }

    fn check(&self, t: &Target) -> Verdict {
        self.checker().check(t)
    }

    fn file(&self, rel: &str, content: &[u8]) -> PathBuf {
        let p = self.cache.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        File::create(&p).unwrap().write_all(content).unwrap();
        p
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn wp(raw: &str) -> WinPath {
    WinPath::parse(raw).unwrap()
}

fn s(p: &Path) -> String {
    p.to_str().unwrap().to_owned()
}

fn canonical(p: &Path) -> WinPath {
    WinProbe.canonicalize(&wp(&s(p))).unwrap()
}

/// Cible dont l'identité attendue est celle d'un autre fichier, pour un chemin qu'on ne veut pas lire.
fn target_with(path: String, expected_from: &Target) -> Target {
    Target { path, ..expected_from.clone() }
}

fn windows_dir() -> PathBuf {
    PathBuf::from(system_layout().unwrap().windows.display_string())
}

#[test]
fn fixture_element_autorise_accepte() {
    let f = Fixture::new("ok");
    let p = f.file("index.json", b"{}");
    let v = f.check(&f.target(&s(&p)));
    assert!(v.is_accepted(), "{v:?}");
}

#[test]
fn adversarial_jonction_temp_vers_windows() {
    let f = Fixture::new("jonction");
    let link = f.cache.join("piege");
    junction::create(windows_dir(), &link).unwrap();

    // La jonction elle-même : un lien, jamais traité.
    let v = f.check(&f.target(&s(&link)));
    assert!(matches!(v.refusal(), Some(Refusal::ReparsePoint(_))), "{v:?}");

    // Un fichier à travers la jonction : son chemin réel est dans Windows.
    let hosts = link.join("System32").join("drivers").join("etc").join("hosts");
    let v = f.check(&f.target(&s(&hosts)));
    assert!(matches!(v.refusal(), Some(Refusal::NotAllowed | Refusal::Denied(_))), "{v:?}");

    // Même avec une règle qui couvrirait la jonction entière, rien ne passe.
    let mut wide = f.allow.clone();
    wide.grant(RuleGrant { rule_id: "large".into(), root: canonical(&f.root), scope: Scope::Contents, min_age_secs: None });
    let checker = Checker { probe: &WinProbe, deny: &f.deny, allow: &wide, now: i64::MAX };
    let mut t = f.target(&s(&hosts));
    t.authority = Authority::Rule("large".into());
    assert!(!checker.check(&t).is_accepted());

    // Le contenu de Windows est intact.
    assert!(windows_dir().join("System32").exists());
}

#[test]
fn fixture_jonction_parent_refusee() {
    // Une jonction vers un dossier anodin, placée sur le chemin : refusée elle aussi.
    let f = Fixture::new("jonction-parent");
    let real = f.root.join("ailleurs");
    fs::create_dir_all(&real).unwrap();
    File::create(real.join("a.txt")).unwrap();
    let link = f.cache.join("detour");
    junction::create(&real, &link).unwrap();
    let v = f.check(&f.target(&s(&link.join("a.txt"))));
    assert!(matches!(v.refusal(), Some(Refusal::NotAllowed | Refusal::ReparsePoint(_))), "{v:?}");
    assert!(real.join("a.txt").exists());
}

#[test]
fn fixture_liens_symboliques_refuses() {
    let f = Fixture::new("symlink");
    let outside = f.root.join("dehors.txt");
    File::create(&outside).unwrap().write_all(b"precieux").unwrap();
    let file_link = f.cache.join("lien.txt");
    std::os::windows::fs::symlink_file(&outside, &file_link).unwrap();
    let v = f.check(&f.target(&s(&file_link)));
    assert!(matches!(v.refusal(), Some(Refusal::ReparsePoint(_))), "{v:?}");

    let dir_link = f.cache.join("lien-dossier");
    std::os::windows::fs::symlink_dir(windows_dir(), &dir_link).unwrap();
    let v = f.check(&f.target(&s(&dir_link)));
    assert!(matches!(v.refusal(), Some(Refusal::ReparsePoint(_))), "{v:?}");
    assert_eq!(fs::read(&outside).unwrap(), b"precieux");
}

#[test]
fn fixture_noms_courts_8_3() {
    let f = Fixture::new("8.3");
    let long = f.file("Dossier au nom très long pour 8.3\\Fichier au nom très long.json", b"x");
    let short = short_path(&wp(&s(&long))).unwrap();
    assert!(short.contains('~'), "8.3 désactivé sur ce volume (fsutil 8dot3name set C: 0) : {short}");
    // Le nom court mène au même fichier, et le chemin réel est le nom long.
    let v = f.check(&f.target(&short));
    match v {
        Verdict::Accepted(ok) => assert_eq!(ok.path, canonical(&long)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn fixture_progra_1_vers_program_files_refuse() {
    let layout = system_layout().unwrap();
    let pf = layout.program_files[0].clone();
    let deny = DenyList::new(&layout);
    // Règle volontairement absurde : tout le volume. La denylist doit quand même refuser.
    let mut allow = Allowlist::new();
    allow.grant(RuleGrant { rule_id: "volume".into(), root: pf.volume_root(), scope: Scope::Contents, min_age_secs: None });
    let checker = Checker { probe: &WinProbe, deny: &deny, allow: &allow, now: i64::MAX };
    let facts = WinProbe.facts(&pf).unwrap();
    let short = short_path(&pf).unwrap();
    let t = Target { path: short.clone(), authority: Authority::Rule("volume".into()), expected: Expected::from_facts(&facts), bytes: 0 };
    let v = checker.check(&t);
    if short.contains('~') {
        assert_eq!(v.refusal(), Some(&Refusal::Denied(DenyReason::ProgramFiles)), "{short} : {v:?}");
    } else {
        assert!(!v.is_accepted(), "{v:?}");
    }
    let literal = Target { path: format!("{}PROGRA~1", pf.volume_root().display_string()), ..t };
    assert!(!checker.check(&literal).is_accepted());
}

#[test]
fn fixture_prefixe_verbatim_et_casse() {
    let f = Fixture::new("verbatim");
    let p = f.file("Casse.TXT", b"x");
    let verbatim = format!("\\\\?\\{}", s(&p));
    assert!(f.check(&f.target(&verbatim)).is_accepted());
    assert!(f.check(&f.target(&s(&p).to_uppercase())).is_accepted());

    // \\?\ vers Windows, avec une règle qui couvrirait tout le volume : denylist.
    let win = system_layout().unwrap().windows;
    let mut allow = Allowlist::new();
    allow.grant(RuleGrant { rule_id: "volume".into(), root: win.volume_root(), scope: Scope::Contents, min_age_secs: None });
    let checker = Checker { probe: &WinProbe, deny: &f.deny, allow: &allow, now: i64::MAX };
    let hosts = win.join("System32\\drivers\\etc\\hosts").unwrap();
    let facts = WinProbe.facts(&hosts).unwrap();
    for raw in [hosts.verbatim(), hosts.display_string().to_lowercase(), format!("{}\\..\\{}", win.display_string(), "Windows\\System32\\drivers\\etc\\hosts")] {
        let t = Target { path: raw.clone(), authority: Authority::Rule("volume".into()), expected: Expected::from_facts(&facts), bytes: 0 };
        assert!(matches!(checker.check(&t).refusal(), Some(Refusal::Denied(_))), "{raw}");
    }
}

#[test]
fn fixture_chemin_relatif_refuse() {
    let f = Fixture::new("relatif");
    let p = f.file("a.txt", b"x");
    let t = f.target(&s(&p));
    for raw in ["cache\\a.txt", "..\\cache\\a.txt", "\\Windows\\System32"] {
        let v = f.check(&target_with(raw.into(), &t));
        assert!(matches!(v.refusal(), Some(Refusal::InvalidPath(_))), "{raw} : {v:?}");
    }
}

#[test]
fn fixture_file_id_ntfs() {
    let f = Fixture::new("fileid");
    let a = f.file("a.bin", b"aaaa");
    let b = f.file("b.bin", b"aaaa");
    let link = f.cache.join("a-lien-physique.bin");
    fs::hard_link(&a, &link).unwrap();
    let fa = WinProbe.facts(&canonical(&a)).unwrap();
    let fa2 = WinProbe.facts(&canonical(&a)).unwrap();
    let fb = WinProbe.facts(&canonical(&b)).unwrap();
    let fl = WinProbe.facts(&canonical(&link)).unwrap();
    assert_eq!(fa, fa2);
    assert_ne!(fa.file_id, fb.file_id);
    assert_eq!(fa.file_id, fl.file_id, "un lien physique est le même fichier");
    assert_ne!(fa.file_id.volume_serial, 0);
    assert_eq!(fa.size, 4);
    assert!(fa.modified > 0);
}

#[test]
fn adversarial_toctou_fichier_remplace() {
    let f = Fixture::new("toctou");
    let p = f.file("rapport.pdf", b"version originale");
    let scanned = f.target(&s(&p));
    let modified = fs::metadata(&p).unwrap().modified().unwrap();

    // Entre le scan et l'action : même nom, même taille, même date… autre fichier.
    fs::remove_file(&p).unwrap();
    File::create(&p).unwrap().write_all(b"version piratee!!").unwrap();
    File::options().write(true).open(&p).unwrap().set_modified(modified).unwrap();
    let now = WinProbe.facts(&canonical(&p)).unwrap();
    assert_eq!((now.size, now.modified), (scanned.expected.size, scanned.expected.modified));
    assert_eq!(f.check(&scanned).refusal(), Some(&Refusal::Changed(Change::Identity)));

    // Simple modification du contenu : refusée aussi.
    let q = f.file("notes.txt", b"abc");
    let scanned = f.target(&s(&q));
    OpenOptions::new().append(true).open(&q).unwrap().write_all(b"d").unwrap();
    assert_eq!(f.check(&scanned).refusal(), Some(&Refusal::Changed(Change::Size)));
}

#[test]
fn adversarial_chemin_long_unicode_espaces() {
    let f = Fixture::new("long");
    let segment = "dossier avec espaces é 漢字 🙂";
    let mut long = f.cache.clone();
    for _ in 0..10 {
        long.push(segment);
    }
    let verbatim_dir = PathBuf::from(format!("\\\\?\\{}", s(&long)));
    fs::create_dir_all(&verbatim_dir).unwrap();
    let file = verbatim_dir.join("fichier final é.txt");
    File::create(&file).unwrap().write_all(b"x").unwrap();
    let raw = format!("{}\\fichier final é.txt", s(&long));
    assert!(raw.chars().count() > 260, "{}", raw.chars().count());

    // Contrôle positif : un vrai fichier long, dans la zone autorisée, est bien lu et accepté.
    let legit = f.target(&raw);
    assert!(f.check(&legit).is_accepted());

    // 1. Le même chemin long qui remonte par « .. » jusqu'à Windows.
    let depth = wp(&raw).depth();
    let escape = format!("{raw}{}\\{}", "\\..".repeat(depth), "Windows\\System32\\drivers\\etc\\hosts");
    let v = f.check(&target_with(escape, &legit));
    assert!(!v.is_accepted(), "{v:?}");

    // 2. Un chemin long Unicode hors de la zone autorisée.
    let mut sibling = f.root.join("hors zone");
    for _ in 0..10 {
        sibling.push(segment);
    }
    fs::create_dir_all(PathBuf::from(format!("\\\\?\\{}", s(&sibling)))).unwrap();
    let outside = format!("{}\\fichier.txt", s(&sibling));
    File::create(PathBuf::from(format!("\\\\?\\{outside}"))).unwrap();
    let v = f.check(&f.target(&outside));
    assert_eq!(v.refusal(), Some(&Refusal::NotAllowed), "{v:?}");

    // 3. Le chemin long autorisé, mais le fichier a été remplacé depuis le scan.
    fs::remove_file(&file).unwrap();
    File::create(&file).unwrap().write_all(b"x").unwrap();
    assert_eq!(f.check(&legit).refusal(), Some(&Refusal::Changed(Change::Identity)));

    // 4. Le chemin long à travers une jonction vers Windows.
    let link = f.cache.join("raccourci piégé é");
    junction::create(windows_dir(), &link).unwrap();
    let mut through = link.clone();
    for _ in 0..8 {
        through.push(segment);
    }
    let v = f.check(&target_with(s(&through), &legit));
    assert!(!v.is_accepted(), "{v:?}");
}

#[test]
fn fixture_fichier_verrouille_ignore_et_compte() {
    let f = Fixture::new("verrou");
    let p = f.file("ouvert.log", b"x");
    let t = f.target(&s(&p));
    {
        let _held = OpenOptions::new().read(true).share_mode(0).open(&p).unwrap();
        assert!(matches!(f.check(&t), Verdict::Skipped { reason: SkipReason::Locked, .. }));
        let plan = Plan::build(&f.checker(), std::slice::from_ref(&t), DeleteMode::Trash);
        assert_eq!((plan.items().len(), plan.skipped().len(), plan.refused().len()), (0, 1, 0));
    }
    assert!(f.check(&t).is_accepted());
}

#[test]
fn fixture_attribut_system_refuse() {
    let f = Fixture::new("system");
    let p = f.file("desktop.ini", b"[.ShellClassInfo]");
    let name = HSTRING::from(s(&p));
    // SAFETY : chaîne terminée par un nul possédée par `name`.
    unsafe { SetFileAttributesW(&name, FILE_ATTRIBUTE_SYSTEM) }.unwrap();
    let v = f.check(&f.target(&s(&p)));
    assert_eq!(v.refusal(), Some(&Refusal::Denied(DenyReason::SystemAttribute)));
    // SAFETY : idem ; rend le fichier supprimable par le nettoyage de la fixture.
    unsafe { SetFileAttributesW(&name, FILE_ATTRIBUTE_NORMAL) }.unwrap();
}

#[test]
fn fixture_dossier_git_refuse() {
    let f = Fixture::new("git");
    let project = f.cache.join("projet");
    fs::create_dir_all(project.join(".git")).unwrap();
    fs::create_dir_all(project.join("node_modules")).unwrap();
    let v = f.check(&f.target(&s(&project)));
    assert_eq!(v.refusal(), Some(&Refusal::Denied(DenyReason::GitProject)));
    let v = f.check(&f.target(&s(&project.join(".git"))));
    assert_eq!(v.refusal(), Some(&Refusal::Denied(DenyReason::GitInternals)));
    // L'artefact régénérable, lui, passe.
    assert!(f.check(&f.target(&s(&project.join("node_modules")))).is_accepted());
}

#[test]
fn fixture_dossiers_connus_proteges() {
    let layout = system_layout().unwrap();
    let deny = DenyList::new(&layout);
    assert_eq!(deny.check(&layout.windows), Some(DenyReason::Windows));
    assert_eq!(deny.check(&layout.program_data), Some(DenyReason::ProgramData));
    assert_eq!(deny.check(&layout.profile), Some(DenyReason::ProfileRoot));
    assert_eq!(deny.check(&layout.users), Some(DenyReason::ProfileRoot));
    assert!(deny.check(&layout.public).is_some());
    for pf in &layout.program_files {
        assert_eq!(deny.check(pf), Some(DenyReason::ProgramFiles));
    }
    let temp = canonical(&std::env::temp_dir());
    assert_eq!(deny.check(&temp.join("lumen-test.tmp").unwrap()), None, "{temp}");
}

#[test]
fn fixture_simulation_ne_touche_a_rien() {
    let f = Fixture::new("dry-run");
    let a = f.file("a.bin", b"aaaa");
    let b = f.file("b.bin", b"bb");
    let plan = Plan::build(&f.checker(), &[f.target(&s(&a)), f.target(&s(&b))], DeleteMode::Trash);
    let dry = simulate(&plan);
    assert_eq!((dry.items, dry.bytes), (2, 6));
    let confirmation = Confirmation { fingerprint: plan.fingerprint(), permanent_acknowledged: true };
    assert_eq!(execute(&plan, &confirmation), Err(ExecError::RealExecutionDisabled));
    assert!(a.exists() && b.exists());
}

fn recycle_bin_items(drive_root: &str) -> i64 {
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
    let root = HSTRING::from(drive_root);
    // SAFETY : chaîne et structure possédées, cbSize renseigné.
    unsafe { SHQueryRecycleBinW(&root, &mut info) }.unwrap();
    info.i64NumItems
}

#[test]
fn fixture_corbeille_ifileoperation() {
    let f = Fixture::new("corbeille");
    let p = f.file("a recycler é.txt", b"lumen");
    let path = canonical(&p);
    let drive = format!("{}\\", &path.display_string()[..2]);
    let before = recycle_bin_items(&drive);
    recycle(&path, 5).unwrap();
    assert!(!p.exists(), "le fichier doit avoir quitté son dossier");
    assert_eq!(recycle_bin_items(&drive), before + 1, "le fichier doit être dans la Corbeille, pas détruit");
}

#[test]
fn fixture_volume_fixe_avec_corbeille() {
    let f = Fixture::new("volume");
    let p = f.file("a.txt", b"x");
    let facts = volume_facts(&canonical(&p)).unwrap();
    assert_eq!(facts.kind, DriveKind::Fixed);
    assert!(!facts.subst);
    assert!(!facts.recycle_disabled, "Corbeille désactivée sur le volume du dossier temporaire ?");
}

/// Lecteur SUBST temporaire (session courante), retiré à la fin du test.
struct Subst {
    drive: char,
    target: String,
}

impl Subst {
    fn new(target: &Path) -> Subst {
        // SAFETY : appel sans paramètre.
        let used = unsafe { GetLogicalDrives() };
        let drive = ('H'..='Y').rev().find(|d| used & (1 << (*d as u32 - 'A' as u32)) == 0).unwrap();
        let target = s(target);
        // SAFETY : chaînes possédées.
        unsafe { DefineDosDeviceW(DEFINE_DOS_DEVICE_FLAGS(0), &HSTRING::from(format!("{drive}:")), &HSTRING::from(target.as_str())) }
            .unwrap();
        Subst { drive, target }
    }
}

impl Drop for Subst {
    fn drop(&mut self) {
        // SAFETY : retire exactement la définition créée par `new`.
        unsafe {
            let _ = DefineDosDeviceW(
                DDD_REMOVE_DEFINITION,
                &HSTRING::from(format!("{}:", self.drive)),
                &HSTRING::from(self.target.as_str()),
            );
        }
    }
}

#[test]
fn adversarial_lecteur_subst_sans_corbeille() {
    let f = Fixture::new("subst");
    let p = f.file("via-subst.txt", b"precieux");
    let subst = Subst::new(&f.cache);
    let via = format!("{}:\\via-subst.txt", subst.drive);
    assert!(Path::new(&via).exists(), "lecteur SUBST non créé");

    // Pré-contrôle : le lecteur demandé est un SUBST, sans Corbeille.
    let facts = volume_facts(&wp(&via)).unwrap();
    assert!(facts.subst, "{facts:?}");

    // Le contrôle de sécurité passe (le chemin réel est dans la zone autorisée)…
    let t = f.target(&via);
    assert!(f.check(&t).is_accepted());
    // … mais l'envoi à la Corbeille est refusé, et rien n'est supprimé.
    let plan = Plan::build(&f.checker(), std::slice::from_ref(&t), DeleteMode::Trash);
    assert!(plan.items().is_empty());
    assert_eq!(plan.refused()[0].1, Refusal::NoRecycleBin(RecycleBlock::Subst));
    assert!(matches!(recycle(&wp(&via), 8), Err(super::recycle::RecycleError::NoRecycleBin(RecycleBlock::Subst))));
    assert_eq!(fs::read(&p).unwrap(), b"precieux");
}

#[test]
fn fixture_profils_du_registre() {
    let layout = system_layout().unwrap();
    let profiles = profile_list().unwrap();
    // Le profil courant figure dans ProfileList, comparé en chemin long résolu.
    assert!(profiles.contains(&layout.profile), "{} absent de {profiles:?}", layout.profile);
    // Tous les autres profils du registre sont refusés, où qu'ils soient.
    let deny = DenyList::new(&layout);
    for other in profiles.iter().filter(|p| **p != layout.profile) {
        assert!(deny.check(other).is_some(), "{other}");
        assert!(deny.check(&other.join("AppData").unwrap()).is_some(), "{other}");
    }

    // Le nom court 8.3 du profil courant résout vers le même chemin long : pas « un autre profil ».
    let short = short_path(&layout.profile).unwrap();
    assert_eq!(wp(&long_path(&short)), layout.profile, "{short}");
    let temp = canonical(&std::env::temp_dir());
    assert_eq!(deny.check(&temp.join("x.tmp").unwrap()), None);

    // Un dossier au nom anodin, déclaré comme profil d'un autre compte : refusé par le pipeline.
    let f = Fixture::new("profil");
    let p = f.file("notes.txt", b"x");
    let mut other_layout = layout.clone();
    other_layout.other_profiles.push(canonical(&f.cache));
    let deny = DenyList::new(&other_layout);
    let checker = Checker { probe: &WinProbe, deny: &deny, allow: &f.allow, now: i64::MAX };
    assert_eq!(checker.check(&f.target(&s(&p))).refusal(), Some(&Refusal::Denied(DenyReason::OtherProfile)));
}
