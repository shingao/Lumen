//! Partie Windows du scanner. Seul module du crate autorisé à utiliser `unsafe`.
//!
//! - [`WinSource`] : liste chaque dossier avec `GetFileInformationByHandleEx(FileIdExtdDirectoryInfo)`,
//!   qui renvoie pour chaque entrée son file ID, son type et son étiquette de point d'analyse. Les
//!   tailles et dates de cette liste sont une copie que NTFS ne tient pas toujours à jour (autres
//!   noms d'un lien physique, fichiers écrits par certaines API) : chaque fichier est donc ouvert
//!   **pour ses attributs seulement** (`OpenFileById`, `FILE_READ_ATTRIBUTES`, sans rappel cloud,
//!   jamais le contenu) et ses tailles et dates sont lues dans son propre enregistrement. Les
//!   fichiers cloud et les points d'analyse ne sont jamais ouverts. Les fichiers WOF, compressés ou
//!   clairsemés passent en plus par `GetCompressedFileSizeW`.
//! - `mft` : lecture directe de la MFT (administrateur), `helper` : assistant élevé.
#![allow(unsafe_code)]

pub mod helper;
pub mod mft;
pub mod trust;
#[cfg(test)]
mod tests;

use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, FILETIME, HANDLE};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FileBasicInfo, FileIdInfo, FileStandardInfo, GetCompressedFileSizeW, GetDiskFreeSpaceExW, GetDiskFreeSpaceW, GetDriveTypeW,
    GetFileInformationByHandle, GetFileInformationByHandleEx, GetLogicalDrives, GetLongPathNameW, GetVolumeInformationW,
    GetVolumePathNameW, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_NO_RECALL,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO, FILE_INFO_BY_HANDLE_CLASS, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, SetFileTime, GetFinalPathNameByHandleW,
    FILE_NAME_NORMALIZED, FILE_WRITE_ATTRIBUTES, GETFINALPATHNAMEBYHANDLE_FLAGS, VOLUME_NAME_DOS, OpenFileById, ExtendedFileIdType, FileIdType,
    FILE_BASIC_INFO, FILE_ID_128, FILE_ID_DESCRIPTOR, FILE_ID_DESCRIPTOR_0, FILE_STANDARD_INFO,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::{FOLDERID_Profile, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

use crate::classify::{classify, listed_allocation, needs_compressed_size, ATTR_DIRECTORY, ATTR_REPARSE};
use crate::model::{Kind, RawEntry};
use crate::walk::{DirError, Source};

const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_PATH_NOT_FOUND: u32 = 3;
const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_NO_MORE_FILES: u32 = 18;
const ERROR_SHARING_VIOLATION: u32 = 32;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_INVALID_PARAMETER: u32 = 87;
const ERROR_INVALID_LEVEL: u32 = 124;
const ERROR_INVALID_FUNCTION: u32 = 1;

pub(crate) fn win32_code(e: &windows::core::Error) -> u32 {
    let hr = e.code().0 as u32;
    if hr & 0xFFFF_0000 == 0x8007_0000 {
        hr & 0xFFFF
    } else {
        hr
    }
}

fn dir_error(e: &windows::core::Error) -> DirError {
    match win32_code(e) {
        ERROR_ACCESS_DENIED => DirError::AccessDenied,
        ERROR_SHARING_VIOLATION => DirError::Locked,
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => DirError::NotFound,
        code => DirError::Other(code as i32),
    }
}

/// Handle fermé à la sortie de portée.
pub(crate) struct Handle(pub(crate) HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY : handle valide, fermé une seule fois.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Forme `\\?\` : pas de limite de 260 caractères, pas de réinterprétation du chemin.
pub fn verbatim(path: &str) -> String {
    if path.starts_with("\\\\?\\") {
        path.to_owned()
    } else if let Some(unc) = path.strip_prefix("\\\\") {
        format!("\\\\?\\UNC\\{unc}")
    } else {
        format!("\\\\?\\{path}")
    }
}

/// Ouvre un dossier pour lister son contenu (ou un élément pour lire ses attributs), sans suivre
/// de lien et sans jamais déclencher de rapatriement depuis le cloud.
fn open_meta(path: &str, access: u32) -> Result<Handle, windows::core::Error> {
    let name = HSTRING::from(verbatim(path));
    // SAFETY : chaîne possédée pendant l'appel.
    let h = unsafe {
        CreateFileW(
            &name,
            access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_OPEN_NO_RECALL,
            None,
        )
    }?;
    Ok(Handle(h))
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    b.get(i..i + 4).map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn u64_at(b: &[u8], i: usize) -> u64 {
    b.get(i..i + 8).map_or(0, |s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
}

fn utf16_at(b: &[u8], start: usize, bytes: usize) -> String {
    let raw = b.get(start..start + bytes).unwrap_or(&[]);
    let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    String::from_utf16_lossy(&units)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layout {
    /// FILE_ID_EXTD_DIR_INFO : file ID 128 bits et étiquette de point d'analyse.
    Extd,
    /// FILE_ID_BOTH_DIR_INFO : file ID 64 bits (systèmes de fichiers qui ignorent la classe étendue).
    Both,
}

/// Source Windows du parcours : métadonnées des listes de dossier uniquement.
/// Ce que l'enregistrement du fichier dit de lui-même.
struct OwnRecord {
    logical: u64,
    allocation: u64,
    attributes: u32,
    modified: i64,
    accessed: i64,
}

static COMPRESSED_LOOKUPS: AtomicU64 = AtomicU64::new(0);
static COMPRESSED_NANOS: AtomicU64 = AtomicU64::new(0);

/// Lectures `GetCompressedFileSizeW` faites depuis le lancement du processus : (nombre, durée
/// cumulée en ms, tous fils confondus). Mesure du banc d'essai.
pub fn compressed_lookups() -> (u64, u64) {
    (COMPRESSED_LOOKUPS.load(Ordering::Relaxed), COMPRESSED_NANOS.load(Ordering::Relaxed) / 1_000_000)
}

#[derive(Debug, Clone, Copy)]
pub struct WinSource {
    cluster: u64,
}

impl WinSource {
    pub fn new(root: &str) -> WinSource {
        WinSource { cluster: cluster_size(root).unwrap_or(4096) }
    }

    /// Taille réelle sur disque d'un fichier compressé, clairsemé ou WOF (métadonnée seule).
    fn compressed_size(&self, path: &str) -> Option<u64> {
        let started = Instant::now();
        let name = HSTRING::from(verbatim(path));
        let mut high = 0u32;
        // SAFETY : chaîne et entier possédés.
        let low = unsafe { GetCompressedFileSizeW(&name, Some(&mut high)) };
        let failed = low == u32::MAX && windows::core::Error::from_thread().code().is_err();
        COMPRESSED_LOOKUPS.fetch_add(1, Ordering::Relaxed);
        COMPRESSED_NANOS.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
        if failed {
            return None;
        }
        Some((u64::from(high) << 32) | u64::from(low))
    }

    /// Tailles et dates lues dans l'enregistrement du fichier lui-même (et non dans la copie de la
    /// liste du dossier). Ouverture par file ID pour les attributs seulement : ni contenu, ni rappel.
    fn own_record(&self, dir: &Handle, layout: Layout, file_id: u128) -> Option<OwnRecord> {
        let descriptor = FILE_ID_DESCRIPTOR {
            dwSize: std::mem::size_of::<FILE_ID_DESCRIPTOR>() as u32,
            Type: if layout == Layout::Extd { ExtendedFileIdType } else { FileIdType },
            Anonymous: if layout == Layout::Extd {
                FILE_ID_DESCRIPTOR_0 { ExtendedFileId: FILE_ID_128 { Identifier: file_id.to_le_bytes() } }
            } else {
                FILE_ID_DESCRIPTOR_0 { FileId: file_id as u64 as i64 }
            },
        };
        // SAFETY : handle de dossier valide (indique le volume), descripteur possédé.
        let h = unsafe {
            OpenFileById(
                dir.0,
                &descriptor,
                FILE_READ_ATTRIBUTES.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_OPEN_NO_RECALL,
            )
        }
        .ok()
        .map(Handle)?;
        let mut standard = FILE_STANDARD_INFO::default();
        let mut basic = FILE_BASIC_INFO::default();
        // SAFETY : handle valide, structures possédées de la taille annoncée.
        unsafe {
            GetFileInformationByHandleEx(h.0, FileStandardInfo, &mut standard as *mut _ as *mut c_void, std::mem::size_of::<FILE_STANDARD_INFO>() as u32).ok()?;
            GetFileInformationByHandleEx(h.0, FileBasicInfo, &mut basic as *mut _ as *mut c_void, std::mem::size_of::<FILE_BASIC_INFO>() as u32).ok()?;
        }
        Some(OwnRecord {
            logical: standard.EndOfFile.max(0) as u64,
            allocation: standard.AllocationSize.max(0) as u64,
            attributes: basic.FileAttributes,
            modified: basic.LastWriteTime,
            accessed: basic.LastAccessTime,
        })
    }

    fn parse(&self, dir: &str, dir_h: &Handle, layout: Layout, buf: &[u8], out: &mut Vec<RawEntry>) {
        let mut off = 0usize;
        loop {
            let e = &buf[off.min(buf.len())..];
            if e.len() < 72 {
                break;
            }
            let next = u32_at(e, 0) as usize;
            let accessed = u64_at(e, 16) as i64;
            let modified = u64_at(e, 24) as i64;
            let eof = u64_at(e, 40);
            let alloc = u64_at(e, 48);
            let attributes = u32_at(e, 56);
            let name_len = u32_at(e, 60) as usize;
            let (file_id, tag, name_at) = match layout {
                Layout::Extd => {
                    let id = e.get(72..88).map_or(0, |s| {
                        let mut a = [0u8; 16];
                        a.copy_from_slice(s);
                        u128::from_le_bytes(a)
                    });
                    (id, u32_at(e, 68), 88)
                }
                // Dans FILE_ID_BOTH_DIR_INFO, EaSize porte l'étiquette quand l'entrée est un point d'analyse.
                Layout::Both => (u128::from(u64_at(e, 96)), u32_at(e, 64), 104),
            };
            let name = utf16_at(e, name_at, name_len);
            if name != "." && name != ".." {
                let tag = if attributes & ATTR_REPARSE != 0 { tag } else { 0 };
                let mut kind = classify(attributes, tag);
                let (mut eof, mut alloc, mut attributes, mut modified, mut accessed) = (eof, alloc, attributes, modified, accessed);
                if kind == Kind::File {
                    if let Some(own) = self.own_record(dir_h, layout, file_id) {
                        (eof, alloc, modified, accessed) = (own.logical, own.allocation, own.modified, own.accessed);
                        attributes = own.attributes;
                        // Un fichier devenu cloud entre-temps reste compté à part.
                        kind = classify(attributes, tag);
                    }
                }
                let allocated = match kind {
                    Kind::File if needs_compressed_size(attributes, tag) => {
                        self.compressed_size(&self.join(dir, &name)).unwrap_or(alloc)
                    }
                    Kind::File => listed_allocation(alloc, self.cluster),
                    _ => 0,
                };
                let logical = if matches!(kind, Kind::File | Kind::CloudFile) { eof } else { 0 };
                out.push(RawEntry { name, kind, attributes, file_id, logical, allocated, modified, accessed });
            }
            if next == 0 {
                break;
            }
            off += next;
        }
    }
}

impl Source for WinSource {
    fn root(&self, path: &str) -> Result<RawEntry, DirError> {
        let h = open_meta(path, FILE_READ_ATTRIBUTES.0).map_err(|e| dir_error(&e))?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY : handle valide, structure possédée.
        unsafe { GetFileInformationByHandle(h.0, &mut info) }.map_err(|e| dir_error(&e))?;
        let name = path.trim_end_matches('\\').rsplit('\\').next().unwrap_or(path).to_owned();
        Ok(RawEntry {
            name,
            kind: Kind::Dir,
            attributes: info.dwFileAttributes | ATTR_DIRECTORY,
            file_id: file_id_of(&h).unwrap_or(0),
            logical: 0,
            allocated: 0,
            modified: filetime(info.ftLastWriteTime.dwHighDateTime, info.ftLastWriteTime.dwLowDateTime),
            accessed: filetime(info.ftLastAccessTime.dwHighDateTime, info.ftLastAccessTime.dwLowDateTime),
        })
    }

    fn read_dir(&self, path: &str, out: &mut Vec<RawEntry>) -> Result<(), DirError> {
        let h = open_dir(path)?;
        // Tampon aligné sur 8 octets, 64 Kio par appel.
        let mut buf = vec![0u64; 8192];
        let bytes = (buf.len() * 8) as u32;
        let mut layout = Layout::Extd;
        let mut first = true;
        loop {
            let class = match (layout, first) {
                (Layout::Extd, true) => 20,  // FileIdExtdDirectoryRestartInfo
                (Layout::Extd, false) => 19, // FileIdExtdDirectoryInfo
                (Layout::Both, true) => 11,  // FileIdBothDirectoryRestartInfo
                (Layout::Both, false) => 10, // FileIdBothDirectoryInfo
            };
            // SAFETY : handle valide, tampon possédé de `bytes` octets.
            let r = unsafe {
                GetFileInformationByHandleEx(h.0, FILE_INFO_BY_HANDLE_CLASS(class), buf.as_mut_ptr() as *mut c_void, bytes)
            };
            match r {
                Ok(()) => {
                    first = false;
                    // SAFETY : vue en octets du tampon de u64 possédé.
                    let raw = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, bytes as usize) };
                    self.parse(path, &h, layout, raw, out);
                }
                Err(e) => {
                    let code = win32_code(&e);
                    if code == ERROR_NO_MORE_FILES {
                        break;
                    }
                    let unsupported = matches!(code, ERROR_INVALID_PARAMETER | ERROR_NOT_SUPPORTED | ERROR_INVALID_LEVEL | ERROR_INVALID_FUNCTION);
                    if first && layout == Layout::Extd && unsupported {
                        layout = Layout::Both;
                        continue;
                    }
                    return Err(dir_error(&e));
                }
            }
        }
        Ok(())
    }

    fn join(&self, dir: &str, name: &str) -> String {
        if dir.ends_with('\\') {
            format!("{dir}{name}")
        } else {
            format!("{dir}\\{name}")
        }
    }

    fn volume_serial(&self, root: &str) -> u64 {
        open_meta(root, FILE_READ_ATTRIBUTES.0).ok().and_then(|h| volume_serial_of(&h)).unwrap_or(0)
    }

    fn last_access_reliable(&self, _root: &str) -> bool {
        last_access_reliable()
    }
}

pub(crate) fn filetime(high: u32, low: u32) -> i64 {
    ((u64::from(high) << 32) | u64::from(low)) as i64
}

fn id_info(h: &Handle) -> Option<FILE_ID_INFO> {
    let mut info = FILE_ID_INFO::default();
    // SAFETY : handle valide, tampon de la taille annoncée.
    unsafe {
        GetFileInformationByHandleEx(h.0, FileIdInfo, &mut info as *mut FILE_ID_INFO as *mut c_void, std::mem::size_of::<FILE_ID_INFO>() as u32)
    }
    .ok()?;
    Some(info)
}

fn file_id_of(h: &Handle) -> Option<u128> {
    id_info(h).map(|i| u128::from_le_bytes(i.FileId.Identifier))
}

fn volume_serial_of(h: &Handle) -> Option<u64> {
    id_info(h).map(|i| i.VolumeSerialNumber)
}

/// Ouvre un dossier pour le lister sans changer sa date de dernier accès : lister un dossier la
/// met à jour (quand Windows tient cette date) ; `SetFileTime` à `0xFFFFFFFF` la gèle pour toutes
/// les opérations faites par ce handle. Le gel demande `FILE_WRITE_ATTRIBUTES` ; s'il est refusé
/// (dossier système), le dossier est listé sans gel. Les fichiers, eux, ne sont ouverts que pour
/// leurs attributs (`FILE_READ_ATTRIBUTES`), ce qui ne touche jamais leur dernier accès.
fn open_dir(path: &str) -> Result<Handle, DirError> {
    if let Ok(h) = open_meta(path, FILE_LIST_DIRECTORY.0 | FILE_WRITE_ATTRIBUTES.0) {
        let frozen = FILETIME { dwLowDateTime: u32::MAX, dwHighDateTime: u32::MAX };
        // SAFETY : handle valide ouvert avec FILE_WRITE_ATTRIBUTES ; aucune date n'est modifiée.
        if unsafe { SetFileTime(h.0, None, Some(&frozen), None) }.is_ok() {
            return Ok(h);
        }
    }
    open_meta(path, FILE_LIST_DIRECTORY.0).map_err(|e| dir_error(&e))
}

/// Chemin réel d'un dossier ou d'un fichier (noms longs, liens résolus), obtenu par son handle :
/// contrairement à `GetLongPathNameW`, ne liste aucun dossier parent (dont la date de dernier
/// accès changerait).
pub fn real_path(path: &str) -> Option<String> {
    let h = open_meta(path, FILE_READ_ATTRIBUTES.0).ok()?;
    let mut buf = vec![0u16; 32_768];
    let flags = GETFINALPATHNAMEBYHANDLE_FLAGS(FILE_NAME_NORMALIZED.0 | VOLUME_NAME_DOS.0);
    // SAFETY : handle valide, tampon possédé.
    let n = unsafe { GetFinalPathNameByHandleW(h.0, &mut buf, flags) } as usize;
    if n == 0 || n >= buf.len() {
        return None;
    }
    let p = String::from_utf16_lossy(&buf[..n]);
    if p.starts_with("\\\\?\\UNC\\") {
        return None;
    }
    Some(p.strip_prefix("\\\\?\\").unwrap_or(&p).to_owned())
}

/// Racine d'une analyse : chemin réel, ou à défaut en noms longs.
pub fn root_path(path: &str) -> String {
    real_path(path).unwrap_or_else(|| long_path(path))
}

/// Chemin en noms longs (aucun nom court 8.3), inchangé si Windows ne peut pas le résoudre.
pub fn long_path(path: &str) -> String {
    let mut buf = vec![0u16; 32_768];
    // SAFETY : chaîne et tampon possédés.
    let n = unsafe { GetLongPathNameW(&HSTRING::from(path), Some(&mut buf)) } as usize;
    if n == 0 || n >= buf.len() {
        return path.to_owned();
    }
    String::from_utf16_lossy(&buf[..n])
}

/// Racine du volume qui porte `path` (`C:\`, ou un dossier de montage).
pub fn volume_root(path: &str) -> String {
    let mut buf = vec![0u16; 1024];
    // SAFETY : chaîne et tampon possédés.
    if unsafe { GetVolumePathNameW(&HSTRING::from(path), &mut buf) }.is_ok() {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        return String::from_utf16_lossy(&buf[..len]);
    }
    path.chars().take(3).collect()
}

fn cluster_size(path: &str) -> Option<u64> {
    let root = HSTRING::from(volume_root(path));
    let (mut spc, mut bps, mut free, mut total) = (0u32, 0u32, 0u32, 0u32);
    // SAFETY : chaîne et entiers possédés.
    unsafe { GetDiskFreeSpaceW(&root, Some(&mut spc), Some(&mut bps), Some(&mut free), Some(&mut total)) }.ok()?;
    Some(u64::from(spc) * u64::from(bps))
}

/// Windows tient-il à jour la date de dernier accès (`NtfsDisableLastAccessUpdate`) ?
pub fn last_access_reliable() -> bool {
    let mut value = 0u32;
    let mut size = 4u32;
    // SAFETY : chaînes et tampon possédés.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &HSTRING::from("SYSTEM\\CurrentControlSet\\Control\\FileSystem"),
            &HSTRING::from("NtfsDisableLastAccessUpdate"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    // Bit 0 à 1 : mise à jour désactivée (valeurs 1, 0x80000001, 0x80000003).
    status == ERROR_SUCCESS && value & 1 == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeKind {
    Fixed,
    Removable,
    Network,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub letter: char,
    pub label: String,
    pub file_system: String,
    pub kind: VolumeKind,
    pub total: u64,
    pub free: u64,
}

impl Volume {
    /// La MFT ne se lit que sur un disque fixe NTFS (ni amovible, ni réseau, ni lecteur SUBST).
    pub fn mft_capable(&self) -> bool {
        self.file_system.eq_ignore_ascii_case("NTFS") && self.kind == VolumeKind::Fixed && helper::validate_volume(self.letter).is_ok()
    }
}

/// Lecteurs montés, avec leur espace et leur système de fichiers.
pub fn volumes() -> Vec<Volume> {
    // SAFETY : sans paramètre.
    let mask = unsafe { GetLogicalDrives() };
    let mut out = Vec::new();
    for (i, letter) in ('A'..='Z').enumerate() {
        if mask & (1 << i) == 0 {
            continue;
        }
        let root = HSTRING::from(format!("{letter}:\\"));
        // SAFETY : chaîne possédée.
        let kind = match unsafe { GetDriveTypeW(&root) } {
            2 => VolumeKind::Removable,
            3 => VolumeKind::Fixed,
            4 => VolumeKind::Network,
            0 | 1 => continue,
            _ => VolumeKind::Other,
        };
        let (mut free, mut total) = (0u64, 0u64);
        // SAFETY : chaîne et entiers possédés.
        if unsafe { GetDiskFreeSpaceExW(&root, None, Some(&mut total), Some(&mut free)) }.is_err() {
            continue;
        }
        let mut label = [0u16; 261];
        let mut fs = [0u16; 261];
        // SAFETY : chaîne et tampons possédés.
        let _ = unsafe { GetVolumeInformationW(&root, Some(&mut label), None, None, None, Some(&mut fs)) };
        let text = |b: &[u16]| String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]);
        out.push(Volume { letter, label: text(&label), file_system: text(&fs), kind, total, free });
    }
    out
}

/// L'appli tourne-t-elle déjà avec les droits administrateur ?
pub fn is_elevated() -> bool {
    let mut token = HANDLE::default();
    // SAFETY : pseudo-handle du processus courant ; jeton fermé par Handle.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.is_err() {
        return false;
    }
    let token = Handle(token);
    let mut elevation = TOKEN_ELEVATION::default();
    let mut size = 0u32;
    // SAFETY : structure possédée, taille annoncée.
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        )
    };
    ok.is_ok() && elevation.TokenIsElevated != 0
}

/// Dossier du profil de l'utilisateur courant.
pub fn user_profile() -> Option<String> {
    // SAFETY : GUID statique ; chaîne libérée par CoTaskMemFree.
    unsafe {
        let p: PWSTR = SHGetKnownFolderPath(&FOLDERID_Profile, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const c_void));
        s
    }
}

/// Mémoire du processus : (pic de l'ensemble de travail, ensemble de travail actuel), en octets.
pub fn memory_usage() -> (u64, u64) {
    let mut c = PROCESS_MEMORY_COUNTERS { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32, ..Default::default() };
    // SAFETY : pseudo-handle et structure possédée.
    let _ = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
    (c.PeakWorkingSetSize as u64, c.WorkingSetSize as u64)
}
