//! Banc d'essai du scanner, lancé par la CI.
//!
//! ```text
//! cargo run --release -p scan --example bench -- generate <dossier> <nombre de fichiers>
//! cargo run --release -p scan --example bench -- walk <dossier>
//! cargo run --release -p scan --example bench -- mft <dossier>      (administrateur, Windows)
//! cargo run --release -p scan --example bench -- compare <dossier>  (administrateur, Windows)
//! ```
//! `compare` lance les deux méthodes dans le même processus et liste leurs différences.
//! Chaque mesure affiche une ligne Markdown : méthode, fichiers, taille allouée, durée, mémoire.

use std::fs::{self, File};
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use scan::{Progress, Scan};

fn gib(bytes: u64) -> String {
    format!("{:.2} Gio", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

fn mib(bytes: u64) -> String {
    format!("{:.0} Mio", bytes as f64 / (1024.0 * 1024.0))
}

fn generate(dir: &str, count: usize) -> std::io::Result<()> {
    let started = Instant::now();
    let per_dir = 500;
    for i in 0..count {
        let sub = format!("{dir}/{:03}/{:04}", i / (per_dir * 100), i / per_dir);
        if i % per_dir == 0 {
            fs::create_dir_all(&sub)?;
        }
        // Tailles variées : petits fichiers résidents, quelques Kio, parfois plus.
        let size = match i % 10 {
            0 => 0,
            1..=5 => 300,
            6..=8 => 4096 + i % 9000,
            _ => 70_000,
        };
        File::create(format!("{sub}/fichier-{i}.dat"))?.write_all(&vec![1u8; size])?;
    }
    println!("| génération | {count} fichiers | | {:.1} s | |", started.elapsed().as_secs_f64());
    Ok(())
}

fn report(label: &str, scan: &Scan, seconds: f64) {
    let memory = memory();
    println!(
        "| {label} | {} fichiers, {} dossiers | {} | {seconds:.2} s | {memory} |",
        scan.stats.files,
        scan.stats.dirs,
        gib(scan.stats.allocated)
    );
    println!(
        "<!-- détail : liens {} · doublons physiques {} · cloud {} · erreurs {} · entrées {} -->",
        scan.stats.links,
        scan.stats.hardlink_dups,
        scan.stats.cloud_files,
        scan.stats.errors(),
        scan.len()
    );
}

#[cfg(windows)]
fn memory() -> String {
    let (peak, current) = scan::win::memory_usage();
    format!("pic {} (actuel {})", mib(peak), mib(current))
}

#[cfg(not(windows))]
fn memory() -> String {
    let _ = mib;
    "—".into()
}

fn run_walk(dir: &str) -> Result<(), String> {
    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    #[cfg(windows)]
    let (root, source) = {
        let root = scan::win::root_path(dir);
        let source = scan::win::WinSource::new(&root);
        (root, source)
    };
    #[cfg(unix)]
    let (root, source) = (dir.to_owned(), scan::unix::StdSource);
    let scan = scan::walk(&source, &root, &cancel, &|_: &Progress| {}).map_err(|e| e.to_string())?;
    report("parcours", &scan, started.elapsed().as_secs_f64());
    #[cfg(windows)]
    {
        let (count, ms) = scan::win::compressed_lookups();
        println!("<!-- GetCompressedFileSizeW : {count} appels, {ms} ms cumulés sur tous les fils -->");
    }
    Ok(())
}

#[cfg(windows)]
fn run_mft(dir: &str) -> Result<(), String> {
    let cancel = AtomicBool::new(false);
    let started = Instant::now();
    let scan = scan::win::mft::scan_mft(dir, &cancel, &|_: &Progress| {}).map_err(|e| e.to_string())?;
    report("MFT", &scan, started.elapsed().as_secs_f64());
    Ok(())
}

/// Fichiers comptés (hors dossiers et doublons physiques) : chemin → (taille allouée, attributs).
#[cfg(windows)]
fn files_of(scan: &Scan) -> std::collections::HashMap<String, (u64, u32, scan::Kind)> {
    (0..scan.len() as u32)
        .filter_map(|i| {
            let n = scan.node(i)?;
            let dup = n.flags & scan::model::F_HARDLINK_DUP != 0;
            (n.kind != scan::Kind::Dir && !dup).then(|| (scan.path(i).to_lowercase(), (n.allocated, n.attributes, n.kind)))
        })
        .collect()
}

#[cfg(windows)]
fn run_compare(dir: &str) -> Result<(), String> {
    let cancel = AtomicBool::new(false);
    let root = scan::win::root_path(dir);
    let started = Instant::now();
    let walked = scan::walk(&scan::win::WinSource::new(&root), &root, &cancel, &|_: &Progress| {}).map_err(|e| e.to_string())?;
    let walk_s = started.elapsed().as_secs_f64();
    let (count, ms) = scan::win::compressed_lookups();
    let started = Instant::now();
    let read = scan::win::mft::scan_mft(dir, &cancel, &|_: &Progress| {}).map_err(|e| e.to_string())?;
    let mft_s = started.elapsed().as_secs_f64();
    println!("parcours {walk_s:.1} s ({} ; GetCompressedFileSizeW : {count} appels, {ms} ms cumulés), MFT {mft_s:.1} s ({})",
        gib(walked.stats.allocated), gib(read.stats.allocated));
    println!("erreurs du parcours : {:?}", walked.stats.error_samples);

    let a = files_of(&walked);
    let b = files_of(&read);
    let mut only_mft: Vec<(&String, u64)> = b.iter().filter(|(k, _)| !a.contains_key(*k)).map(|(k, v)| (k, v.0)).collect();
    let mut only_walk: Vec<(&String, u64)> = a.iter().filter(|(k, _)| !b.contains_key(*k)).map(|(k, v)| (k, v.0)).collect();
    let mut differ: Vec<(&String, u64, u64, u32, scan::Kind)> = a
        .iter()
        .filter_map(|(k, v)| b.get(k).filter(|w| w.0 != v.0).map(|w| (k, v.0, w.0, v.1, v.2)))
        .collect();
    only_mft.sort_by_key(|e| std::cmp::Reverse(e.1));
    only_walk.sort_by_key(|e| std::cmp::Reverse(e.1));
    differ.sort_by_key(|e| std::cmp::Reverse(e.1.abs_diff(e.2)));
    let sum = |v: &[(&String, u64)]| v.iter().map(|e| e.1).sum::<u64>();
    println!("seulement dans la MFT : {} fichiers, {}", only_mft.len(), gib(sum(&only_mft)));
    for (k, v) in only_mft.iter().take(15) {
        println!("  {} {k}", mib(*v));
    }
    println!("seulement dans le parcours : {} fichiers, {}", only_walk.len(), gib(sum(&only_walk)));
    for (k, v) in only_walk.iter().take(15) {
        println!("  {} {k}", mib(*v));
    }
    let delta: i128 = differ.iter().map(|e| i128::from(e.2) - i128::from(e.1)).sum();
    println!("tailles différentes : {} fichiers, MFT − parcours = {:.2} Gio", differ.len(), delta as f64 / 1073741824.0);
    for (k, w, m, attrs, kind) in differ.iter().take(25) {
        println!("  parcours {} / MFT {} attributs {attrs:#x} {kind:?} {k}", mib(*w), mib(*m));
    }
    Ok(())
}

#[cfg(not(windows))]
fn run_compare(_dir: &str) -> Result<(), String> {
    Err("la MFT ne se lit que sous Windows".into())
}

#[cfg(not(windows))]
fn run_mft(_dir: &str) -> Result<(), String> {
    Err("la MFT ne se lit que sous Windows".into())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("generate") if args.len() == 4 => {
            let count = args[3].parse().unwrap_or(200_000);
            generate(&args[2], count).map_err(|e| e.to_string())
        }
        Some("walk") if args.len() == 3 => run_walk(&args[2]),
        Some("mft") if args.len() == 3 => run_mft(&args[2]),
        Some("compare") if args.len() == 3 => run_compare(&args[2]),
        _ => Err("usage : bench generate <dossier> <n> | walk <dossier> | mft <dossier> | compare <dossier>".into()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
