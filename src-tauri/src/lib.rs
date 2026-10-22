//! Lumen — backend Tauri.
//!
//! Analyse du disque (crate `scan`), règles appliquées (crate `rules` : statuts,
//! recommandations, KPI), nettoyage par `safety` (plan, confirmation, recontrôle complet,
//! exécution) et journal SQLite.

mod advanced;
mod analysis;
mod cleanup;
#[cfg(test)]
mod e2e;
mod dev;
mod error;
mod games;
mod journal;
mod scanning;
mod timeline;

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
    advanced::ipc::mark_session_start();
    let result = tauri::Builder::default()
        .manage(scanning::ScanState::default())
        .manage(cleanup::CleanState::default())
        .manage(cleanup::JournalState::default())
        .manage(advanced::ipc::AdvancedState::default())
        .manage(timeline::DailyState::default())
        .manage(games::GamesState::default())
        .manage(dev::DevState::default())
        .invoke_handler(tauri::generate_handler![
            scanning::app_info,
            scanning::volumes_list,
            scanning::scan_start,
            scanning::scan_cancel,
            scanning::scan_summary,
            scanning::scan_tree,
            scanning::scan_find,
            scanning::scan_cached,
            scanning::scan_cache_info,
            scanning::recommendations,
            scanning::recommendation_detail,
            scanning::rule_kpis,
            cleanup::plan_create,
            cleanup::plan_script,
            cleanup::plan_execute,
            cleanup::plan_cancel,
            cleanup::journal_list,
            cleanup::journal_detail,
            cleanup::journal_script,
            cleanup::journal_restore,
            advanced::ipc::dormant_find,
            advanced::ipc::largest_files,
            advanced::ipc::duplicates_start,
            advanced::ipc::duplicates_cancel,
            advanced::ipc::system_sinks,
            advanced::ipc::advanced_plan,
            advanced::ipc::restore_points_read,
            advanced::ipc::open_system_protection,
            timeline::timeline_series,
            timeline::timeline_data,
            timeline::timeline_trend,
            timeline::timeline_compare,
            timeline::timeline_database,
            timeline::timeline_purge,
            timeline::timeline_daily,
            games::games_list,
            games::game_cover,
            games::game_open,
            games::games_running,
            games::games_shaders,
            dev::dev_overview,
            dev::docker_images,
            dev::wsl_disks,
        ])
        .run(tauri::generate_context!());
    if let Err(err) = result {
        eprintln!("{}", LumenError::Startup(err.to_string()));
        std::process::exit(1);
    }
}
