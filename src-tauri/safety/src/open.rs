//! Ouvrir un élément trouvé par la recherche (logique pure) : Lumen n'exécute jamais un
//! programme. Un fichier qui lancerait du code (exécutable, script, raccourci, installeur…) est
//! montré dans l'Explorateur au lieu d'être ouvert ; un dossier s'ouvre dans l'Explorateur.

use crate::path::WinPath;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAction {
    /// Ouvrir avec l'appli par défaut (un dossier : dans l'Explorateur).
    Open,
    /// Montrer dans l'Explorateur, l'élément sélectionné.
    Reveal,
}

/// Extensions qui lanceraient du code (casse ignorée).
const RUNNABLE: &[&str] = &[
    "EXE", "COM", "SCR", "PIF", "CPL", "MSC", "BAT", "CMD", "PS1", "PSM1", "PSD1", "VBS", "VBE", "JS", "JSE", "WSF", "WSH", "WS", "HTA", "MSI", "MSP",
    "MSIX", "MSIXBUNDLE", "APPX", "APPXBUNDLE", "APPINSTALLER", "APPLICATION", "APPREF-MS", "LNK", "URL", "SCF", "REG", "INF", "JAR", "PY", "PYW",
    "SH", "DLL", "SYS", "OCX", "CHM", "GADGET", "XLL", "PPKG", "DIAGCAB", "SETTINGCONTENT-MS", "LIBRARY-MS", "SEARCH-MS", "SEARCHCONNECTOR-MS",
];

/// Ce qui sera réellement fait pour une demande.
pub fn decide(path: &WinPath, is_dir: bool, requested: OpenAction) -> OpenAction {
    if requested == OpenAction::Reveal || is_dir {
        return requested;
    }
    let name = path.file_name().unwrap_or_default();
    // Sans extension, ou avec une extension qui exécute : montré, jamais lancé.
    match name.rsplit_once('.') {
        Some((_, ext)) if !ext.is_empty() && !RUNNABLE.iter().any(|r| r.eq_ignore_ascii_case(ext)) => OpenAction::Open,
        _ => OpenAction::Reveal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> WinPath {
        WinPath::parse(s).unwrap()
    }

    #[test]
    fn jamais_un_programme_lance() {
        for f in ["C:\\Users\\A\\Videos\\film.mp4", "D:\\Docs\\rapport.PDF", "C:\\x\\photo.jpeg"] {
            assert_eq!(decide(&p(f), false, OpenAction::Open), OpenAction::Open, "{f}");
        }
        for f in ["C:\\Users\\A\\Downloads\\setup.exe", "C:\\x\\run.BAT", "C:\\x\\a.ps1", "C:\\x\\raccourci.lnk", "C:\\x\\installeur.msi", "C:\\x\\script.vbs", "C:\\x\\sans-extension"] {
            assert_eq!(decide(&p(f), false, OpenAction::Open), OpenAction::Reveal, "{f}");
        }
        assert_eq!(decide(&p("C:\\Users\\A\\Projets"), true, OpenAction::Open), OpenAction::Open);
        assert_eq!(decide(&p("C:\\x\\film.mp4"), false, OpenAction::Reveal), OpenAction::Reveal);
    }
}
