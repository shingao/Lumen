// Pas de console sous Windows en release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // L'assistant élevé (« Scan rapide (admin) ») est le même exécutable, lancé avec un argument.
    if let Some(code) = lumen_lib::helper_entry() {
        std::process::exit(code);
    }
    lumen_lib::run()
}
