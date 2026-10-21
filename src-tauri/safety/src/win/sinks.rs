//! Gouffres système : ce que Lumen peut en savoir sans droits d'administrateur (tailles des
//! fichiers système, réglages lus dans le registre). Rien n'est modifié ici.

use std::ffi::c_void;

use windows::core::HSTRING;
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Storage::FileSystem::{GetFileAttributesExW, GetFileExInfoStandard, FILE_ATTRIBUTE_DIRECTORY, WIN32_FILE_ATTRIBUTE_DATA};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD};

use crate::path::WinPath;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SinkFacts {
    /// Lettre du lecteur système.
    pub system_drive: char,
    /// Taille de `hiberfil.sys` (absent : hibernation désactivée).
    pub hiberfil: Option<u64>,
    pub pagefile: Option<u64>,
    pub swapfile: Option<u64>,
    /// `HibernateEnabled` (Power).
    pub hibernate_enabled: Option<bool>,
    /// Démarrage rapide (`HiberbootEnabled`) : il repose sur l'hibernation.
    pub fast_startup: Option<bool>,
    pub windows_old: bool,
    /// Fréquence minimale entre deux points de restauration (minutes, 1440 par défaut).
    pub restore_point_frequency_min: u32,
    /// Valeur brute de `NtfsDisableLastAccessUpdate` (ce qu'affiche `fsutil behavior query
    /// disablelastaccess`), et si la date de dernier accès est tenue à jour.
    pub last_access_raw: Option<u32>,
    pub last_access_updated: bool,
}

fn dword(key: &str, value: &str) -> Option<u32> {
    let mut v = 0u32;
    let mut size = 4u32;
    // SAFETY : chaînes et tampon possédés.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from(key),
            &HSTRING::from(value),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    (status == ERROR_SUCCESS).then_some(v)
}

/// Taille d'un fichier (sans l'ouvrir : fichiers système en cours d'utilisation compris).
fn size_of_file(path: &str) -> Option<u64> {
    let mut data = WIN32_FILE_ATTRIBUTE_DATA::default();
    // SAFETY : chaîne et structure possédées, de la taille attendue par la classe demandée.
    unsafe { GetFileAttributesExW(&HSTRING::from(path), GetFileExInfoStandard, &mut data as *mut _ as *mut c_void) }.ok()?;
    (data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY.0 == 0).then(|| (u64::from(data.nFileSizeHigh) << 32) | u64::from(data.nFileSizeLow))
}

/// Date de dernier accès tenue à jour par Windows ? Bit 0 de `NtfsDisableLastAccessUpdate` à 1 :
/// désactivée (1, 3, 0x80000001, 0x80000003). Valeur absente : on ne s'y fie pas.
pub fn last_access_update() -> (Option<u32>, bool) {
    let raw = dword("SYSTEM\\CurrentControlSet\\Control\\FileSystem", "NtfsDisableLastAccessUpdate");
    (raw, raw.is_some_and(|v| v & 1 == 0))
}

pub fn system_sinks() -> SinkFacts {
    let drive = super::system_layout()
        .ok()
        .and_then(|l| match l.windows.prefix() {
            crate::path::Prefix::Drive(d) => Some(*d),
            crate::path::Prefix::Unc { .. } => None,
        })
        .unwrap_or('C');
    let root = format!("{drive}:\\");
    let windows_old = WinPath::parse(&format!("{root}Windows.old")).ok().is_some_and(|p| {
        use crate::probe::Probe;
        super::WinProbe.attributes(&p).is_ok_and(|a| a.directory)
    });
    let (last_access_raw, last_access_updated) = last_access_update();
    SinkFacts {
        system_drive: drive,
        hiberfil: size_of_file(&format!("{root}hiberfil.sys")),
        pagefile: size_of_file(&format!("{root}pagefile.sys")),
        swapfile: size_of_file(&format!("{root}swapfile.sys")),
        hibernate_enabled: dword("SYSTEM\\CurrentControlSet\\Control\\Power", "HibernateEnabled").map(|v| v != 0),
        fast_startup: dword("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Power", "HiberbootEnabled").map(|v| v != 0),
        windows_old,
        restore_point_frequency_min: dword("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\SystemRestore", "SystemRestorePointCreationFrequency")
            .unwrap_or(1440),
        last_access_raw,
        last_access_updated,
    }
}
