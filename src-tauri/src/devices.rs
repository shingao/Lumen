//! Périphériques (jalon 12) : type de chaque volume (interne, USB, carte SD, réseau), détection à
//! chaud des branchements et débranchements, arrêt propre d'une analyse dont le support disparaît.
//!
//! Détection : le masque des lettres (`GetLogicalDrives`) est relu toutes les 1,5 s ; c'est un
//! appel immédiat qui ne touche à aucun lecteur, donc un lecteur réseau lent ne bloque jamais la
//! boucle (équivalent de `WM_DEVICECHANGE`, sans fenêtre de messages ni crochet). La liste
//! complète n'est relue qu'à un changement du masque.
//!
//! Sur une clé USB ou une carte SD, Windows supprime sans Corbeille (`safety::bin` refuse donc la
//! Corbeille) : Lumen n'y propose aucun nettoyage, seulement l'analyse et l'export. Un lecteur
//! réseau non plus. Les téléphones en MTP n'ont pas de lettre de lecteur : ils ne sont pas pris en
//! charge.

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::error::LumenError;
use crate::scanning::{self, ScanState};

/// Intervalle de relecture du masque des lettres.
const POLL: Duration = Duration::from_millis(1500);

/// Type de lecteur annoncé par Windows (`GetDriveTypeW`).
#[cfg_attr(not(windows), allow(dead_code))] // construit sous Windows (et dans les tests)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drive {
    Fixed,
    Removable,
    Network,
    Other,
}

/// Bus du support (`IOCTL_STORAGE_QUERY_PROPERTY`).
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bus {
    Usb,
    Sd,
    Other,
    Unknown,
}

/// Type affiché : `fixed` (interne), `usb`, `sd`, `removable` (autre amovible), `network`,
/// `other` (lecteur optique, disque en mémoire…). Un disque USB qui se déclare fixe reste `usb`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn kind(drive: Drive, bus: Bus) -> &'static str {
    match (drive, bus) {
        (Drive::Network, _) => "network",
        (_, Bus::Sd) => "sd",
        (_, Bus::Usb) => "usb",
        (Drive::Fixed, _) => "fixed",
        (Drive::Removable, _) => "removable",
        (Drive::Other, _) => "other",
    }
}

/// Lumen ne propose de nettoyage que sur un disque interne : ailleurs, pas de Corbeille.
pub fn cleanable(kind: &str) -> bool {
    kind == "fixed"
}

pub const NO_CLEANUP: &str = "Pas de Corbeille sur ce support : Lumen ne propose aucun nettoyage ici";

/// Refus d'un plan de nettoyage sur un volume sans Corbeille (USB, carte SD, réseau…) ou absent.
pub fn check_cleanable(volumes: &[(String, &'static str)], letter: &str) -> Result<(), LumenError> {
    match volumes.iter().find(|(l, _)| l.eq_ignore_ascii_case(letter)) {
        Some((_, k)) if cleanable(k) => Ok(()),
        Some(_) => Err(LumenError::Plan(NO_CLEANUP.into())),
        None => Err(LumenError::Plan(format!("le support {letter} n'est plus branché : rien n'a été fait"))),
    }
}

/// Garde des plans de nettoyage (sous Windows ; ailleurs, aucun volume réel).
pub fn ensure_cleanable(letter: &str) -> Result<(), LumenError> {
    if !cfg!(windows) {
        return Ok(());
    }
    let volumes: Vec<(String, &'static str)> = scanning::volumes_list_raw().into_iter().map(|v| (v.letter, v.kind)).collect();
    check_cleanable(&volumes, letter)
}

/// Lettres d'un masque `GetLogicalDrives` (« C: », « E: »…).
pub fn letters(mask: u32) -> Vec<String> {
    ('A'..='Z').enumerate().filter(|(i, _)| mask & (1 << i) != 0).map(|(_, l)| format!("{l}:")).collect()
}

/// Lettres apparues et disparues entre deux masques ; `None` si rien n'a changé.
pub fn diff(old: u32, new: u32) -> Option<(Vec<String>, Vec<String>)> {
    (old != new).then(|| (letters(new & !old), letters(old & !new)))
}

/// Boucle de détection à chaud : à chaque changement, arrête proprement l'analyse d'un volume
/// disparu et envoie la nouvelle liste (`volumes:changed`).
pub fn watch(app: AppHandle) {
    let Some(mut last) = platform::mask() else { return };
    let _ = std::thread::Builder::new().name("lumen-devices".into()).spawn(move || loop {
        std::thread::sleep(POLL);
        let Some(mask) = platform::mask() else { return };
        let Some((_appeared, _gone)) = diff(last, mask) else { continue };
        last = mask;
        let state = app.state::<ScanState>();
        scanning::volumes_changed(&state, &letters(mask));
        let _ = app.emit("volumes:changed", scanning::volumes_with(&state));
    });
}

mod platform {
    #[cfg(windows)]
    pub fn mask() -> Option<u32> {
        Some(scan::win::drive_mask())
    }

    #[cfg(not(windows))]
    pub fn mask() -> Option<u32> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_de_support_et_nettoyage() {
        assert_eq!(kind(Drive::Fixed, Bus::Other), "fixed");
        // Disque USB qui se déclare fixe, clé USB, carte SD (lecteur intégré ou USB).
        assert_eq!(kind(Drive::Fixed, Bus::Usb), "usb");
        assert_eq!(kind(Drive::Removable, Bus::Usb), "usb");
        assert_eq!(kind(Drive::Removable, Bus::Sd), "sd");
        assert_eq!(kind(Drive::Fixed, Bus::Sd), "sd");
        assert_eq!(kind(Drive::Removable, Bus::Unknown), "removable");
        assert_eq!(kind(Drive::Network, Bus::Usb), "network");
        assert_eq!(kind(Drive::Other, Bus::Unknown), "other");
        for k in ["usb", "sd", "removable", "network", "other"] {
            assert!(!cleanable(k), "{k} : aucun nettoyage");
        }
        let vols = vec![("C:".to_owned(), "fixed"), ("E:".to_owned(), "usb"), ("F:".to_owned(), "sd"), ("Z:".to_owned(), "network")];
        assert!(check_cleanable(&vols, "c:").is_ok());
        for l in ["E:", "F:", "Z:"] {
            assert_eq!(check_cleanable(&vols, l).unwrap_err().to_string(), format!("plan de nettoyage : {NO_CLEANUP}"));
        }
        // Support débranché depuis l'analyse : refus, rien n'est fait.
        assert!(check_cleanable(&vols, "G:").unwrap_err().to_string().contains("n'est plus branché"));
    }

    #[test]
    fn branchement_et_debranchement_detectes() {
        let c = 1 << 2;
        let e = 1 << 4;
        assert_eq!(letters(c | e), ["C:", "E:"]);
        assert_eq!(diff(c, c), None);
        assert_eq!(diff(c, c | e), Some((vec!["E:".to_owned()], vec![])));
        assert_eq!(diff(c | e, c), Some((vec![], vec!["E:".to_owned()])));
        // Une clé remplacée par une autre à une autre lettre dans le même intervalle.
        assert_eq!(diff(c | e, c | (1 << 5)), Some((vec!["F:".to_owned()], vec!["E:".to_owned()])));
    }
}
