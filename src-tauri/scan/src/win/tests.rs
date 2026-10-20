//! Tests Windows du scanner, sur une vraie arborescence de fixtures : liens physiques, fichiers
//! clairsemés et compressés, jonction, lien symbolique, petits fichiers résidents, flux nommé,
//! chemin long Unicode, fichier hors ligne, fichier verrouillé. Le parcours et la MFT doivent
//! donner exactement les mêmes totaux.

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use windows::core::HSTRING;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{SetFileAttributesW, FILE_ATTRIBUTE_OFFLINE};
use windows::Win32::System::Ioctl::{FSCTL_SET_COMPRESSION, FSCTL_SET_SPARSE};
use windows::Win32::System::IO::DeviceIoControl;

use super::{is_elevated, mft::scan_mft, root_path, WinSource};
use crate::model::{Kind, Scan};
use crate::walk::{walk, DirError, Progress, Source};

fn s(p: &Path) -> String {
    p.to_str().unwrap().to_owned()
}

fn ioctl(file: &File, code: u32, input: &[u8]) {
    let mut returned = 0u32;
    // SAFETY : handle valide pendant l'appel, tampon d'entrée possédé.
    unsafe {
        DeviceIoControl(
            HANDLE(file.as_raw_handle()),
            code,
            (!input.is_empty()).then_some(input.as_ptr() as *const c_void),
            input.len() as u32,
            None,
            0,
            Some(&mut returned),
            None,
        )
    }
    .unwrap();
}

/// Arborescence de test. Fichiers comptés (une fois) : 0 o, 1 o, 700 o (résident), 5 000 o,
/// 70 000 o, 1 Mio (+ 2 liens physiques), clairsemé, compressé, Unicode long, verrouillé = 10.
fn fixtures() -> tempfile::TempDir {
    let dir = tempfile::Builder::new().prefix("lumen-scan-").tempdir().unwrap();
    let r = dir.path();
    fs::create_dir_all(r.join("Projets\\app\\node_modules\\pkg")).unwrap();
    fs::create_dir_all(r.join("Vidéos")).unwrap();
    fs::create_dir_all(r.join("vide")).unwrap();
    for (name, size) in [("zero.txt", 0usize), ("un.txt", 1), ("resident.txt", 700), ("Projets\\app\\main.rs", 5000), ("Projets\\app\\node_modules\\pkg\\index.js", 70_000)] {
        File::create(r.join(name)).unwrap().write_all(&vec![7u8; size]).unwrap();
    }
    File::create(r.join("Vidéos\\film é.mkv")).unwrap().write_all(&vec![3u8; 1024 * 1024]).unwrap();
    fs::hard_link(r.join("Vidéos\\film é.mkv"), r.join("Projets\\film-lien.mkv")).unwrap();
    fs::hard_link(r.join("Vidéos\\film é.mkv"), r.join("Vidéos\\film-copie.mkv")).unwrap();

    // Clairsemé : 64 Mio logiques, 4 Kio écrits à 1 Mio.
    let mut sparse = OpenOptions::new().create(true).truncate(true).read(true).write(true).open(r.join("clairseme.bin")).unwrap();
    ioctl(&sparse, FSCTL_SET_SPARSE, &[]);
    sparse.set_len(64 * 1024 * 1024).unwrap();
    sparse.seek(SeekFrom::Start(1024 * 1024)).unwrap();
    sparse.write_all(&[9u8; 4096]).unwrap();
    // Tant que le cache n'est pas écrit, NTFS n'a pas encore alloué (ni compressé) les données :
    // le parcours et la MFT doivent voir le même état disque.
    sparse.sync_all().unwrap();
    drop(sparse);

    // Compressé NTFS : 2 Mio de texte répétitif (compressible, mais pas que des zéros).
    let mut compressed = OpenOptions::new().create(true).truncate(true).read(true).write(true).open(r.join("compresse.bin")).unwrap();
    ioctl(&compressed, FSCTL_SET_COMPRESSION, &1u16.to_le_bytes());
    let text: Vec<u8> = b"Lumen compresse ce texte. ".iter().copied().cycle().take(2 * 1024 * 1024).collect();
    compressed.write_all(&text).unwrap();
    compressed.sync_all().unwrap();
    drop(compressed);

    // Flux nommé : ignoré par les deux méthodes, comme par l'Explorateur.
    File::create(r.join("un.txt:secret")).unwrap().write_all(&[1u8; 50_000]).unwrap();

    // Chemin long Unicode (> 260 caractères).
    let mut long = PathBuf::from(format!("\\\\?\\{}", s(r)));
    for _ in 0..9 {
        long.push("dossier très long é 漢字 🙂 avec espaces");
    }
    fs::create_dir_all(&long).unwrap();
    File::create(long.join("profond.txt")).unwrap().write_all(&[5u8; 9000]).unwrap();

    File::create(r.join("verrouille.log")).unwrap().write_all(&[1u8; 8192]).unwrap();

    // Hors ligne (comme un fichier cloud non téléchargé) : compté à part, jamais ouvert.
    File::create(r.join("cloud.docx")).unwrap().write_all(&[1u8; 20_000]).unwrap();
    // SAFETY : chaîne possédée.
    unsafe { SetFileAttributesW(&HSTRING::from(s(&r.join("cloud.docx"))), FILE_ATTRIBUTE_OFFLINE) }.unwrap();

    // Liens : jamais traversés.
    junction::create(std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into()), r.join("jonction-windows")).unwrap();
    std::os::windows::fs::symlink_file(r.join("un.txt"), r.join("lien.txt")).unwrap();
    dir
}

fn scan_walk(root: &Path) -> Scan {
    let root = root_path(&s(root));
    walk(&WinSource::new(&root), &root, &AtomicBool::new(false), &|_: &Progress| {}).unwrap()
}

fn sizes(scan: &Scan) -> BTreeMap<String, (Kind, u64, u64, u32)> {
    (0..scan.len() as u32)
        .filter_map(|i| {
            let n = scan.node(i)?;
            let dup = n.flags & crate::model::F_HARDLINK_DUP != 0;
            (n.kind != Kind::Dir && !dup).then(|| (scan.path(i).to_lowercase(), (n.kind, n.logical, n.allocated, n.files)))
        })
        .collect()
}

#[test]
fn parcours_sur_les_fixtures() {
    let d = fixtures();
    // Fichier ouvert en exclusivité : listé quand même (rien n'est ouvert), sans erreur.
    let _held = OpenOptions::new().read(true).share_mode(0).open(d.path().join("verrouille.log")).unwrap();
    let scan = scan_walk(d.path());
    let st = &scan.stats;
    assert_eq!(st.files, 10, "{:#?}", sizes(&scan));
    assert_eq!(st.hardlink_dups, 2);
    assert_eq!(st.links, 2);
    assert_eq!(st.cloud_files, 1);
    assert_eq!(st.cloud_bytes, 20_000);
    assert_eq!(st.errors(), 0, "{:?}", st.error_samples);

    let node = |rel: &str| scan.node(scan.find(rel).unwrap()).unwrap().clone();
    assert_eq!(node("verrouille.log").logical, 8192);
    assert_eq!(node("resident.txt").allocated, 0, "petit fichier résident");
    let sparse = node("clairseme.bin");
    assert_eq!(sparse.logical, 64 * 1024 * 1024);
    assert!(sparse.allocated < 1024 * 1024, "clairsemé : {}", sparse.allocated);
    let compressed = node("compresse.bin");
    assert!(compressed.allocated > 0 && compressed.allocated < compressed.logical, "compressé : {}", compressed.allocated);
    assert_eq!(node("un.txt").logical, 1, "flux nommé ignoré");
    let j = scan.find("jonction-windows").unwrap();
    assert_eq!(scan.node(j).unwrap().kind, Kind::Link);
    assert_eq!(scan.children(j).len(), 0, "jonction non traversée");
    assert_eq!(node("cloud.docx").kind, Kind::CloudFile);
    // File ID et date de modification enregistrés pour safety.
    let film = scan.find("Vidéos\\film é.mkv").unwrap();
    assert_ne!(scan.file_id(film), 0);
    assert!(scan.node(film).unwrap().modified > 0);
    assert_ne!(scan.volume_serial, 0);
}

#[test]
fn mft_identique_au_parcours() {
    if !is_elevated() {
        assert!(std::env::var("CI").is_err(), "la CI doit lancer ce test en administrateur");
        eprintln!("ignoré : test de la MFT à lancer en administrateur");
        return;
    }
    let d = fixtures();
    let walked = scan_walk(d.path());
    let read = scan_mft(&root_path(&s(d.path())), &AtomicBool::new(false), &|_: &Progress| {}).unwrap();
    let (a, b) = (&walked.stats, &read.stats);
    assert_eq!(sizes(&walked), sizes(&read), "détail fichier par fichier");
    assert_eq!((a.files, a.allocated, a.logical), (b.files, b.allocated, b.logical));
    assert_eq!((a.hardlink_dups, a.links, a.cloud_files, a.cloud_bytes), (b.hardlink_dups, b.links, b.cloud_files, b.cloud_bytes));
    // Mêmes identités : safety pourra comparer indifféremment.
    let film = "Vidéos\\film é.mkv";
    assert_eq!(walked.file_id(walked.find(film).unwrap()), read.file_id(read.find(film).unwrap()));
    assert_eq!(walked.volume_serial, read.volume_serial);
}

#[test]
fn dossier_interdit_compte_sans_bloquer() {
    let root = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let src = WinSource::new(&format!("{root}\\"));
    let mut out = Vec::new();
    assert_eq!(src.read_dir(&format!("{root}\\System Volume Information"), &mut out), Err(DirError::AccessDenied));
}

#[test]
fn liste_de_dossier_perimee_corrigee() {
    // NTFS ne met à jour la copie des tailles que dans l'entrée du nom utilisé pour écrire :
    // l'autre nom d'un lien physique garde, dans la liste de son dossier, l'ancienne taille.
    let dir = tempfile::Builder::new().prefix("lumen-perime-").tempdir().unwrap();
    let r = dir.path();
    fs::create_dir_all(r.join("a")).unwrap();
    fs::create_dir_all(r.join("b")).unwrap();
    File::create(r.join("a\\donnees.bin")).unwrap().write_all(&[1u8; 4096]).unwrap();
    fs::hard_link(r.join("a\\donnees.bin"), r.join("b\\autre-nom.bin")).unwrap();
    let mut f = OpenOptions::new().append(true).open(r.join("a\\donnees.bin")).unwrap();
    f.write_all(&vec![2u8; 1024 * 1024]).unwrap();
    f.sync_all().unwrap();
    drop(f);
    let real = 4096 + 1024 * 1024;
    let listed = fs::read_dir(r.join("b")).unwrap().next().unwrap().unwrap().metadata().unwrap().len();
    eprintln!("taille dans la liste du dossier b : {listed} (réelle : {real})");

    let scan = scan_walk(r);
    for rel in ["a\\donnees.bin", "b\\autre-nom.bin"] {
        let n = scan.node(scan.find(rel).unwrap()).unwrap().clone();
        assert_eq!(n.logical, real, "{rel}");
        assert!(n.allocated >= real, "{rel} : {}", n.allocated);
    }
    assert_eq!(scan.stats.files, 1);
    assert_eq!(scan.stats.hardlink_dups, 1);
    assert_eq!(scan.stats.logical, real);
}

/// 1er janvier 2020, en FILETIME : une date de dernier accès « dormante ».
const DORMANT: u64 = 132_223_104_000_000_000;

fn last_access(p: &Path) -> u64 {
    use std::os::windows::fs::MetadataExt;
    // Métadonnées par handle, sans lire le contenu ni lister le dossier.
    fs::metadata(p).unwrap().last_access_time()
}

fn set_last_access(p: &Path, value: u64) {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::Storage::FileSystem::{SetFileTime, FILE_FLAG_BACKUP_SEMANTICS, FILE_WRITE_ATTRIBUTES};
    let f = OpenOptions::new().access_mode(FILE_WRITE_ATTRIBUTES.0).custom_flags(FILE_FLAG_BACKUP_SEMANTICS.0).open(p).unwrap();
    let t = FILETIME { dwLowDateTime: value as u32, dwHighDateTime: (value >> 32) as u32 };
    // SAFETY : handle valide ouvert avec FILE_WRITE_ATTRIBUTES.
    unsafe { SetFileTime(HANDLE(f.as_raw_handle()), None, Some(&t), None) }.unwrap();
}

#[test]
fn scan_sans_toucher_au_dernier_acces() {
    // Volume où Windows tient la date de dernier accès à jour (la CI en monte un), sinon %TEMP%.
    let base = std::env::var("LUMEN_TEST_NTFS").ok();
    let d = match &base {
        Some(v) => tempfile::Builder::new().prefix("lumen-acces-").tempdir_in(v).unwrap(),
        None => tempfile::Builder::new().prefix("lumen-acces-").tempdir().unwrap(),
    };
    let r = d.path();
    fs::create_dir_all(r.join("dormant\\sous-dossier")).unwrap();
    fs::create_dir_all(r.join("temoin-dossier")).unwrap();
    File::create(r.join("dormant\\vieux.txt")).unwrap().write_all(&[4u8; 9000]).unwrap();
    File::create(r.join("dormant\\sous-dossier\\petit.txt")).unwrap().write_all(b"dormant").unwrap();
    File::create(r.join("temoin.txt")).unwrap().write_all(&[1u8; 9000]).unwrap();
    File::create(r.join("temoin-dossier\\x.txt")).unwrap().write_all(b"x").unwrap();
    let watched = ["dormant", "dormant\\vieux.txt", "dormant\\sous-dossier", "dormant\\sous-dossier\\petit.txt"];
    for p in watched.iter().chain(&["temoin.txt", "temoin-dossier"]) {
        set_last_access(&r.join(p), DORMANT);
        assert_eq!(last_access(&r.join(p)), DORMANT);
    }

    // Témoins : lire un fichier et lister un dossier mettent bien la date à jour sur ce volume,
    // sinon le test ne prouverait rien.
    let _ = fs::read(r.join("temoin.txt")).unwrap();
    let _ = fs::read_dir(r.join("temoin-dossier")).unwrap().count();
    let file_tracked = last_access(&r.join("temoin.txt")) != DORMANT;
    let dir_tracked = last_access(&r.join("temoin-dossier")) != DORMANT;
    eprintln!("dernier accès tenu à jour : fichiers {file_tracked}, dossiers {dir_tracked} (volume {base:?})");
    if !file_tracked {
        assert!(std::env::var("CI").is_err(), "la CI doit fournir un volume où le dernier accès est tenu à jour (LUMEN_TEST_NTFS)");
        eprintln!("ignoré : Windows ne tient pas la date de dernier accès à jour sur ce volume");
        return;
    }

    let walked = scan_walk(r);
    for p in watched {
        assert_eq!(last_access(&r.join(p)), DORMANT, "parcours : dernier accès de {p} modifié");
    }
    // Le scan rapporte bien la date d'origine (base des fichiers dormants).
    assert_eq!(walked.node(walked.find("dormant\\vieux.txt").unwrap()).unwrap().accessed as u64, DORMANT);

    if is_elevated() {
        let read = scan_mft(&root_path(&s(r)), &AtomicBool::new(false), &|_: &Progress| {}).unwrap();
        for p in watched {
            assert_eq!(last_access(&r.join(p)), DORMANT, "MFT : dernier accès de {p} modifié");
        }
        assert_eq!(read.node(read.find("dormant\\vieux.txt").unwrap()).unwrap().accessed as u64, DORMANT);
    } else {
        assert!(std::env::var("CI").is_err(), "la CI doit lancer ce test en administrateur");
    }
    if !dir_tracked {
        eprintln!("note : lister un dossier ne met pas sa date à jour sur ce système ; seule la partie fichiers est probante");
    }
}
