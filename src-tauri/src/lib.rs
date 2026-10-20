//! Lumen — backend Tauri.
//!
//! Jalon 3 : analyse du disque (crate `scan`) exposée par des commandes typées ; `safety` reste
//! en simulation seule. Les modules suivants arrivent jalon par jalon : rules/ et reco/ (4)…

mod error;
mod scanning;

pub use error::LumenError;

/// Démarrage en assistant élevé (lecture de la MFT) : renvoie le code de sortie, ou `None` pour
/// lancer l'interface normalement.
pub fn helper_entry() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    #[cfg(windows)]
    if args.get(1).map(String::as_str) == Some(scan::win::helper::HELPER_ARG) {
        return Some(scan::win::helper::run_helper(&args));
    }
    #[cfg(not(windows))]
    let _ = args;
    None
}

pub fn run() {
    let result = tauri::Builder::default()
        .manage(scanning::ScanState::default())
        .invoke_handler(tauri::generate_handler![
            scanning::app_info,
            scanning::volumes_list,
            scanning::scan_start,
            scanning::scan_cancel,
            scanning::scan_summary,
            scanning::scan_tree,
        ])
        .run(tauri::generate_context!());
    if let Err(err) = result {
        eprintln!("{}", LumenError::Startup(err.to_string()));
        std::process::exit(1);
    }
}
