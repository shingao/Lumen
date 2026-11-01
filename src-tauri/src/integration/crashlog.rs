//! Journal de plantage local : `%LOCALAPPDATA%\Lumen\logs`, jamais envoyé (Lumen n'a aucun accès
//! réseau). Un plantage Rust écrit `crash-<n>.log` (5 au plus : le plus ancien est réécrit, rien
//! n'est supprimé) ; les erreurs de l'interface s'ajoutent à `interface.log`, remis à zéro
//! au-delà de 256 Kio.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CRASH_SLOTS: u8 = 5;
const FRONT_LOG: &str = "interface.log";
const FRONT_LOG_MAX: u64 = 256 * 1024;
/// Un message de l'interface au-delà est tronqué.
const MESSAGE_MAX: usize = 4_000;

pub fn crash_file(dir: &Path, slot: u8) -> PathBuf {
    dir.join(format!("crash-{slot}.log"))
}

/// Emplacement à écrire : le premier libre, sinon le plus ancien (`modified` : date de chaque
/// emplacement, `None` s'il n'existe pas).
pub fn pick_slot(modified: &[Option<SystemTime>]) -> u8 {
    if let Some(i) = modified.iter().position(Option::is_none) {
        return i as u8 + 1;
    }
    modified.iter().enumerate().min_by_key(|(_, t)| **t).map_or(1, |(i, _)| i as u8 + 1)
}

fn stamp() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format!("{secs} (secondes depuis 1970, UTC)")
}

/// Écrit le rapport d'un plantage dans l'emplacement libre ou le plus ancien.
pub fn write_crash(dir: &Path, report: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let modified: Vec<Option<SystemTime>> =
        (1..=CRASH_SLOTS).map(|s| std::fs::metadata(crash_file(dir, s)).and_then(|m| m.modified()).ok()).collect();
    let path = crash_file(dir, pick_slot(&modified));
    let mut f = File::create(&path)?;
    writeln!(f, "Lumen {} — plantage", env!("CARGO_PKG_VERSION"))?;
    writeln!(f, "Date : {}", stamp())?;
    writeln!(f, "Système : {} {}", std::env::consts::OS, std::env::consts::ARCH)?;
    writeln!(f)?;
    writeln!(f, "{report}")?;
    Ok(path)
}

/// Erreur de l'interface (exception non rattrapée, promesse rejetée).
pub fn append_front(dir: &Path, message: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(FRONT_LOG);
    let full = std::fs::metadata(&path).map(|m| m.len() >= FRONT_LOG_MAX).unwrap_or(false);
    let mut f = if full { File::create(&path)? } else { OpenOptions::new().create(true).append(true).open(&path)? };
    let message: String = message.chars().filter(|c| *c != '\r').take(MESSAGE_MAX).collect();
    writeln!(f, "[{}] {message}", stamp())
}

/// Plantage Rust : rapport écrit avant l'arrêt (`panic = "abort"` en release), puis le
/// comportement par défaut.
pub fn install_panic_hook(dir: PathBuf) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let message = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_default();
        let thread = std::thread::current().name().unwrap_or("?").to_owned();
        let backtrace = std::backtrace::Backtrace::force_capture();
        let _ = write_crash(&dir, &format!("Fil : {thread}\nEmplacement : {location}\nMessage : {message}\n\n{backtrace}"));
        default(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn cinq_emplacements_au_plus_le_plus_ancien_reecrit() {
        let t = |s: u64| Some(UNIX_EPOCH + Duration::from_secs(s));
        assert_eq!(pick_slot(&[None, None, None, None, None]), 1);
        assert_eq!(pick_slot(&[t(5), None, t(1), None, None]), 2);
        assert_eq!(pick_slot(&[t(5), t(4), t(1), t(9), t(7)]), 3);

        let dir = tempfile::tempdir().unwrap();
        for i in 0..7 {
            write_crash(dir.path(), &format!("plantage {i}")).unwrap();
            std::thread::sleep(Duration::from_millis(20));
        }
        let files: Vec<_> = std::fs::read_dir(dir.path()).unwrap().collect();
        assert_eq!(files.len(), 5);
        let all: String = (1..=5).map(|s| std::fs::read_to_string(crash_file(dir.path(), s)).unwrap()).collect();
        assert!(all.contains("plantage 6") && all.contains("plantage 2") && !all.contains("plantage 1\n") && !all.contains("plantage 0"));
    }

    #[test]
    fn erreurs_de_l_interface_bornees() {
        let dir = tempfile::tempdir().unwrap();
        append_front(dir.path(), "TypeError: x is undefined\r\n  at f").unwrap();
        let text = std::fs::read_to_string(dir.path().join(FRONT_LOG)).unwrap();
        assert!(text.contains("TypeError: x is undefined\n  at f") && !text.contains('\r'));
        let long = "é".repeat(10_000);
        for _ in 0..80 {
            append_front(dir.path(), &long).unwrap();
        }
        assert!(std::fs::metadata(dir.path().join(FRONT_LOG)).unwrap().len() < FRONT_LOG_MAX + 10_000);
    }
}
