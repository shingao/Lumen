//! Volumes et profils, lus auprès de Windows : type de lecteur, SUBST, Corbeille du volume
//! (registre BitBucket et stratégies), liste des profils (registre ProfileList), chemins longs.

use std::ffi::c_void;

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Storage::FileSystem::{
    GetDriveTypeW, GetLongPathNameW, GetVolumeNameForVolumeMountPointW, GetVolumePathNameW, QueryDosDeviceW,
};
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ,
    RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};

use super::{from_utf16, last_error};
use crate::bin::{DriveKind, VolumeFacts};
use crate::path::{Prefix, WinPath};
use crate::probe::ProbeError;

const BITBUCKET: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\BitBucket\\Volume";
const POLICIES: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Policies\\Explorer";
const PROFILE_LIST: &str = "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\ProfileList";

fn reg_dword(root: HKEY, subkey: &str, value: &str) -> Option<u32> {
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY : chaînes et tampon possédés ; taille du tampon annoncée.
    let status = unsafe {
        RegGetValueW(
            root,
            &HSTRING::from(subkey),
            &HSTRING::from(value),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    (status == ERROR_SUCCESS).then_some(data)
}

/// Valeur texte, variables d'environnement développées (REG_EXPAND_SZ → REG_SZ).
fn reg_string(root: HKEY, subkey: &str, value: &str) -> Option<String> {
    let mut size: u32 = 0;
    let key = HSTRING::from(subkey);
    let name = HSTRING::from(value);
    // SAFETY : premier appel pour connaître la taille, sans tampon.
    let status = unsafe { RegGetValueW(root, &key, &name, RRF_RT_REG_SZ, None, None, Some(&mut size)) };
    if status != ERROR_SUCCESS || size == 0 {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    // SAFETY : tampon possédé de `size` octets.
    let status = unsafe {
        RegGetValueW(root, &key, &name, RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut c_void), Some(&mut size))
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16(&buf[..len]).ok()
}

/// Chemin long (noms 8.3 développés) d'un chemin existant ; inchangé si Windows ne peut le résoudre.
pub fn long_path(raw: &str) -> String {
    let name = HSTRING::from(raw);
    let mut buf = vec![0u16; 1024];
    // SAFETY : chaîne et tampon possédés.
    let n = unsafe { GetLongPathNameW(&name, Some(&mut buf)) } as usize;
    if n == 0 || n >= buf.len() {
        return raw.to_owned();
    }
    String::from_utf16(&buf[..n]).unwrap_or_else(|_| raw.to_owned())
}

/// Profils de tous les comptes de la machine (registre ProfileList), en chemins longs.
pub fn profile_list() -> Result<Vec<WinPath>, ProbeError> {
    let mut key = HKEY::default();
    // SAFETY : clé ouverte en lecture, fermée plus bas.
    let status = unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, &HSTRING::from(PROFILE_LIST), None, KEY_READ, &mut key) };
    if status != ERROR_SUCCESS {
        return Err(ProbeError::Os(status.0 as i32));
    }
    let mut profiles = Vec::new();
    let mut index = 0u32;
    loop {
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        // SAFETY : tampon possédé, longueur en caractères.
        let status = unsafe { RegEnumKeyExW(key, index, Some(PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None) };
        if status != ERROR_SUCCESS {
            break;
        }
        index += 1;
        let sid = String::from_utf16_lossy(&name[..len as usize]);
        if let Some(raw) = reg_string(key, &sid, "ProfileImagePath") {
            if let Ok(p) = WinPath::parse(&long_path(&raw)) {
                profiles.push(p);
            }
        }
    }
    // SAFETY : clé ouverte ci-dessus.
    unsafe {
        let _ = RegCloseKey(key);
    }
    Ok(profiles)
}

fn volume_root(path: &WinPath) -> Result<String, ProbeError> {
    let name = HSTRING::from(path.display_string());
    let mut buf = vec![0u16; 1024];
    // SAFETY : chaîne et tampon possédés.
    unsafe { GetVolumePathNameW(&name, &mut buf) }.map_err(|_| last_error())?;
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    from_utf16(&buf[..len])
}

fn is_subst(drive: char) -> bool {
    let mut buf = vec![0u16; 1024];
    // SAFETY : chaîne et tampon possédés.
    let n = unsafe { QueryDosDeviceW(&HSTRING::from(format!("{drive}:")), Some(&mut buf)) } as usize;
    // Un lecteur SUBST pointe vers un dossier (« \??\C:\… »), un vrai volume vers un périphérique.
    n > 0 && String::from_utf16_lossy(&buf[..n.min(buf.len())]).starts_with("\\??\\")
}

fn volume_guid(root: &str) -> Option<String> {
    let mut buf = vec![0u16; 64];
    // SAFETY : chaîne et tampon possédés.
    unsafe { GetVolumeNameForVolumeMountPointW(&HSTRING::from(root), &mut buf) }.ok()?;
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let name = String::from_utf16_lossy(&buf[..len]);
    let start = name.find('{')?;
    let end = name.find('}')?;
    Some(name[start..=end].to_owned())
}

/// Type de lecteur et état de la Corbeille pour le volume qui porte `path`.
pub fn volume_facts(path: &WinPath) -> Result<VolumeFacts, ProbeError> {
    let fixed = VolumeFacts { kind: DriveKind::Fixed, subst: false, recycle_disabled: false, max_bytes: None };
    let drive = match path.prefix() {
        Prefix::Unc { .. } => return Ok(VolumeFacts { kind: DriveKind::Remote, ..fixed }),
        Prefix::Drive(d) => *d,
    };
    if is_subst(drive) {
        return Ok(VolumeFacts { subst: true, ..fixed });
    }
    // Racine réelle du volume (un volume peut être monté dans un dossier d'un autre).
    let root = volume_root(path).unwrap_or_else(|_| format!("{drive}:\\"));
    // SAFETY : chaîne possédée.
    let kind = match unsafe { GetDriveTypeW(&HSTRING::from(root.as_str())) } {
        2 => DriveKind::Removable,
        3 => DriveKind::Fixed,
        4 => DriveKind::Remote,
        5 => DriveKind::CdRom,
        6 => DriveKind::RamDisk,
        _ => DriveKind::Unknown,
    };
    let policy = [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE].iter().any(|&k| reg_dword(k, POLICIES, "NoRecycleFiles") == Some(1));
    let (nuke, max_bytes) = match volume_guid(&root) {
        Some(guid) => {
            let key = format!("{BITBUCKET}\\{guid}");
            (
                reg_dword(HKEY_CURRENT_USER, &key, "NukeOnDelete") == Some(1),
                reg_dword(HKEY_CURRENT_USER, &key, "MaxCapacity").map(|mb| u64::from(mb) * 1024 * 1024),
            )
        }
        // Volume sans identifiant (cas anormal) : on ne peut pas vérifier sa Corbeille, on refuse.
        None => (true, None),
    };
    Ok(VolumeFacts { kind, subst: false, recycle_disabled: policy || nuke, max_bytes })
}
