//! Appels Win32 de `safety` : sonde réelle du système de fichiers, dossiers connus de Windows,
//! Corbeille via `IFileOperation`. Seul module du crate autorisé à utiliser `unsafe`.
//! Testé uniquement sur Windows (CI), avec de vraies fixtures (voir `tests.rs`).
#![allow(unsafe_code)]

mod recycle;
#[cfg(test)]
mod tests;

use std::ffi::c_void;
use std::mem::size_of;

use windows::core::{GUID, HSTRING};
use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FileIdInfo, GetFileAttributesW, GetFileInformationByHandle, GetFileInformationByHandleEx,
    GetFinalPathNameByHandleW, GetShortPathNameW, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY,
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SYSTEM, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_ID_INFO, FILE_NAME_NORMALIZED, GETFINALPATHNAMEBYHANDLE_FLAGS, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_MODE, FILE_SHARE_NONE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, INVALID_FILE_ATTRIBUTES, OPEN_EXISTING, VOLUME_NAME_DOS,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_Profile, FOLDERID_ProgramData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86, FOLDERID_Public,
    FOLDERID_UserProfiles, FOLDERID_Windows, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
};

use crate::denylist::SystemLayout;
use crate::path::WinPath;
use crate::probe::{Attributes, Facts, FileId, Probe, ProbeError};

#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use recycle::recycle;

const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_PATH_NOT_FOUND: u32 = 3;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_SHARING_VIOLATION: u32 = 32;
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
/// dossier s'ouvre comme un fichier (FILE_FLAG_BACKUP_SEMANTICS). Chemin `\\?\` : pas de limite.
fn open(path: &WinPath, access: u32, share: FILE_SHARE_MODE) -> Result<Handle, windows::core::Error> {
    let name = HSTRING::from(path.verbatim());
    // SAFETY : chaîne terminée par un nul possédée par `name` pendant l'appel.
    let h = unsafe {
        CreateFileW(&name, access, share, None, OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT, None)
    }?;
    Ok(Handle(h))
}

fn share_all() -> FILE_SHARE_MODE {
    FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE
}

fn from_utf16(buf: &[u16]) -> Result<String, ProbeError> {
    String::from_utf16(buf).map_err(|_| ProbeError::BadPath(String::from_utf16_lossy(buf)))
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
        let real = from_utf16(&buf)?;
        WinPath::parse(&real).map_err(|e| ProbeError::BadPath(format!("{real} : {e}")))
    }

    fn attributes(&self, path: &WinPath) -> Result<Attributes, ProbeError> {
        let name = HSTRING::from(path.verbatim());
        // SAFETY : chaîne terminée par un nul possédée par `name`.
        let a = unsafe { GetFileAttributesW(&name) };
        if a == INVALID_FILE_ATTRIBUTES {
            return Err(last_error());
        }
        Ok(Attributes {
            directory: a & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
            reparse_point: a & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0,
            system: a & FILE_ATTRIBUTE_SYSTEM.0 != 0,
        })
    }

    fn facts(&self, path: &WinPath) -> Result<Facts, ProbeError> {
        let h = open(path, FILE_READ_ATTRIBUTES.0, share_all()).map_err(|e| probe_error(&e))?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY : handle valide, structure possédée.
        unsafe { GetFileInformationByHandle(h.0, &mut info) }.map_err(|e| probe_error(&e))?;

        let mut id_info = FILE_ID_INFO::default();
        // SAFETY : handle valide, tampon de la taille annoncée.
        let ex = unsafe {
            GetFileInformationByHandleEx(
                h.0,
                FileIdInfo,
                &mut id_info as *mut FILE_ID_INFO as *mut c_void,
                size_of::<FILE_ID_INFO>() as u32,
            )
        };
        let file_id = match ex {
            Ok(()) => FileId { volume_serial: id_info.VolumeSerialNumber, id: u128::from_le_bytes(id_info.FileId.Identifier) },
            // Systèmes de fichiers sans ID 128 bits : index 64 bits classique.
            Err(_) => FileId {
                volume_serial: u64::from(info.dwVolumeSerialNumber),
                id: (u128::from(info.nFileIndexHigh) << 32) | u128::from(info.nFileIndexLow),
            },
        };

        let a = info.dwFileAttributes;
        let attributes = Attributes {
            directory: a & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
            reparse_point: a & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0,
            system: a & FILE_ATTRIBUTE_SYSTEM.0 != 0,
        };
        let contains_git = attributes.directory
            && !attributes.reparse_point
            && path.join(".git").map(|g| self.attributes(&g).is_ok()).unwrap_or(false);
        Ok(Facts {
            attributes,
            file_id,
            size: (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow),
            modified: ((i64::from(info.ftLastWriteTime.dwHighDateTime)) << 32) | i64::from(info.ftLastWriteTime.dwLowDateTime),
            contains_git,
        })
    }

    /// Ouverture sans aucun partage : échoue si un autre programme a l'élément ouvert.
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
        let s = s.map_err(|_| ProbeError::BadPath("dossier connu illisible".into()))?;
        WinPath::parse(&s).map_err(|e| ProbeError::BadPath(format!("{s} : {e}")))
    }
}

/// Emplacements réels des dossiers système et du profil courant.
pub fn system_layout() -> Result<SystemLayout, ProbeError> {
    let mut program_files = vec![known_folder(&FOLDERID_ProgramFiles)?];
    if let Ok(x86) = known_folder(&FOLDERID_ProgramFilesX86) {
        program_files.push(x86);
    }
    Ok(SystemLayout {
        windows: known_folder(&FOLDERID_Windows)?,
        program_files,
        program_data: known_folder(&FOLDERID_ProgramData)?,
        users: known_folder(&FOLDERID_UserProfiles)?,
        profile: known_folder(&FOLDERID_Profile)?,
        public: known_folder(&FOLDERID_Public)?,
    })
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
