//! Appels Win32 de `safety` : sonde réelle du système de fichiers, dossiers connus de Windows,
//! Corbeille via `IFileOperation`. Seul module du crate autorisé à utiliser `unsafe`.
//! Testé uniquement sur Windows (CI), avec de vraies fixtures (voir `tests.rs`).
#![allow(unsafe_code)]

pub mod content;
mod effects;
mod elevated;
pub mod process;
mod recycle;
mod restore;
pub mod sinks;
#[cfg(test)]
mod tests;
mod volume;

pub use effects::{tool_dirs, WinEffects};
pub use process::{open_item, open_launcher, running_processes, OpenError};

/// Lit (en administrateur, une demande UAC) les clichés instantanés et points de restauration
/// du lecteur dans `out` (JSON). Lecture seule : aucune suppression.
pub fn read_restore_points(drive: crate::commands::Drive, out: crate::commands::OutputFile, cancel: &std::sync::atomic::AtomicBool) -> crate::exec::CommandRun {
    let command = crate::commands::OfficialCommand::RestorePointsList { drive, out };
    crate::exec::Effects::run_elevated(&WinEffects::new(), &command, cancel)
}

/// Commande de lecture seule de la liste fermée, non élevée (images Docker sans étiquette) ;
/// `None` pour toute autre commande.
pub fn run_read_only(command: &crate::commands::OfficialCommand, cancel: &std::sync::atomic::AtomicBool) -> Option<crate::exec::CommandRun> {
    (command.read_only() && !command.needs_elevation()).then(|| crate::exec::Effects::run_command(&WinEffects::new(), command, cancel))
}

/// Ouvre « Protection du système » de Windows (aucun argument).
pub fn open_system_protection() -> Result<(), String> {
    let layout = system_layout().map_err(|e| e.to_string())?;
    elevated::open_system_protection(&layout.windows)
}
pub use restore::{in_recycle_bin, restore, RestoreError};
pub use volume::{long_path, profile_list, volume_facts};

use std::ffi::c_void;
use std::mem::size_of;

use windows::core::{GUID, HSTRING};
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FileFullDirectoryInfo, FileFullDirectoryRestartInfo, FileIdInfo, GetFileAttributesW, GetFileInformationByHandle,
    GetFileInformationByHandleEx, GetFinalPathNameByHandleW, GetShortPathNameW, GetTempPathW, BY_HANDLE_FILE_INFORMATION,
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_OFFLINE, FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS, FILE_ATTRIBUTE_RECALL_ON_OPEN,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_NO_RECALL,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_LIST_DIRECTORY, FILE_NAME_NORMALIZED, GETFINALPATHNAMEBYHANDLE_FLAGS,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_MODE, FILE_SHARE_NONE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    INVALID_FILE_ATTRIBUTES, OPEN_EXISTING, VOLUME_NAME_DOS,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_LocalAppData, FOLDERID_Music, FOLDERID_Pictures,
    FOLDERID_Profile, FOLDERID_ProgramData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86, FOLDERID_Public,
    FOLDERID_RoamingAppData, FOLDERID_UserProfiles, FOLDERID_Videos, FOLDERID_Windows, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
};

use crate::bin::VolumeFacts;
use crate::denylist::SystemLayout;
use crate::path::WinPath;
use crate::probe::{Attributes, Facts, FileId, Listing, Probe, ProbeError};

#[cfg(test)]
pub(crate) use recycle::recycle;

const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_PATH_NOT_FOUND: u32 = 3;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_SHARING_VIOLATION: u32 = 32;
const ERROR_NO_MORE_FILES: u32 = 18;
const ERROR_BAD_NETPATH: u32 = 53;
const ERROR_INVALID_NAME: u32 = 123;

/// Code Win32 d'une erreur de la crate `windows` (HRESULT_FROM_WIN32 inverse).
fn win32_code(e: &windows::core::Error) -> u32 {
    let hr = e.code().0 as u32;
    if hr & 0xFFFF_0000 == 0x8007_0000 {
        hr & 0xFFFF
    } else {
        hr
    }
}

fn probe_error(e: &windows::core::Error) -> ProbeError {
    match win32_code(e) {
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND | ERROR_BAD_NETPATH | ERROR_INVALID_NAME => ProbeError::NotFound,
        ERROR_ACCESS_DENIED => ProbeError::AccessDenied,
        code => ProbeError::Os(code as i32),
    }
}

fn last_error() -> ProbeError {
    probe_error(&windows::core::Error::from_thread())
}

/// Handle fermé à la sortie de portée.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY : handle valide, obtenu par CreateFileW et fermé une seule fois.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Ouvre l'élément lui-même : un lien n'est jamais suivi (FILE_FLAG_OPEN_REPARSE_POINT), un
/// dossier s'ouvre comme un fichier (FILE_FLAG_BACKUP_SEMANTICS), un fichier cloud n'est jamais
/// rapatrié (FILE_FLAG_OPEN_NO_RECALL). Chemin `\\?\` : pas de limite.
fn open(path: &WinPath, access: u32, share: FILE_SHARE_MODE) -> Result<Handle, windows::core::Error> {
    let name = HSTRING::from(path.verbatim());
    let flags = FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_OPEN_NO_RECALL;
    // SAFETY : chaîne terminée par un nul possédée par `name` pendant l'appel.
    let h = unsafe { CreateFileW(&name, access, share, None, OPEN_EXISTING, flags, None) }?;
    Ok(Handle(h))
}

fn attributes_of(a: u32) -> Attributes {
    Attributes {
        directory: a & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
        reparse_point: a & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0,
        system: a & FILE_ATTRIBUTE_SYSTEM.0 != 0,
        cloud: a & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS.0 | FILE_ATTRIBUTE_RECALL_ON_OPEN.0 | FILE_ATTRIBUTE_OFFLINE.0) != 0,
    }
}

/// Identité et état d'un élément, lus par son handle (pas de seconde ouverture par chemin).
struct HandleFacts {
    attributes: Attributes,
    file_id: FileId,
    size: u64,
    modified: i64,
}

fn handle_facts(h: &Handle) -> Result<HandleFacts, ProbeError> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY : handle valide, structure possédée.
    unsafe { GetFileInformationByHandle(h.0, &mut info) }.map_err(|e| probe_error(&e))?;

    let mut id_info = FILE_ID_INFO::default();
    // SAFETY : handle valide, tampon de la taille annoncée.
    let ex = unsafe {
        GetFileInformationByHandleEx(h.0, FileIdInfo, &mut id_info as *mut FILE_ID_INFO as *mut c_void, size_of::<FILE_ID_INFO>() as u32)
    };
    let file_id = match ex {
        Ok(()) => FileId { volume_serial: id_info.VolumeSerialNumber, id: u128::from_le_bytes(id_info.FileId.Identifier) },
        // Systèmes de fichiers sans ID 128 bits : index 64 bits classique.
        Err(_) => FileId {
            volume_serial: u64::from(info.dwVolumeSerialNumber),
            id: (u128::from(info.nFileIndexHigh) << 32) | u128::from(info.nFileIndexLow),
        },
    };
    Ok(HandleFacts {
        attributes: attributes_of(info.dwFileAttributes),
        file_id,
        size: (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow),
        modified: ((i64::from(info.ftLastWriteTime.dwHighDateTime)) << 32) | i64::from(info.ftLastWriteTime.dwLowDateTime),
    })
}

fn share_all() -> FILE_SHARE_MODE {
    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
}

fn from_utf16(buf: &[u16]) -> Result<String, ProbeError> {
    String::from_utf16(buf).map_err(|_| ProbeError::BadPath(String::from_utf16_lossy(buf)))
}

/// Noms d'un tampon FILE_FULL_DIR_INFO (entrées chaînées par NextEntryOffset).
fn parse_full_dir_info(raw: &[u8], names: &mut Vec<String>) -> Result<(), ProbeError> {
    const NAME_OFFSET: usize = 68;
    let u32_at = |o: usize| raw.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let mut off = 0usize;
    loop {
        let (Some(next), Some(len)) = (u32_at(off), u32_at(off + 60)) else {
            return Err(ProbeError::BadPath("liste de dossier tronquée".into()));
        };
        let start = off + NAME_OFFSET;
        let Some(name_bytes) = raw.get(start..start + len as usize) else {
            return Err(ProbeError::BadPath("liste de dossier tronquée".into()));
        };
        let units: Vec<u16> = name_bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let name = from_utf16(&units)?;
        if name != "." && name != ".." {
            names.push(name);
        }
        if next == 0 {
            return Ok(());
        }
        off += next as usize;
    }
}

/// Sonde réelle, appuyée sur Win32.
#[derive(Debug, Default, Clone, Copy)]
pub struct WinProbe;

impl Probe for WinProbe {
    fn canonicalize(&self, path: &WinPath) -> Result<WinPath, ProbeError> {
        let h = open(path, FILE_READ_ATTRIBUTES.0, share_all()).map_err(|e| probe_error(&e))?;
        let mut buf = vec![0u16; 512];
        loop {
            // SAFETY : handle valide, tampon possédé et de longueur connue.
            let n = unsafe { GetFinalPathNameByHandleW(h.0, &mut buf, GETFINALPATHNAMEBYHANDLE_FLAGS(FILE_NAME_NORMALIZED.0 | VOLUME_NAME_DOS.0)) } as usize;
            if n == 0 {
                return Err(last_error());
            }
            if n < buf.len() {
                buf.truncate(n);
                break;
            }
            buf.resize(n + 1, 0);
        }
        // Le chemin final est déjà en noms longs ; GetLongPathNameW le confirme (aucun 8.3 restant).
        let real = long_path(&from_utf16(&buf)?);
        WinPath::parse(&real).map_err(|e| ProbeError::BadPath(format!("{real} : {e}")))
    }

    fn attributes(&self, path: &WinPath) -> Result<Attributes, ProbeError> {
        let name = HSTRING::from(path.verbatim());
        // SAFETY : chaîne terminée par un nul possédée par `name`.
        let a = unsafe { GetFileAttributesW(&name) };
        if a == INVALID_FILE_ATTRIBUTES {
            return Err(last_error());
        }
        Ok(attributes_of(a))
    }

    fn facts(&self, path: &WinPath) -> Result<Facts, ProbeError> {
        let h = open(path, FILE_READ_ATTRIBUTES.0, share_all()).map_err(|e| probe_error(&e))?;
        let f = handle_facts(&h)?;
        let contains_git = f.attributes.directory
            && !f.attributes.reparse_point
            && path.join(".git").map(|g| self.attributes(&g).is_ok()).unwrap_or(false);
        Ok(Facts { attributes: f.attributes, file_id: f.file_id, size: f.size, modified: f.modified, contains_git })
    }

    fn list(&self, dir: &WinPath) -> Result<Listing, ProbeError> {
        let h = open(dir, FILE_LIST_DIRECTORY.0 | FILE_READ_ATTRIBUTES.0, share_all()).map_err(|e| probe_error(&e))?;
        let f = handle_facts(&h)?;
        if !f.attributes.directory || f.attributes.reparse_point || f.attributes.cloud {
            return Err(ProbeError::Os(267)); // ERROR_DIRECTORY : pas un dossier ordinaire
        }
        let mut names = Vec::new();
        // Tampon aligné sur 8 octets, 64 Kio par appel.
        let mut buf = vec![0u64; 8192];
        let bytes = (buf.len() * 8) as u32;
        let mut class = FileFullDirectoryRestartInfo;
        loop {
            // SAFETY : handle valide, tampon possédé de `bytes` octets.
            let r = unsafe { GetFileInformationByHandleEx(h.0, class, buf.as_mut_ptr() as *mut c_void, bytes) };
            match r {
                Ok(()) => {}
                Err(e) if win32_code(&e) == ERROR_NO_MORE_FILES => break,
                Err(e) => return Err(probe_error(&e)),
            }
            class = FileFullDirectoryInfo;
            // SAFETY : vue en octets du tampon de u64 possédé.
            let raw = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, bytes as usize) };
            parse_full_dir_info(raw, &mut names)?;
        }
        Ok(Listing { file_id: f.file_id, names })
    }

    /// Ouverture sans aucun partage : échoue si un autre programme a l'élément ouvert.
    fn volume_facts(&self, path: &WinPath) -> Result<VolumeFacts, ProbeError> {
        volume_facts(path)
    }

    fn content_hash(&self, path: &WinPath) -> Result<[u8; 32], ProbeError> {
        content::hash_file(path, content::Digest::Full, None)
    }

    fn is_locked(&self, path: &WinPath) -> Result<bool, ProbeError> {
        match open(path, GENERIC_READ.0, FILE_SHARE_NONE) {
            Ok(_) => Ok(false),
            Err(e) if win32_code(&e) == ERROR_SHARING_VIOLATION => Ok(true),
            Err(e) => Err(probe_error(&e)),
        }
    }
}

fn known_folder(id: &GUID) -> Result<WinPath, ProbeError> {
    // SAFETY : GUID statique ; la chaîne renvoyée est libérée par CoTaskMemFree.
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).map_err(|e| probe_error(&e))?;
        let s = p.to_string();
        CoTaskMemFree(Some(p.0 as *const c_void));
        let s = long_path(&s.map_err(|_| ProbeError::BadPath("dossier connu illisible".into()))?);
        WinPath::parse(&s).map_err(|e| ProbeError::BadPath(format!("{s} : {e}")))
    }
}

/// Emplacements réels des dossiers système et du profil courant.
pub fn system_layout() -> Result<SystemLayout, ProbeError> {
    let mut program_files = vec![known_folder(&FOLDERID_ProgramFiles)?];
    if let Ok(x86) = known_folder(&FOLDERID_ProgramFilesX86) {
        program_files.push(x86);
    }
    // Profil courant et autres profils comparés en chemins longs résolus, jamais par leur nom.
    let profile = known_folder(&FOLDERID_Profile)?;
    let other_profiles = profile_list()?.into_iter().filter(|p| p != &profile).collect();
    Ok(SystemLayout {
        windows: known_folder(&FOLDERID_Windows)?,
        program_files,
        program_data: known_folder(&FOLDERID_ProgramData)?,
        users: known_folder(&FOLDERID_UserProfiles)?,
        profile,
        public: known_folder(&FOLDERID_Public)?,
        other_profiles,
    })
}

/// Valeurs des variables des règles (`%LOCALAPPDATA%`, `%TEMP%`…) : dossiers connus de Windows
/// en chemins longs, jamais lus dans l'environnement du processus (sauf `%TEMP%`, qui n'a pas de
/// dossier connu : `GetTempPathW`, puis vérifié par la denylist comme toute cible).
pub fn rule_variables() -> Vec<(&'static str, WinPath)> {
    let folders: [(&str, &GUID); 13] = [
        ("USERPROFILE", &FOLDERID_Profile),
        ("LOCALAPPDATA", &FOLDERID_LocalAppData),
        ("APPDATA", &FOLDERID_RoamingAppData),
        ("DOCUMENTS", &FOLDERID_Documents),
        ("PICTURES", &FOLDERID_Pictures),
        ("VIDEOS", &FOLDERID_Videos),
        ("MUSIC", &FOLDERID_Music),
        ("DESKTOP", &FOLDERID_Desktop),
        ("DOWNLOADS", &FOLDERID_Downloads),
        ("WINDIR", &FOLDERID_Windows),
        ("PROGRAMFILES", &FOLDERID_ProgramFiles),
        ("PROGRAMFILESX86", &FOLDERID_ProgramFilesX86),
        ("PROGRAMDATA", &FOLDERID_ProgramData),
    ];
    let mut out: Vec<(&'static str, WinPath)> = folders.into_iter().filter_map(|(n, id)| known_folder(id).ok().map(|p| (n, p))).collect();
    let mut buf = vec![0u16; 1024];
    // SAFETY : tampon possédé.
    let n = unsafe { GetTempPathW(Some(&mut buf)) } as usize;
    if n > 0 && n < buf.len() {
        if let Ok(p) = WinPath::parse(&long_path(String::from_utf16_lossy(&buf[..n]).trim_end_matches('\\'))) {
            out.push(("TEMP", p));
        }
    }
    if let Ok(w) = known_folder(&FOLDERID_Windows) {
        out.push(("SYSTEMDRIVE", w.volume_root()));
    }
    out
}

/// Nom court 8.3 d'un élément existant (identique au nom long si le volume n'en génère pas).
pub fn short_path(path: &WinPath) -> Result<String, ProbeError> {
    let name = HSTRING::from(path.display_string());
    let mut buf = vec![0u16; 1024];
    // SAFETY : chaîne terminée par un nul et tampon possédés.
    let n = unsafe { GetShortPathNameW(&name, Some(&mut buf)) } as usize;
    if n == 0 || n >= buf.len() {
        return Err(last_error());
    }
    buf.truncate(n);
    from_utf16(&buf)
}
