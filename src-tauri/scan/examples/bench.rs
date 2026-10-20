//! Banc d'essai du scanner, lancé par la CI.
//!
//! ```text
//! cargo run --release -p scan --example bench -- generate <dossier> <nombre de fichiers>
//! cargo run --release -p scan --example bench -- walk <dossier>
//! cargo run --release -p scan --example bench -- mft <dossier>      (administrateur, Windows)
//! ```
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
        let root = scan::win::long_path(dir);
        let source = scan::win::WinSource::new(&root);
        (root, source)
    };
    #[cfg(unix)]
    let (root, source) = (dir.to_owned(), scan::unix::StdSource);
    let scan = scan::walk(&source, &root, &cancel, &|_: &Progress| {}).map_err(|e| e.to_string())?;
    report("parcours", &scan, started.elapsed().as_secs_f64());
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
        _ => Err("usage : bench generate <dossier> <n> | walk <dossier> | mft <dossier>".into()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
