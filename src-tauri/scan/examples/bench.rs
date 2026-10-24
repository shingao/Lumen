//! Banc d'essai du scanner, lancé par la CI.
//!
//! ```text
//! cargo run --release -p scan --example bench -- generate <dossier> <nombre de fichiers>
//! cargo run --release -p scan --example bench -- walk <dossier>
//! cargo run --release -p scan --example bench -- mft <dossier>      (administrateur, Windows)
//! cargo run --release -p scan --example bench -- compare <dossier>  (administrateur, Windows)
//! cargo run --release -p scan --example bench -- search <nombre d'entrées>
//! ```
//! `search` construit une analyse synthétique en mémoire (noms réalistes), son index, puis mesure
//! des requêtes typiques ; il échoue si l'une dépasse 50 ms.
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

/// Analyse synthétique de `count` entrées : dossiers de 200 éléments, noms variés.
fn synthetic(count: usize) -> Scan {
    use scan::model::Builder;
    use scan::{Kind, Method, RawEntry, Stats};
    let words = ["rapport", "vacances", "été", "facture", "photo", "montage", "projet", "données", "sauvegarde", "mariage", "lumen", "cache", "image", "export", "réunion", "budget"];
    let exts = ["mp4", "jpg", "pdf", "docx", "txt", "zip", "png", "mkv", "xlsx", "rs", "js", "json"];
    let e = |name: String, kind: Kind, size: u64| RawEntry { name, kind, attributes: 0, file_id: 0, logical: size, allocated: size, modified: 0, accessed: 0 };
    let mut b = Builder::new(&e("C:".into(), Kind::Dir, 0));
    let dirs = count / 200 + 1;
    let top = b.add_children(0, &(0..dirs).map(|d| e(format!("{} {d}", words[d % words.len()]), Kind::Dir, 0)).collect::<Vec<_>>());
    let mut made = dirs;
    for d in 0..dirs {
        if made >= count {
            break;
        }
        let n = 200.min(count - made);
        let files: Vec<RawEntry> = (0..n)
            .map(|i| {
                let k = d * 200 + i;
                let name = format!("{} {} {}_{k}.{}", words[k % words.len()], words[(k / 7) % words.len()], 2000 + k % 27, exts[k % exts.len()]);
                e(name, Kind::File, ((k * 7919) % (64 << 20)) as u64)
            })
            .collect();
        b.add_children(top.start + d as u32, &files);
        made += n;
    }
    b.finish("C:\\".into(), 1, Method::Walk, true, Stats::default())
}

fn run_search(count: usize) -> Result<(), String> {
    use scan::search::{parse, search, Index};
    let started = Instant::now();
    let scan = synthetic(count);
    let built = Instant::now();
    let idx = Index::build(&scan);
    println!("| index | {} entrées | {} | {} ms (analyse synthétique : {} ms) |", scan.len(), mib(idx.memory_bytes()), built.elapsed().as_millis(), (built - started).as_millis());
    let mut worst = 0u128;
    for q in ["rapport", "vacances ete", "mrgjl", "ext:mp4 >1go", "type:video montage", ">40mo", "sauvegarde 2019", "zzzz"] {
        let query = parse(q);
        // Meilleure de 5 mesures (la première réchauffe les caches).
        let mut best = u128::MAX;
        let mut n = 0;
        for _ in 0..5 {
            let t = Instant::now();
            n = search(&scan, &idx, &query, 50).len();
            best = best.min(t.elapsed().as_micros());
        }
        worst = worst.max(best);
        println!("| `{q}` | {n} résultats | {:.1} ms |", best as f64 / 1000.0);
    }
    if worst > 50_000 {
        return Err(format!("recherche trop lente : {:.1} ms (objectif 50 ms)", worst as f64 / 1000.0));
    }
    Ok(())
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
        Some("search") if args.len() == 3 => run_search(args[2].parse().unwrap_or(1_000_000)),
        _ => Err("usage : bench generate <dossier> <n> | walk <dossier> | mft <dossier> | compare <dossier> | search <n>".into()),
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
