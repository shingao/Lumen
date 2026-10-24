//! Tests Windows réels du déplacement intelligent : un dossier du disque système part sur un
//! autre disque fixe NTFS (D: sur les machines de la CI), avec de vrais flux alternatifs,
//! attributs, dates et ACL, une vraie jonction, le vrai Restart Manager et de vraies pannes à
//! chaque étape. Aucune sonde simulée, sauf l'espace libre annoncé (cible pleine).
#![allow(clippy::disallowed_methods)] // Les fixtures créent et suppriment leurs propres fichiers.

use std::cell::Cell;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime};

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::{HLOCAL, LocalFree, ERROR_SUCCESS};
use windows::Win32::Security::Authorization::{ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT};
use windows::Win32::Security::{GetSecurityDescriptorControl, DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SE_DACL_PROTECTED};
use windows::Win32::Storage::FileSystem::GetLogicalDrives;

use super::relocate::{app_data_roots, synced_roots, WinMover};
use super::system_layout;
use super::tests::Subst;
use crate::bin::DriveKind;
use crate::denylist::DenyList;
use crate::path::WinPath;
use crate::probe::{Probe, ProbeError};
use crate::relocate::{
    finish, health, prepare, run, staging_of, target_for, undo, Failure, Finished, Health, MoveContext, MoveEffects, MoveIoError, MoveProbe, PathKind,
    Prepared, Refusal, Staged, Step, TreeEntry, Undone, VolumeInfo,
};

static NEXT: AtomicU32 = AtomicU32::new(0);

fn wp(raw: &str) -> WinPath {
    WinPath::parse(raw).unwrap()
}

fn s(p: &Path) -> String {
    p.to_str().unwrap().to_owned()
}

/// Un autre disque fixe NTFS que celui du dossier temporaire (D: sur la CI).
fn other_drive(source: char) -> char {
    // SAFETY : appel sans paramètre.
    let used = unsafe { GetLogicalDrives() };
    ('C'..='Z')
        .filter(|d| *d != source && used & (1 << (*d as u32 - 'A' as u32)) != 0)
        .find(|d| WinMover.volume(*d).is_some_and(|v| v.kind == DriveKind::Fixed && !v.subst && v.ntfs))
        .expect("un second disque fixe NTFS est nécessaire (D: sur la CI)")
}

struct World {
    root: PathBuf,
    source: WinPath,
    drive: char,
    deny: DenyList,
    synced: Vec<WinPath>,
    app_data: Vec<WinPath>,
}

impl World {
    /// Dossier à déplacer : fichiers, sous-dossier, flux alternatifs (fichier et dossier),
    /// lecture seule + caché, dates anciennes, une ACL protégée.
    fn new(name: &str) -> World {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("lumen-move-{}-{name}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let src = root.join("Rushes été");
        fs::create_dir_all(src.join("sous dossier\\profond")).unwrap();
        fs::write(src.join("a.bin"), vec![7u8; 300_000]).unwrap();
        fs::write(src.join("a.bin:Zone.Identifier"), b"[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();
        fs::write(src.join("sous dossier\\b.txt"), "bonjour").unwrap();
        fs::write(src.join("sous dossier\\profond\\vide.dat"), b"").unwrap();
        fs::write(src.join("sous dossier:Lumen.Test"), b"flux du dossier").unwrap();
        let ro = src.join("z lecture seule.txt");
        fs::write(&ro, "ne pas modifier").unwrap();
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        File::options().write(true).open(src.join("a.bin")).unwrap().set_modified(old).unwrap();
        // ACL protégée (héritage coupé, entrées copiées) sur un fichier.
        let st = std::process::Command::new("icacls").arg(s(&src.join("sous dossier\\b.txt"))).arg("/inheritance:d").output().unwrap();
        assert!(st.status.success(), "{st:?}");
        let mut perms = fs::metadata(&ro).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&ro, perms).unwrap();
        attrib_hidden(&ro);
        let source = super::WinProbe.canonicalize(&wp(&s(&src))).unwrap();
        let drive = other_drive(match source.prefix() {
            crate::path::Prefix::Drive(d) => *d,
            crate::path::Prefix::Unc { .. } => unreachable!(),
        });
        World { root, source, drive, deny: DenyList::new(&system_layout().unwrap()), synced: synced_roots(), app_data: app_data_roots() }
    }

    fn ctx(&self) -> MoveContext<'_> {
        MoveContext { deny: &self.deny, game_dirs: &[], synced_roots: &self.synced, app_data: &self.app_data }
    }

    fn target(&self) -> WinPath {
        target_for(&self.source, self.drive).unwrap()
    }

    fn prepare(&self) -> Result<Prepared, Vec<Refusal>> {
        prepare(&self.source, self.drive, &self.ctx(), &WinMover)
    }

    fn run_with(&self, fx: &dyn MoveEffects) -> Result<Staged, Failure> {
        let plan = self.prepare().unwrap();
        run(&plan, &plan.fingerprint(), &self.ctx(), &WinMover, fx, &|_, _, _| {})
    }

    fn src(&self, rel: &str) -> PathBuf {
        let top = PathBuf::from(self.source.display_string());
        if rel.is_empty() {
            top
        } else {
            top.join(rel)
        }
    }
}

impl Drop for World {
    fn drop(&mut self) {
        // Jonction d'abord (jamais suivie), puis les copies sur le disque cible.
        let _ = WinMover.remove_junction(&self.source, None);
        clear_readonly(&PathBuf::from(self.target().display_string()));
        let _ = fs::remove_dir_all(self.target().display_string());
        clear_readonly(&self.root);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn attrib_hidden(p: &Path) {
    let st = std::process::Command::new("attrib").arg("+h").arg(s(p)).output().unwrap();
    assert!(st.status.success());
}

fn clear_readonly(dir: &Path) {
    let _ = std::process::Command::new("attrib").arg("-r").arg("-h").arg("/s").arg("/d").arg(format!("{}\\*", s(dir))).output();
}

/// DACL en SDDL et drapeau « protégée ».
fn dacl(p: &Path) -> (String, bool) {
    let mut sd = PSECURITY_DESCRIPTOR::default();
    // SAFETY : chaîne possédée ; descripteur libéré par LocalFree.
    let st = unsafe { GetNamedSecurityInfoW(&HSTRING::from(s(p)), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, None, None, None, None, &mut sd) };
    assert_eq!(st, ERROR_SUCCESS, "{p:?}");
    let mut control = 0u16;
    let mut rev = 0u32;
    // SAFETY : descripteur valide.
    unsafe { GetSecurityDescriptorControl(sd, &mut control, &mut rev) }.unwrap();
    let mut text = PWSTR::null();
    // SAFETY : descripteur valide ; chaîne libérée par LocalFree.
    unsafe { ConvertSecurityDescriptorToStringSecurityDescriptorW(sd, SDDL_REVISION_1, DACL_SECURITY_INFORMATION, &mut text, None) }.unwrap();
    // SAFETY : chaîne terminée par un nul renvoyée par Windows.
    let out = unsafe { text.to_string() }.unwrap();
    // SAFETY : allocations de Windows libérées une fois.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(text.0 as *mut _)));
        let _ = LocalFree(Some(HLOCAL(sd.0)));
    }
    (out, control & SE_DACL_PROTECTED.0 != 0)
}

/// Entrées d'une DACL, sans l'indicateur « hérité » : les droits effectifs.
fn effective(sddl: &str) -> Vec<String> {
    let mut aces: Vec<String> = sddl.split('(').skip(1).map(|a| a.trim_end_matches(')').replace("ID", "")).collect();
    aces.sort();
    aces
}

/// Contenu complet (fichiers, flux, attributs utiles, dates d'écriture) d'un dossier.
fn snapshot(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    fn rec(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let mut names: Vec<_> = fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        names.sort();
        for p in names {
            let m = fs::symlink_metadata(&p).unwrap();
            let rel = p.strip_prefix(base).unwrap().display().to_string();
            let attrs = m.file_attributes() & 0x7; // lecture seule, caché, système
            if m.is_dir() {
                out.push(format!("D {rel} {attrs:x}"));
                rec(base, &p, out);
            } else {
                out.push(format!("F {rel} {attrs:x} {} {:?} {}", m.len(), m.modified().unwrap(), blake3::hash(&fs::read(&p).unwrap())));
            }
        }
    }
    rec(dir, dir, &mut out);
    out
}

#[test]
fn deplacement_complet_fidele_puis_annulation() {
    let w = World::new("complet");
    let before = snapshot(&w.src(""));
    let before_dacl = dacl(&w.src("sous dossier\\b.txt"));
    let top_dacl = dacl(&w.src(""));
    let plan = w.prepare().unwrap();
    assert!(plan.review, "dossier temporaire = AppData\\Local : « À vérifier »");
    assert_eq!(plan.files, 4);
    let staged = w.run_with(&WinMover).unwrap();
    assert_eq!(WinMover.junction_target(&w.source), Some(w.target()));
    let target = PathBuf::from(w.target().display_string());
    // Copie fidèle : contenus, flux alternatifs, attributs, dates.
    assert_eq!(snapshot(&target), before);
    assert_eq!(fs::read(target.join("a.bin:Zone.Identifier")).unwrap(), b"[ZoneTransfer]\r\nZoneId=3\r\n");
    assert_eq!(fs::read(target.join("sous dossier:Lumen.Test")).unwrap(), b"flux du dossier");
    // ACL : protégée reste protégée, mêmes entrées ; le dossier déplacé garde ses droits effectifs
    // et n'hérite pas de son nouveau parent.
    let copied = dacl(&target.join("sous dossier\\b.txt"));
    assert!(copied.1);
    assert_eq!(effective(&copied.0), effective(&before_dacl.0));
    let top = dacl(&target);
    assert!(top.1, "dossier déplacé : ACL protégée");
    assert_eq!(effective(&top.0), effective(&top_dacl.0));
    // Avant la confirmation, l'original est encore là, mis de côté.
    assert_eq!(snapshot(&PathBuf::from(staging_of(&w.source).unwrap().display_string())), before);
    assert_eq!(finish(&staged, true, &WinMover, &WinMover), Finished::Done);
    assert!(!PathBuf::from(staging_of(&w.source).unwrap().display_string()).exists(), "original supprimé définitivement");
    // Les applis lisent au même chemin, à travers la jonction (flux compris).
    assert!(fs::symlink_metadata(w.src("")).unwrap().file_attributes() & 0x400 != 0, "une jonction");
    assert_eq!(snapshot(&w.src("")), before);
    assert_eq!(fs::read(w.src("a.bin:Zone.Identifier")).unwrap(), b"[ZoneTransfer]\r\nZoneId=3\r\n");
    assert_eq!(health(&w.source, &w.target(), &WinMover), Health::Ok);

    // Annuler : recopie vérifiée, jonction retirée, vrai dossier remis, copie à la Corbeille.
    let r = undo(&w.source, &w.target(), &WinMover, &WinMover, &|_, _, _| {});
    assert!(matches!(r, Undone::Done { .. }), "{r:?}");
    assert_eq!(WinMover.junction_target(&w.source), None);
    assert_eq!(WinMover.kind(&w.source), Some(PathKind::Dir));
    assert_eq!(snapshot(&w.src("")), before);
    assert_eq!(fs::read(w.src("sous dossier:Lumen.Test")).unwrap(), b"flux du dossier");
    assert!(dacl(&w.src("sous dossier\\b.txt")).1);
    assert!(!target.exists(), "copie de la cible envoyée à la Corbeille");
}

/// Effets réels, avec une panne choisie.
struct Faulty {
    op: &'static str,
    nth: Cell<u32>,
}

impl Faulty {
    fn new(op: &'static str, nth: u32) -> Faulty {
        Faulty { op, nth: Cell::new(nth) }
    }

    fn trip(&self, op: &str) -> bool {
        if op != self.op {
            return false;
        }
        let n = self.nth.get();
        self.nth.set(n.saturating_sub(1));
        n == 1
    }

    fn fail(&self, op: &str) -> Result<(), MoveIoError> {
        if self.trip(op) {
            Err(MoveIoError::Io(format!("panne simulée : {op}")))
        } else {
            Ok(())
        }
    }
}

impl MoveEffects for Faulty {
    fn create_dir(&self, path: &WinPath, template: &WinPath) -> Result<(), MoveIoError> {
        self.fail("create_dir")?;
        WinMover.create_dir(path, template)
    }
    fn copy_file(&self, from: &WinPath, to: &WinPath) -> Result<(), MoveIoError> {
        self.fail("copy_file")?;
        WinMover.copy_file(from, to)?;
        if self.trip("corrupt") {
            let mut f = OpenOptions::new().write(true).open(to.display_string()).unwrap();
            f.write_all(b"X").unwrap();
        }
        Ok(())
    }
    fn copy_dir_meta(&self, from: &WinPath, to: &WinPath, top: bool) -> Result<(), MoveIoError> {
        self.fail("copy_dir_meta")?;
        WinMover.copy_dir_meta(from, to, top)
    }
    fn rename(&self, from: &WinPath, to: &WinPath) -> Result<(), MoveIoError> {
        self.fail("rename")?;
        WinMover.rename(from, to)
    }
    fn create_junction(&self, link: &WinPath, target: &WinPath) -> Result<(), MoveIoError> {
        self.fail("create_junction")?;
        if self.trip("wrong_junction") {
            return WinMover.create_junction(link, &target.parent().unwrap());
        }
        WinMover.create_junction(link, target)
    }
    fn remove_junction(&self, link: &WinPath, expected: Option<&WinPath>) -> Result<(), MoveIoError> {
        self.fail("remove_junction")?;
        WinMover.remove_junction(link, expected)
    }
    fn delete_tree(&self, path: &WinPath) -> Result<(), MoveIoError> {
        self.fail("delete_tree")?;
        WinMover.delete_tree(path)
    }
    fn recycle(&self, path: &WinPath) -> Result<(), MoveIoError> {
        self.fail("recycle")?;
        WinMover.recycle(path)
    }
}

fn assert_untouched(w: &World, before: &[String]) {
    assert_eq!(WinMover.kind(&w.source), Some(PathKind::Dir), "un vrai dossier, pas une jonction");
    assert_eq!(snapshot(&w.src("")), before);
    assert!(!PathBuf::from(w.target().display_string()).exists(), "copie retirée");
    assert!(!PathBuf::from(staging_of(&w.source).unwrap().display_string()).exists());
}

#[test]
fn echec_a_chaque_etape_puis_retour_en_arriere() {
    let cases: &[(&str, u32, Step)] = &[
        ("create_dir", 1, Step::Copy),
        ("create_dir", 3, Step::Copy),
        ("copy_file", 2, Step::Copy),
        ("copy_dir_meta", 1, Step::Copy),
        ("copy_dir_meta", 3, Step::Copy),
        ("corrupt", 1, Step::Verify),
        ("rename", 1, Step::SetAside),
        ("create_junction", 1, Step::Junction),
        ("wrong_junction", 1, Step::Test),
    ];
    for &(op, nth, step) in cases {
        let w = World::new(op);
        let before = snapshot(&w.src(""));
        match w.run_with(&Faulty::new(op, nth)) {
            Err(Failure::Step { step: s, rolled_back: true, rollback_error: None, .. }) => assert_eq!(s, step, "{op} {nth}"),
            other => panic!("{op} {nth} : {other:?}"),
        }
        assert_untouched(&w, &before);
    }
}

#[test]
fn confirmation_refusee_et_suppression_ratee() {
    let w = World::new("refus");
    let before = snapshot(&w.src(""));
    let staged = w.run_with(&WinMover).unwrap();
    assert_eq!(finish(&staged, false, &WinMover, &WinMover), Finished::RolledBack);
    assert_untouched(&w, &before);
    // Suppression de l'original en échec : rien n'est perdu, la jonction mène à la copie.
    let staged = w.run_with(&WinMover).unwrap();
    assert!(matches!(finish(&staged, true, &WinMover, &Faulty::new("delete_tree", 1)), Finished::DeleteFailed(_)));
    assert_eq!(snapshot(&PathBuf::from(staging_of(&w.source).unwrap().display_string())), before);
    assert_eq!(snapshot(&w.src("")), before);
}

#[test]
fn annulation_en_echec_ne_perd_rien() {
    for (op, nth) in [("create_dir", 1), ("copy_file", 2), ("corrupt", 1), ("remove_junction", 1), ("rename", 1)] {
        let w = World::new(op);
        let before = snapshot(&w.src(""));
        let staged = w.run_with(&WinMover).unwrap();
        assert_eq!(finish(&staged, true, &WinMover, &WinMover), Finished::Done);
        let r = undo(&w.source, &w.target(), &WinMover, &Faulty::new(op, nth), &|_, _, _| {});
        assert!(matches!(r, Undone::Failed { .. }), "{op} {r:?}");
        assert_eq!(WinMover.junction_target(&w.source), Some(w.target()), "{op}");
        assert_eq!(snapshot(&w.src("")), before, "{op}");
        assert_eq!(snapshot(&PathBuf::from(w.target().display_string())), before);
    }
}

/// Sonde réelle, sauf l'espace libre annoncé pour le disque cible.
struct Full {
    drive: char,
    free: u64,
}

impl MoveProbe for Full {
    fn volume(&self, d: char) -> Option<VolumeInfo> {
        WinMover.volume(d).map(|v| if d == self.drive { VolumeInfo { free: self.free, ..v } } else { v })
    }
    fn kind(&self, p: &WinPath) -> Option<PathKind> {
        WinMover.kind(p)
    }
    fn tree(&self, d: &WinPath) -> Result<Vec<TreeEntry>, ProbeError> {
        WinMover.tree(d)
    }
    fn holders(&self, f: &[WinPath]) -> Vec<String> {
        WinMover.holders(f)
    }
    fn hash(&self, f: &WinPath) -> Result<[u8; 32], ProbeError> {
        WinMover.hash(f)
    }
    fn junction_target(&self, l: &WinPath) -> Option<WinPath> {
        WinMover.junction_target(l)
    }
}

#[test]
fn cible_pleine_refusee() {
    let w = World::new("plein");
    let before = snapshot(&w.src(""));
    let bytes = w.prepare().unwrap().bytes;
    // Il faut la taille plus 10 % : juste en dessous, refusé.
    let full = Full { drive: w.drive, free: bytes + bytes / 10 - 1 };
    match prepare(&w.source, w.drive, &w.ctx(), &full) {
        Err(r) => assert!(matches!(r[..], [Refusal::NotEnoughSpace { .. }]), "{r:?}"),
        Ok(_) => panic!("cible pleine acceptée"),
    }
    // Remplie entre la confirmation et l'action : le recontrôle refuse, rien n'est fait.
    let plan = w.prepare().unwrap();
    let r = run(&plan, &plan.fingerprint(), &w.ctx(), &Full { drive: w.drive, free: 10 }, &WinMover, &|_, _, _| {});
    assert!(matches!(r, Err(Failure::Refused(_))), "{r:?}");
    assert_untouched(&w, &before);
}

#[test]
fn dossier_verrouille_refuse_avec_le_nom_du_programme() {
    let w = World::new("verrou");
    let before = snapshot(&w.src(""));
    let lock = OpenOptions::new().read(true).share_mode(0).open(w.src("sous dossier\\b.txt")).unwrap();
    match w.prepare() {
        Err(r) => match &r[..] {
            [Refusal::InUse(apps)] => assert!(!apps.is_empty() && apps.iter().all(|a| !a.trim().is_empty()), "{apps:?}"),
            other => panic!("{other:?}"),
        },
        Ok(_) => panic!("dossier ouvert accepté"),
    }
    // Ouvert après la confirmation : le recontrôle refuse aussi.
    drop(lock);
    let plan = w.prepare().unwrap();
    let lock = OpenOptions::new().read(true).share_mode(0).open(w.src("a.bin")).unwrap();
    assert!(matches!(run(&plan, &plan.fingerprint(), &w.ctx(), &WinMover, &WinMover, &|_, _, _| {}), Err(Failure::Refused(_))));
    drop(lock);
    assert_untouched(&w, &before);
}

#[test]
fn refus_reels_du_deplacement() {
    let w = World::new("refus-reels");
    let src_drive = match w.source.prefix() {
        crate::path::Prefix::Drive(d) => *d,
        crate::path::Prefix::Unc { .. } => unreachable!(),
    };
    let only = |path: &WinPath, to: char| prepare(path, to, &w.ctx(), &WinMover).err().unwrap_or_default();
    assert_eq!(only(&w.source, src_drive), [Refusal::SameVolume]);
    // Lecteur SUBST : jamais une cible.
    let subst = Subst::new(&w.root);
    assert!(only(&w.source, subst.drive).contains(&Refusal::TargetSubst));
    drop(subst);
    // Une jonction dans le dossier.
    let inner = w.src("lien");
    fs::create_dir_all(w.root.join("ailleurs")).unwrap();
    junction::create(w.root.join("ailleurs"), &inner).unwrap();
    assert_eq!(only(&w.source, w.drive), [Refusal::ReparsePoints]);
    fs::remove_dir(&inner).unwrap();
    // Un disque virtuel (WSL, Docker).
    fs::write(w.src("ext4.vhdx"), b"x").unwrap();
    assert_eq!(only(&w.source, w.drive), [Refusal::VirtualDisk]);
    fs::remove_file(w.src("ext4.vhdx")).unwrap();
    // Dossier protégé (Windows).
    let windows = system_layout().unwrap().windows.join("Temp").unwrap();
    assert!(only(&windows, w.drive).iter().any(|r| matches!(r, Refusal::Denied(_))));
    assert!(w.prepare().is_ok());
}

#[test]
fn jonctions_suivies() {
    let w = World::new("suivi");
    let staged = w.run_with(&WinMover).unwrap();
    assert_eq!(finish(&staged, true, &WinMover, &WinMover), Finished::Done);
    assert_eq!(health(&w.source, &w.target(), &WinMover), Health::Ok);
    // Disque cible absent : une lettre qui n'existe pas.
    // SAFETY : appel sans paramètre.
    let used = unsafe { GetLogicalDrives() };
    let absent = ('H'..='Y').rev().find(|d| used & (1 << (*d as u32 - 'A' as u32)) == 0).unwrap();
    assert_eq!(health(&w.source, &target_for(&w.source, absent).unwrap(), &WinMover), Health::TargetVolumeAbsent);
    // Copie disparue du disque cible.
    let target = PathBuf::from(w.target().display_string());
    let aside = target.with_file_name("ailleurs");
    fs::rename(&target, &aside).unwrap();
    assert_eq!(health(&w.source, &w.target(), &WinMover), Health::TargetMissing);
    fs::rename(&aside, &target).unwrap();
    // Jonction retirée par quelqu'un d'autre.
    WinMover.remove_junction(&w.source, Some(&w.target())).unwrap();
    assert_eq!(health(&w.source, &w.target(), &WinMover), Health::JunctionMissing);
}
