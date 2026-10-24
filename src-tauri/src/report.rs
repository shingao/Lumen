//! Export du rapport (jalon 9) : le front construit la page HTML autonome ou le CSV, Lumen
//! l'enregistre dans Téléchargements sous un nom neuf (jamais par-dessus un fichier existant)
//! puis peut le montrer dans l'Explorateur. Le front n'envoie aucun chemin : seulement le
//! format, l'horodatage du nom et le contenu.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

use crate::error::LumenError;

/// Taille maximale d'un rapport (le HTML embarque ses polices : quelques centaines de Kio).
const MAX_BYTES: usize = 64 << 20;

#[derive(Default)]
pub struct ReportState(Mutex<Option<PathBuf>>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedReportDto {
    pub path: String,
    pub name: String,
}

/// Nom du fichier : `Lumen-rapport-<horodatage>.<ext>`, horodatage limité aux chiffres et tirets.
pub(crate) fn file_name(format: &str, stamp: &str, n: u32) -> Result<String, LumenError> {
    let ext = match format {
        "html" => "html",
        "csv" => "csv",
        _ => return Err(LumenError::Report("format inconnu".into())),
    };
    if stamp.is_empty() || stamp.len() > 20 || !stamp.chars().all(|c| c.is_ascii_digit() || c == '-') {
        return Err(LumenError::Report("horodatage invalide".into()));
    }
    Ok(if n == 0 { format!("Lumen-rapport-{stamp}.{ext}") } else { format!("Lumen-rapport-{stamp}-{}.{ext}", n + 1) })
}

/// Écrit sous un nom libre du dossier (`-2`, `-3`… si besoin) ; jamais d'écrasement.
pub(crate) fn write_new(dir: &Path, format: &str, stamp: &str, content: &[u8]) -> Result<PathBuf, LumenError> {
    if content.len() > MAX_BYTES {
        return Err(LumenError::Report("rapport trop gros".into()));
    }
    for n in 0..100 {
        let path = dir.join(file_name(format, stamp, n)?);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                f.write_all(content).map_err(|e| LumenError::Report(e.to_string()))?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(LumenError::Report(e.to_string())),
        }
    }
    Err(LumenError::Report("trop de rapports du même nom".into()))
}

#[tauri::command]
pub fn report_save(state: State<'_, ReportState>, format: String, stamp: String, content: String) -> Result<SavedReportDto, LumenError> {
    let dir = platform::downloads().ok_or_else(|| LumenError::Report("dossier Téléchargements introuvable".into()))?;
    let path = write_new(&dir, &format, &stamp, content.as_bytes())?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if let Ok(mut last) = state.0.lock() {
        *last = Some(path.clone());
    }
    Ok(SavedReportDto { path: path.display().to_string(), name })
}

/// Montre le dernier rapport enregistré dans l'Explorateur (jamais un autre fichier).
#[tauri::command]
pub fn report_reveal(state: State<'_, ReportState>) -> Result<(), LumenError> {
    let path = state.0.lock().ok().and_then(|p| p.clone()).ok_or_else(|| LumenError::Report("aucun rapport enregistré".into()))?;
    platform::reveal(&path)
}

mod platform {
    use super::*;

    #[cfg(windows)]
    pub fn downloads() -> Option<PathBuf> {
        safety::win::rule_variables().into_iter().find(|(n, _)| *n == "DOWNLOADS").map(|(_, p)| PathBuf::from(p.display_string()))
    }

    #[cfg(not(windows))]
    pub fn downloads() -> Option<PathBuf> {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Downloads")).filter(|p| p.is_dir())
    }

    #[cfg(windows)]
    pub fn reveal(path: &Path) -> Result<(), LumenError> {
        let p = safety::path::WinPath::parse(&path.display().to_string()).map_err(|e| LumenError::Report(e.to_string()))?;
        safety::win::open_item(&p, false, safety::open::OpenAction::Reveal).map(|_| ()).map_err(|e| LumenError::Report(e.to_string()))
    }

    #[cfg(not(windows))]
    pub fn reveal(_: &Path) -> Result<(), LumenError> {
        Err(LumenError::Report("disponible sous Windows uniquement".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nom_et_jamais_d_ecrasement() {
        assert_eq!(file_name("html", "2026-10-24-1530", 0).unwrap(), "Lumen-rapport-2026-10-24-1530.html");
        assert_eq!(file_name("csv", "2026-10-24-1530", 1).unwrap(), "Lumen-rapport-2026-10-24-1530-2.csv");
        assert!(file_name("exe", "1", 0).is_err());
        assert!(file_name("html", "..\\..\\x", 0).is_err());
        assert!(file_name("html", "", 0).is_err());
        let dir = tempfile::tempdir().unwrap();
        let a = write_new(dir.path(), "html", "1", b"premier").unwrap();
        let b = write_new(dir.path(), "html", "1", b"second").unwrap();
        assert_ne!(a, b);
        assert_eq!(std::fs::read(&a).unwrap(), b"premier");
        assert_eq!(std::fs::read(&b).unwrap(), b"second");
        assert!(b.ends_with("Lumen-rapport-1-2.html"));
    }
}
