//! Tests portables du parcours (Linux), sur une vraie arborescence temporaire.

use std::fs::{self, File};
use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crate::model::Kind;
use crate::unix::StdSource;
use crate::walk::{walk, Progress, ScanError};

fn tree() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("Projets/app/node_modules/pkg")).unwrap();
    fs::create_dir_all(root.join("Vidéos")).unwrap();
    File::create(root.join("Projets/app/main.rs")).unwrap().write_all(&[1u8; 5000]).unwrap();
    File::create(root.join("Projets/app/node_modules/pkg/index.js")).unwrap().write_all(&[2u8; 9000]).unwrap();
    File::create(root.join("Vidéos/film é.mkv")).unwrap().write_all(&[3u8; 20000]).unwrap();
    fs::hard_link(root.join("Vidéos/film é.mkv"), root.join("Projets/film-lien.mkv")).unwrap();
    std::os::unix::fs::symlink(root.join("Vidéos"), root.join("raccourci")).unwrap();
    // Fichier clairsemé : 64 Mo logiques, presque rien d'alloué.
    File::create(root.join("clairseme.bin")).unwrap().set_len(64 * 1024 * 1024).unwrap();
    dir
}

fn scan(dir: &tempfile::TempDir) -> crate::Scan {
    let cancel = AtomicBool::new(false);
    walk(&StdSource, dir.path().to_str().unwrap(), &cancel, &|_: &Progress| {}).unwrap()
}

#[test]
fn compte_une_fois_les_liens_physiques() {
    let d = tree();
    let s = scan(&d);
    // main.rs, index.js, film (une seule fois), clairseme.bin.
    assert_eq!(s.stats.files, 4);
    assert_eq!(s.stats.hardlink_dups, 1);
    let total_logical = 5000 + 9000 + 20000 + 64 * 1024 * 1024;
    assert_eq!(s.stats.logical, total_logical);
}

#[test]
fn liens_symboliques_non_traverses() {
    let d = tree();
    let s = scan(&d);
    assert_eq!(s.stats.links, 1);
    let link = s.find("raccourci").unwrap();
    assert_eq!(s.node(link).unwrap().kind, Kind::Link);
    assert_eq!(s.children(link).len(), 0);
}

#[test]
fn taille_allouee_des_fichiers_clairsemes() {
    let d = tree();
    let s = scan(&d);
    let sparse = s.node(s.find("clairseme.bin").unwrap()).unwrap();
    assert_eq!(sparse.logical, 64 * 1024 * 1024);
    assert!(sparse.allocated < 1024 * 1024, "{}", sparse.allocated);
    assert!(s.stats.allocated < s.stats.logical);
}

#[test]
fn identite_enregistree_pour_safety() {
    let d = tree();
    let s = scan(&d);
    let id = s.find("Projets/app/main.rs").unwrap();
    let n = s.node(id).unwrap();
    let meta = fs::symlink_metadata(d.path().join("Projets/app/main.rs")).unwrap();
    use std::os::unix::fs::MetadataExt;
    assert_eq!(s.file_id(id), u128::from(meta.ino()));
    assert_eq!(n.logical, 5000);
    assert_eq!(n.modified, crate::unix::filetime(meta.mtime(), meta.mtime_nsec()));
}

#[test]
fn dossier_illisible_compte_sans_bloquer() {
    use std::os::unix::fs::PermissionsExt;
    let d = tree();
    let locked = d.path().join("prive");
    fs::create_dir(&locked).unwrap();
    File::create(locked.join("secret.txt")).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let s = scan(&d);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    // root peut tout lire : le test n'a de sens que pour un compte ordinaire.
    if s.stats.access_denied == 0 {
        assert_eq!(s.stats.files, 5);
    } else {
        assert_eq!(s.stats.access_denied, 1);
        assert_eq!(s.stats.files, 4);
    }
}

#[test]
fn progression_et_annulation() {
    let d = tree();
    for i in 0..200 {
        fs::create_dir_all(d.path().join(format!("gros/{i}"))).unwrap();
        File::create(d.path().join(format!("gros/{i}/f.txt"))).unwrap();
    }
    let calls = AtomicUsize::new(0);
    let cancel = AtomicBool::new(false);
    let s = walk(&StdSource, d.path().to_str().unwrap(), &cancel, &|p: &Progress| {
        calls.fetch_add(1, Ordering::Relaxed);
        assert!(p.files <= 300);
    })
    .unwrap();
    assert!(calls.load(Ordering::Relaxed) >= 1, "au moins l'événement final");
    assert_eq!(s.stats.files, 204);

    let cancelled = AtomicBool::new(true);
    let r = walk(&StdSource, d.path().to_str().unwrap(), &cancelled, &|_: &Progress| {});
    assert!(matches!(r, Err(ScanError::Cancelled)));
}

#[test]
fn tranche_agregee_pour_le_front() {
    let d = tree();
    let s = scan(&d);
    let slice = s.slice(s.root(), 2, 2).unwrap();
    assert!(slice.children.len() <= 3);
    assert_eq!(slice.children.iter().filter(|c| c.others.is_some()).count(), 1);
    let sum: u64 = slice.children.iter().map(|c| c.allocated).sum();
    assert_eq!(sum, slice.allocated);
}
