//! Lumen — backend Tauri.
//!
//! Jalon 1 : fenêtre et interface uniquement (données factices côté front).
//! Les modules arrivent jalon par jalon : safety/ (2), scan/ (3), rules/ et reco/ (4)…

mod error;

pub use error::LumenError;

pub fn run() {
    let result = tauri::Builder::default().run(tauri::generate_context!());
    if let Err(err) = result {
        eprintln!("{}", LumenError::Startup(err.to_string()));
        std::process::exit(1);
    }
}
