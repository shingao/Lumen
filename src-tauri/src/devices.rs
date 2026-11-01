//! Périphériques (jalon 12) : type de chaque volume (interne, USB, carte SD, réseau), détection à
//! chaud des branchements et débranchements, arrêt propre d'une analyse dont le support disparaît.
//!
//! Sondage (équivalent de `WM_DEVICECHANGE`, sans fenêtre de messages ni crochet) : toutes les
//! 1,5 s, **seulement** `GetLogicalDrives` (masque des lettres, appel immédiat qui ne touche aucun
//! lecteur), et `GetDriveType` pour une lettre qui vient d'apparaître (gestionnaire de montage,
//! sans toucher au volume). Le nom, le système de fichiers, l'espace et le bus ne sont lus qu'à
//! l'apparition d'une lettre (dans un fil à part, jamais par la boucle) ou sur demande (analyse
//! du volume, infobulle et alertes si elles sont activées, disques internes seulement). Un lecteur
//! réseau n'est jamais lu par le sondage : seulement quand l'utilisateur l'analyse. Toutes les
//! commandes servent la liste depuis ce cache : une lecture lente ou bloquée (lecteur réseau, clé
//! qui se monte, disque en veille) ne bloque jamais l'interface.
//!
//! Sur une clé USB ou une carte SD, Windows supprime sans Corbeille (`safety::bin` refuse donc la
//! Corbeille) : Lumen n'y propose aucun nettoyage, seulement l'analyse et l'export. Un lecteur
//! réseau non plus. Les téléphones en MTP n'ont pas de lettre de lecteur : ils ne sont pas pris en
//! charge.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use crate::error::LumenError;
use crate::scanning::{self, ScanState, VolumeDto};
use crate::timeline::store::VolumeIdentity;

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
#[cfg_attr(not(windows), allow(dead_code))] // lu sous Windows (et dans les tests)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bus {
    Usb,
    Sd,
    Other,
    Unknown,
}

/// Type affiché : `fixed` (interne), `usb`, `sd`, `removable` (autre amovible), `network`,
/// `other` (lecteur optique, disque en mémoire…). Un disque USB qui se déclare fixe reste `usb`.
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

/// Garde des plans de nettoyage (sous Windows ; ailleurs, aucun volume réel). Lit le cache :
/// un volume dont les détails ne sont pas encore lus n'est pas nettoyable.
pub fn ensure_cleanable(letter: &str) -> Result<(), LumenError> {
    if !cfg!(windows) {
        return Ok(());
    }
    check_cleanable(&DEVICES.kinds(), letter)
}

fn chars(mask: u32) -> Vec<char> {
    ('A'..='Z').enumerate().filter(|(i, _)| mask & (1 << i) != 0).map(|(_, l)| l).collect()
}

/// Ce qu'on lit des lecteurs. Le sondage n'appelle que `mask` à chaque tour et `drive_type` pour
/// une lettre apparue ; `details` et `space` touchent le volume (lents, parfois bloquants).
pub trait DriveSource: Send + Sync {
    /// `GetLogicalDrives`.
    fn mask(&self) -> u32;
    /// `GetDriveType` (`None` : lettre sans racine).
    fn drive_type(&self, letter: char) -> Option<Drive>;
    /// Nom, système de fichiers, espace, identité (GUID, numéro de série) et bus.
    fn details(&self, letter: char, drive: Drive) -> Option<Details>;
    /// Espace seul (total, libre).
    fn space(&self, letter: char) -> Option<(u64, u64)>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Details {
    pub label: String,
    pub file_system: String,
    pub total: u64,
    pub free: u64,
    pub guid: Option<String>,
    pub serial: u32,
    pub bus: Bus,
    /// NTFS local lisible par l'assistant MFT.
    pub mft_capable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Read {
    /// Pas encore lu (lettre apparue, lecture en cours) ; pour un lecteur réseau : jamais lu
    /// tant que l'utilisateur ne l'analyse pas.
    Pending,
    Ready(Details),
    /// Lecture impossible (lecteur de cartes vide, support pas prêt) : pas de puce.
    Missing,
}

#[derive(Debug, Clone)]
struct Slot {
    drive: Drive,
    read: Read,
}

#[derive(Debug, Default)]
struct DevState {
    mask: Option<u32>,
    slots: BTreeMap<char, Slot>,
}

/// Changement de la liste des lettres : apparues (avec leur type) et disparues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub appeared: Vec<(char, Drive)>,
    pub gone: Vec<char>,
}

impl Change {
    /// Lettres dont les détails se lisent à l'apparition : jamais un lecteur réseau.
    pub fn to_read(&self) -> Vec<char> {
        self.appeared.iter().filter(|(_, d)| *d != Drive::Network).map(|(l, _)| *l).collect()
    }
}

/// Cache des volumes : seule source des commandes (aucune lecture de lecteur en les servant).
/// Le verrou n'est jamais tenu pendant une lecture de lecteur.
pub struct Devices(Mutex<DevState>);

pub static DEVICES: Devices = Devices::new();

fn first_char(letter: &str) -> Option<char> {
    letter.chars().next().map(|c| c.to_ascii_uppercase())
}

impl Devices {
    pub const fn new() -> Devices {
        Devices(Mutex::new(DevState { mask: None, slots: BTreeMap::new() }))
    }

    fn lock(&self) -> MutexGuard<'_, DevState> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Un tour du sondage : `mask`, puis `drive_type` des seules lettres apparues. Rien d'autre.
    pub fn poll(&self, src: &dyn DriveSource) -> Option<Change> {
        let mask = src.mask();
        let (appeared, gone) = {
            let st = self.lock();
            if st.mask == Some(mask) {
                return None;
            }
            let old = st.mask.unwrap_or(0);
            (chars(mask & !old), chars(old & !mask))
        };
        let typed: Vec<(char, Option<Drive>)> = appeared.into_iter().map(|l| (l, src.drive_type(l))).collect();
        let mut st = self.lock();
        st.mask = Some(mask);
        for l in &gone {
            st.slots.remove(l);
        }
        let mut added = Vec::new();
        for (l, d) in typed {
            if let Some(drive) = d {
                st.slots.insert(l, Slot { drive, read: Read::Pending });
                added.push((l, drive));
            }
        }
        Some(Change { appeared: added, gone })
    }

    /// Détails des lettres apparues (hors réseau), lus hors du verrou.
    pub fn fill(&self, src: &dyn DriveSource, letters: &[char]) {
        for &l in letters {
            let drive = match self.lock().slots.get(&l) {
                Some(s) if s.drive != Drive::Network && s.read == Read::Pending => s.drive,
                _ => continue,
            };
            self.store(l, drive, src.details(l, drive));
        }
    }

    /// Sur demande (analyse de ce volume) : relit ses détails, lecteur réseau compris.
    pub fn refresh(&self, src: &dyn DriveSource, letter: &str) {
        let Some(l) = first_char(letter) else { return };
        let Some(drive) = self.lock().slots.get(&l).map(|s| s.drive) else { return };
        self.store(l, drive, src.details(l, drive));
    }

    fn store(&self, l: char, drive: Drive, details: Option<Details>) {
        let mut st = self.lock();
        if let Some(slot) = st.slots.get_mut(&l) {
            if slot.drive == drive {
                slot.read = details.map_or(Read::Missing, Read::Ready);
            }
        }
    }

    /// Infobulle et alertes (si l'utilisateur les a activées) : espace des disques internes
    /// déjà lus, jamais un lecteur réseau ni un support amovible.
    pub fn refresh_space(&self, src: &dyn DriveSource) {
        let internal: Vec<char> = self
            .lock()
            .slots
            .iter()
            .filter(|(_, s)| matches!(&s.read, Read::Ready(d) if kind(s.drive, d.bus) == "fixed"))
            .map(|(l, _)| *l)
            .collect();
        for l in internal {
            let Some((total, free)) = src.space(l) else { continue };
            if let Some(Slot { read: Read::Ready(d), .. }) = self.lock().slots.get_mut(&l) {
                d.total = total;
                d.free = free;
            }
        }
    }

    /// Liste servie aux commandes (aucune lecture de lecteur).
    pub fn list(&self) -> Vec<VolumeDto> {
        self.lock().slots.iter().filter_map(|(l, s)| dto(*l, s)).collect()
    }

    pub fn get(&self, letter: &str) -> Option<VolumeDto> {
        let l = first_char(letter)?;
        self.lock().slots.get(&l).and_then(|s| dto(l, s))
    }

    /// Lettres présentes.
    pub fn present(&self) -> Vec<String> {
        self.lock().slots.keys().map(|l| format!("{l}:")).collect()
    }

    /// Type de chaque lettre pour la garde des nettoyages (`pending` tant que rien n'est lu).
    pub fn kinds(&self) -> Vec<(String, &'static str)> {
        self.lock()
            .slots
            .iter()
            .filter_map(|(l, s)| match &s.read {
                Read::Ready(d) => Some((format!("{l}:"), kind(s.drive, d.bus))),
                Read::Pending => Some((format!("{l}:"), "pending")),
                Read::Missing => None,
            })
            .collect()
    }

    /// Identité, espace des volumes déjà lus (Timeline). Aucune lecture.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn identities(&self) -> Vec<(VolumeIdentity, u64, u64)> {
        self.lock()
            .slots
            .iter()
            .filter_map(|(l, s)| match &s.read {
                Read::Ready(d) => Some((VolumeIdentity::new(d.guid.as_deref(), d.serial, &d.file_system, &d.label, &format!("{l}:")), d.total, d.free)),
                _ => None,
            })
            .collect()
    }

    /// Identité d'une lettre ; un volume local pas encore lu (démarrage) est lu maintenant, un
    /// lecteur réseau jamais.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn identity(&self, src: Option<&dyn DriveSource>, letter: &str) -> Option<VolumeIdentity> {
        let l = first_char(letter)?;
        let pending = self.lock().slots.get(&l).is_some_and(|s| s.read == Read::Pending && s.drive != Drive::Network);
        if let (true, Some(src)) = (pending, src) {
            self.fill(src, &[l]);
        }
        let letter = format!("{l}:");
        self.identities().into_iter().find(|(i, _, _)| i.letter == letter).map(|(i, _, _)| i)
    }
}

fn dto(letter: char, slot: &Slot) -> Option<VolumeDto> {
    let l = format!("{letter}:");
    match &slot.read {
        Read::Missing => None,
        Read::Ready(d) => {
            let kind = kind(slot.drive, d.bus);
            Some(VolumeDto {
                key: VolumeIdentity::new(d.guid.as_deref(), d.serial, &d.file_system, &d.label, &l).key,
                letter: l,
                label: d.label.clone(),
                file_system: d.file_system.clone(),
                kind,
                total: d.total,
                free: d.free,
                // MFT : disque interne NTFS seulement (jamais un disque USB qui se dit fixe).
                mft_capable: d.mft_capable && kind == "fixed",
                cleanable: cleanable(kind),
                analyzed_at: None,
                ready: true,
            })
        }
        Read::Pending => Some(VolumeDto {
            key: format!("letter:{l}"),
            letter: l,
            label: String::new(),
            file_system: String::new(),
            kind: match slot.drive {
                Drive::Network => "network",
                Drive::Fixed => "fixed",
                Drive::Removable => "removable",
                Drive::Other => "other",
            },
            total: 0,
            free: 0,
            mft_capable: false,
            cleanable: false,
            analyzed_at: None,
            ready: false,
        }),
    }
}

/// Source des lecteurs de la machine (`None` hors Windows).
pub fn system() -> Option<&'static dyn DriveSource> {
    platform::system()
}

/// Premier tour du sondage au démarrage (masque et types seulement, immédiat : l'Explorateur
/// peut valider un chemin du menu contextuel tout de suite), puis lecture des détails dans un fil
/// à part et boucle de détection à chaud.
pub fn start(app: AppHandle) {
    let Some(src) = system() else { return };
    if let Some(change) = DEVICES.poll(src) {
        read_in_background(app.clone(), src, change.to_read());
    }
    let _ = std::thread::Builder::new().name("lumen-devices".into()).spawn(move || loop {
        std::thread::sleep(POLL);
        let Some(change) = DEVICES.poll(src) else { continue };
        let state = app.state::<ScanState>();
        scanning::volumes_changed(&state, &DEVICES.present());
        // Liste tout de suite (nouvelle lettre « lecture… »), détails ensuite, hors de la boucle :
        // une clé lente à se monter ne retarde jamais la détection suivante.
        let _ = app.emit("volumes:changed", scanning::volumes_with(&state));
        read_in_background(app.clone(), src, change.to_read());
    });
}

fn read_in_background(app: AppHandle, src: &'static dyn DriveSource, letters: Vec<char>) {
    if letters.is_empty() {
        return;
    }
    let _ = std::thread::Builder::new().name("lumen-devices-read".into()).spawn(move || {
        DEVICES.fill(src, &letters);
        let state = app.state::<ScanState>();
        let _ = app.emit("volumes:changed", scanning::volumes_with(&state));
    });
}

mod platform {
    use super::*;

    #[cfg(windows)]
    struct System;

    #[cfg(windows)]
    impl DriveSource for System {
        fn mask(&self) -> u32 {
            scan::win::drive_mask()
        }

        fn drive_type(&self, letter: char) -> Option<Drive> {
            Some(match scan::win::drive_type(letter)? {
                scan::win::VolumeKind::Fixed => Drive::Fixed,
                scan::win::VolumeKind::Removable => Drive::Removable,
                scan::win::VolumeKind::Network => Drive::Network,
                scan::win::VolumeKind::Other => Drive::Other,
            })
        }

        fn details(&self, letter: char, drive: Drive) -> Option<Details> {
            let kind = match drive {
                Drive::Fixed => scan::win::VolumeKind::Fixed,
                Drive::Removable => scan::win::VolumeKind::Removable,
                Drive::Network => scan::win::VolumeKind::Network,
                Drive::Other => scan::win::VolumeKind::Other,
            };
            let v = scan::win::volume(letter, kind)?;
            let bus = match v.bus {
                scan::win::Bus::Usb => Bus::Usb,
                scan::win::Bus::Sd => Bus::Sd,
                scan::win::Bus::Other => Bus::Other,
                scan::win::Bus::Unknown => Bus::Unknown,
            };
            let mft_capable = v.mft_capable();
            Some(Details { label: v.label, file_system: v.file_system, total: v.total, free: v.free, guid: v.guid, serial: v.serial, bus, mft_capable })
        }

        fn space(&self, letter: char) -> Option<(u64, u64)> {
            scan::win::space(letter)
        }
    }

    #[cfg(windows)]
    pub fn system() -> Option<&'static dyn DriveSource> {
        Some(&System)
    }

    #[cfg(not(windows))]
    pub fn system() -> Option<&'static dyn DriveSource> {
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

    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Condvar};
    use std::time::Instant;

    /// Lecteurs simulés : compte chaque appel par lettre ; `details` peut bloquer jusqu'à ce que
    /// le test le libère (lecteur réseau qui ne répond pas, clé lente, disque qui se réveille).
    #[derive(Default)]
    struct Fake {
        mask: AtomicU32,
        types: Mutex<HashMap<char, Drive>>,
        calls: Mutex<Vec<(&'static str, char)>>,
        gate: Arc<(Mutex<bool>, Condvar)>,
        block: Mutex<Vec<char>>,
    }

    impl Fake {
        fn plug(&self, l: char, d: Drive) {
            self.types.lock().unwrap().insert(l, d);
            self.mask.fetch_or(1 << (l as u32 - 'A' as u32), Ordering::SeqCst);
        }
        fn unplug(&self, l: char) {
            self.mask.fetch_and(!(1 << (l as u32 - 'A' as u32)), Ordering::SeqCst);
        }
        fn count(&self, what: &str, l: char) -> usize {
            self.calls.lock().unwrap().iter().filter(|(w, c)| *w == what && *c == l).count()
        }
        fn open(&self) {
            let (m, cv) = &*self.gate;
            *m.lock().unwrap() = true;
            cv.notify_all();
        }
    }

    impl DriveSource for Fake {
        fn mask(&self) -> u32 {
            self.mask.load(Ordering::SeqCst)
        }
        fn drive_type(&self, l: char) -> Option<Drive> {
            self.calls.lock().unwrap().push(("type", l));
            self.types.lock().unwrap().get(&l).copied()
        }
        fn details(&self, l: char, drive: Drive) -> Option<Details> {
            self.calls.lock().unwrap().push(("details", l));
            if self.block.lock().unwrap().contains(&l) {
                let (m, cv) = &*self.gate;
                let mut open = m.lock().unwrap();
                while !*open {
                    open = cv.wait(open).unwrap();
                }
            }
            let bus = if l == 'E' { Bus::Usb } else { Bus::Other };
            let _ = drive;
            Some(Details { label: format!("Disque {l}"), file_system: "NTFS".into(), total: 100, free: 40, guid: Some(format!("\\\\?\\Volume{{{l}}}\\")), serial: 1, bus, mft_capable: true })
        }
        fn space(&self, l: char) -> Option<(u64, u64)> {
            self.calls.lock().unwrap().push(("space", l));
            Some((100, 10))
        }
    }

    #[test]
    fn sondage_masque_et_type_seulement() {
        let src = Fake::default();
        let dev = Devices::new();
        src.plug('C', Drive::Fixed);
        src.plug('Z', Drive::Network);
        // Démarrage : tout « apparaît » ; détails lus pour C:, jamais pour le lecteur réseau.
        let change = dev.poll(&src).unwrap();
        assert_eq!(change.to_read(), ['C']);
        dev.fill(&src, &change.to_read());
        assert_eq!(src.count("details", 'Z'), 0);
        let z = dev.get("Z:").unwrap();
        assert!(!z.ready && z.kind == "network" && !z.cleanable);
        // 100 tours sans changement : ni type, ni détails, ni espace relus.
        let before = src.calls.lock().unwrap().len();
        for _ in 0..100 {
            assert_eq!(dev.poll(&src), None);
        }
        assert_eq!(src.calls.lock().unwrap().len(), before, "le sondage ne touche aucun lecteur");
        // Une clé branchée : son type, puis ses détails une seule fois ; débranchée : retirée.
        src.plug('E', Drive::Removable);
        let change = dev.poll(&src).unwrap();
        assert_eq!((change.appeared.clone(), change.gone.clone()), (vec![('E', Drive::Removable)], vec![]));
        dev.fill(&src, &change.to_read());
        assert_eq!((src.count("type", 'E'), src.count("details", 'E')), (1, 1));
        assert_eq!(dev.get("E:").map(|v| v.kind), Some("usb"));
        assert_eq!((src.count("type", 'C'), src.count("details", 'C')), (1, 1), "C: jamais relu");
        src.unplug('E');
        assert_eq!(dev.poll(&src).unwrap().gone, ['E']);
        assert!(dev.get("E:").is_none());
        // Infobulle et alertes : espace des seuls disques internes, jamais le réseau.
        dev.refresh_space(&src);
        assert_eq!((src.count("space", 'C'), src.count("space", 'Z')), (1, 0));
        assert_eq!(dev.get("C:").map(|v| v.free), Some(10));
        // Le lecteur réseau n'est lu que sur demande (son analyse).
        dev.refresh(&src, "Z:");
        assert!(dev.get("Z:").unwrap().ready);
        // Garde des nettoyages : un volume pas encore lu n'est pas nettoyable.
        src.plug('F', Drive::Fixed);
        let _ = dev.poll(&src);
        assert!(check_cleanable(&dev.kinds(), "F:").is_err());
        assert!(check_cleanable(&dev.kinds(), "C:").is_ok());
    }

    #[test]
    fn source_bloquante_l_interface_reste_reactive() {
        let src = Arc::new(Fake::default());
        let dev = Arc::new(Devices::new());
        src.plug('C', Drive::Fixed);
        dev.fill(&*src, &dev.poll(&*src).unwrap().to_read());
        // Une clé dont la lecture ne répond pas (bloquée jusqu'à libération).
        src.block.lock().unwrap().push('E');
        src.plug('E', Drive::Removable);
        let change = dev.poll(&*src).unwrap();
        let reader = {
            let (src, dev) = (src.clone(), dev.clone());
            std::thread::spawn(move || dev.fill(&*src, &change.to_read()))
        };
        while src.count("details", 'E') == 0 {
            std::thread::yield_now();
        }
        // Pendant ce temps : la liste se sert tout de suite (E: « lecture… »), le sondage continue
        // et voit une autre lettre, la garde des nettoyages répond.
        let t = Instant::now();
        for _ in 0..50 {
            let list = dev.list();
            assert!(list.iter().any(|v| v.letter == "E:" && !v.ready));
            assert!(check_cleanable(&dev.kinds(), "E:").is_err());
        }
        src.plug('G', Drive::Network);
        assert_eq!(dev.poll(&*src).unwrap().appeared, [('G', Drive::Network)]);
        assert!(t.elapsed() < Duration::from_millis(500), "jamais bloqué par la lecture en cours : {:?}", t.elapsed());
        assert!(!reader.is_finished());
        // La lecture se termine : E: est connue.
        src.open();
        reader.join().unwrap();
        assert!(dev.get("E:").unwrap().ready);
    }
}
