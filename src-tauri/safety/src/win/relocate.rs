//! Déplacement intelligent, effets réels (`crate::relocate`) :
//!
//! - disques : `GetVolumeInformationW` (NTFS), `GetDiskFreeSpaceExW`, type et SUBST par
//!   `volume_facts` ;
//! - contenu : liste par handle, sans jamais suivre un lien ; programmes qui tiennent des
//!   fichiers : Restart Manager (`RmGetList`), avec leur nom (« Discord ») ;
//! - copie fidèle : `CopyFileExW` (contenu, attributs, flux alternatifs ; jamais par-dessus un
//!   fichier), puis dates (création, accès, écriture), ACL et propriétaire ; dossiers : attributs,
//!   dates, flux alternatifs et ACL recopiés une fois leur contenu copié ;
//! - vérification : BLAKE3 du flux principal **et de chaque flux nommé**, date de dernier accès
//!   gelée pendant la lecture ;
//! - jonction : point de montage `\??\D:\…` posé par `FSCTL_SET_REPARSE_POINT`, relu par
//!   `FSCTL_GET_REPARSE_POINT` ; retirée par handle, sans jamais toucher sa cible ;
//! - suppression définitive : par handle, sémantique POSIX, après un parcours complet qui refuse
//!   tout lien ; renommages sur le même disque seulement (`MoveFileExW` sans copie).

use std::ffi::c_void;

use windows::core::{HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{ERROR_MORE_DATA, ERROR_SUCCESS, FILETIME, GENERIC_READ, GENERIC_WRITE, HLOCAL, LocalFree, WIN32_ERROR};
use windows::Win32::Security::Authorization::{GetNamedSecurityInfoW, SetNamedSecurityInfoW, SE_FILE_OBJECT};
use windows::Win32::Security::{
    GetSecurityDescriptorControl, ACL, DACL_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    PSID, SE_DACL_PROTECTED, UNPROTECTED_DACL_SECURITY_INFORMATION,
};
use windows::Win32::Storage::FileSystem::{
    CopyFileExW, CreateDirectoryW, CreateFileW, FindClose, FindFirstStreamW, FindNextStreamW, FindStreamInfoStandard,
    GetDiskFreeSpaceExW, GetFileAttributesW, GetFileTime, GetVolumeInformationW, MoveFileExW, ReadFile, SetFileAttributesW, SetFileTime,
    WriteFile, COPY_FILE_FAIL_IF_EXISTS, CREATE_NEW, DELETE, FILE_ATTRIBUTE_ARCHIVE, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NOT_CONTENT_INDEXED,
    FILE_ATTRIBUTE_READONLY, FILE_ATTRIBUTE_SYSTEM, FILE_FLAGS_AND_ATTRIBUTES, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_NO_RECALL,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_NONE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FILE_WRITE_ATTRIBUTES, INVALID_FILE_ATTRIBUTES, MOVE_FILE_FLAGS, OPEN_EXISTING, WIN32_FIND_STREAM_DATA,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Registry::{RegCloseKey, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ, REG_VALUE_TYPE};
use windows::Win32::System::RestartManager::{RmEndSession, RmGetList, RmRegisterResources, RmStartSession, RM_PROCESS_INFO};
use windows::Win32::UI::Shell::{FOLDERID_LocalAppData, FOLDERID_LocalAppDataLow, FOLDERID_RoamingAppData};

use super::content::{hash_file, Digest};
use super::effects::mark_for_deletion;
use super::{from_utf16, handle_facts, known_folder, open, probe_error, share_all, volume_facts, Handle, WinProbe};
use crate::path::{fold, WinPath};
use crate::probe::{Probe, ProbeError};
use crate::relocate::raw::{explicit_acl, mount_point_buffer, named_stream, parse_mount_point, MAX_BUFFER};
use crate::relocate::{MoveEffects, MoveIoError, MoveProbe, PathKind, TreeEntry, VolumeInfo};

const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
const FSCTL_GET_REPARSE_POINT: u32 = 0x0009_00A8;
/// Fichiers annoncés au Restart Manager par appel.
const RM_BATCH: usize = 1000;
const BLOCK: usize = 1 << 20;

/// Sonde et effets réels du déplacement.
#[derive(Debug, Default, Clone, Copy)]
pub struct WinMover;

fn io(e: &windows::core::Error) -> MoveIoError {
    MoveIoError::Io(format!("{} ({})", e.message().trim(), probe_error(e)))
}

fn io_code(code: WIN32_ERROR) -> MoveIoError {
    io(&windows::core::Error::from(code.to_hresult()))
}

fn probe_io(e: ProbeError) -> MoveIoError {
    MoveIoError::Io(e.to_string())
}

fn wide(p: &WinPath) -> HSTRING {
    HSTRING::from(p.verbatim())
}

/// Ouvre un élément (sans suivre de lien) avec un accès donné.
fn open_with(path: &str, access: u32, share: windows::Win32::Storage::FileSystem::FILE_SHARE_MODE, disposition: windows::Win32::Storage::FileSystem::FILE_CREATION_DISPOSITION) -> Result<Handle, windows::core::Error> {
    let flags = FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_OPEN_NO_RECALL;
    // SAFETY : chaîne possédée pendant l'appel.
    let h = unsafe { CreateFileW(&HSTRING::from(path), access, share, None, disposition, flags, None) }?;
    Ok(Handle(h))
}

fn walk(dir: &WinPath, rel: &str, out: &mut Vec<TreeEntry>) -> Result<(), ProbeError> {
    let listing = WinProbe.list(dir)?;
    for name in listing.names {
        let path = dir.join(&name).map_err(|e| ProbeError::BadPath(format!("{name} : {e}")))?;
        let f = WinProbe.facts(&path)?;
        let a = f.attributes;
        let r = if rel.is_empty() { name.clone() } else { format!("{rel}\\{name}") };
        out.push(TreeEntry { rel: r.clone(), is_dir: a.directory, size: if a.directory { 0 } else { f.size }, reparse: a.reparse_point, cloud: a.cloud });
        if a.directory && !a.reparse_point && !a.cloud {
            walk(&path, &r, out)?;
        }
    }
    Ok(())
}

fn join_rel(base: &WinPath, rel: &str) -> Result<WinPath, MoveIoError> {
    rel.split('\\').try_fold(base.clone(), |acc, c| acc.join(c)).map_err(|e| MoveIoError::Io(e.to_string()))
}

/// Flux nommés d'un fichier ou d'un dossier (`:nom:$DATA`).
fn streams(path: &WinPath) -> Vec<String> {
    let mut out = Vec::new();
    let mut data = WIN32_FIND_STREAM_DATA::default();
    // SAFETY : chaîne et structure possédées ; handle fermé par FindClose.
    let Ok(h) = (unsafe { FindFirstStreamW(&wide(path), FindStreamInfoStandard, &mut data as *mut _ as *mut c_void, None) }) else {
        return out;
    };
    loop {
        let len = data.cStreamName.iter().position(|&c| c == 0).unwrap_or(data.cStreamName.len());
        if let Ok(name) = from_utf16(&data.cStreamName[..len]) {
            if let Some(n) = named_stream(&name) {
                out.push(n.to_owned());
            }
        }
        // SAFETY : handle de recherche valide, structure possédée.
        if unsafe { FindNextStreamW(h, &mut data as *mut _ as *mut c_void) }.is_err() {
            break;
        }
    }
    // SAFETY : handle de recherche ouvert ci-dessus.
    unsafe {
        let _ = FindClose(h);
    }
    out.sort();
    out
}

fn read(h: &Handle, buf: &mut [u8]) -> Result<usize, windows::core::Error> {
    let mut n = 0u32;
    // SAFETY : handle ouvert en lecture, tampon possédé.
    unsafe { ReadFile(h.0, Some(buf), Some(&mut n), None) }?;
    Ok(n as usize)
}

fn frozen() -> FILETIME {
    FILETIME { dwLowDateTime: u32::MAX, dwHighDateTime: u32::MAX }
}

/// BLAKE3 d'un flux nommé, date de dernier accès gelée.
fn hash_stream(path: &WinPath, stream: &str) -> Result<[u8; 32], ProbeError> {
    let h = open_with(&format!("{}{stream}", path.verbatim()), GENERIC_READ.0 | FILE_WRITE_ATTRIBUTES.0, share_all(), OPEN_EXISTING).map_err(|e| probe_error(&e))?;
    // SAFETY : handle ouvert avec FILE_WRITE_ATTRIBUTES ; aucune date n'est modifiée.
    unsafe { SetFileTime(h.0, None, Some(&frozen()), None) }.map_err(|_| ProbeError::AccessTimeNotFrozen)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; BLOCK];
    loop {
        let n = read(&h, &mut buf).map_err(|e| probe_error(&e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(*hasher.finalize().as_bytes())
}

/// Recopie les flux nommés d'un dossier (ceux des fichiers suivent `CopyFileExW`).
fn copy_streams(from: &WinPath, to: &WinPath) -> Result<(), MoveIoError> {
    for s in streams(from) {
        let src = open_with(&format!("{}{s}", from.verbatim()), GENERIC_READ.0 | FILE_WRITE_ATTRIBUTES.0, share_all(), OPEN_EXISTING).map_err(|e| io(&e))?;
        // SAFETY : handle ouvert avec FILE_WRITE_ATTRIBUTES.
        let _ = unsafe { SetFileTime(src.0, None, Some(&frozen()), None) };
        let dst = open_with(&format!("{}{s}", to.verbatim()), GENERIC_WRITE.0, FILE_SHARE_NONE, CREATE_NEW).map_err(|e| io(&e))?;
        let mut buf = vec![0u8; BLOCK];
        loop {
            let n = read(&src, &mut buf).map_err(|e| io(&e))?;
            if n == 0 {
                break;
            }
            let mut written = 0u32;
            // SAFETY : handle ouvert en écriture, tampon possédé.
            unsafe { WriteFile(dst.0, Some(&buf[..n]), Some(&mut written), None) }.map_err(|e| io(&e))?;
            if written as usize != n {
                return Err(MoveIoError::Io("écriture incomplète d'un flux".into()));
            }
        }
    }
    Ok(())
}

struct Times {
    created: FILETIME,
    accessed: FILETIME,
    written: FILETIME,
}

fn times_of(h: &Handle) -> Result<Times, MoveIoError> {
    let mut t = Times { created: FILETIME::default(), accessed: FILETIME::default(), written: FILETIME::default() };
    // SAFETY : handle valide, structures possédées.
    unsafe { GetFileTime(h.0, Some(&mut t.created), Some(&mut t.accessed), Some(&mut t.written)) }.map_err(|e| io(&e))?;
    Ok(t)
}

fn set_times(path: &WinPath, t: &Times) -> Result<(), MoveIoError> {
    let h = open(path, FILE_WRITE_ATTRIBUTES.0, share_all()).map_err(|e| io(&e))?;
    // SAFETY : handle ouvert avec FILE_WRITE_ATTRIBUTES, dates possédées.
    unsafe { SetFileTime(h.0, Some(&t.created), Some(&t.accessed), Some(&t.written)) }.map_err(|e| io(&e))
}

/// Descripteur de sécurité rendu par Windows, libéré à la sortie de portée.
struct Descriptor(PSECURITY_DESCRIPTOR);

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY : alloué par GetNamedSecurityInfoW, libéré une seule fois.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.0 .0)));
        }
    }
}

/// ACL et propriétaire de `from` posés sur `to`. Fichiers et sous-dossiers : leur ACL telle
/// quelle (protégée ou non, les entrées héritées se recalculent depuis le parent copié) ;
/// dossier déplacé (`top`) : ses droits effectifs, hérités compris, figés et protégés.
fn copy_security(from: &WinPath, to: &WinPath, top: bool) -> Result<(), MoveIoError> {
    let mut sd = PSECURITY_DESCRIPTOR::default();
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut owner = PSID::default();
    // SAFETY : chaîne possédée ; pointeurs de sortie valides ; descripteur libéré par Descriptor.
    let st = unsafe {
        GetNamedSecurityInfoW(&wide(from), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION, Some(&mut owner), None, Some(&mut dacl), None, &mut sd)
    };
    if st != ERROR_SUCCESS {
        return Err(io_code(st));
    }
    let _sd = Descriptor(sd);
    let mut control = 0u16;
    let mut revision = 0u32;
    // SAFETY : descripteur valide.
    unsafe { GetSecurityDescriptorControl(sd, &mut control, &mut revision) }.map_err(|e| io(&e))?;
    // ACL nulle (accès à tous) : jamais recopiée, la copie garde l'ACL héritée de son parent.
    if !dacl.is_null() {
        // SAFETY : ACL valide renvoyée par Windows, de la taille annoncée dans son en-tête.
        let bytes = unsafe { std::slice::from_raw_parts(dacl as *const u8, usize::from((*dacl).AclSize)) };
        let explicit;
        let (acl, info) = if top {
            let raw = explicit_acl(bytes).ok_or(MoveIoError::Io("ACL illisible".into()))?;
            // Tampon aligné pour la structure ACL.
            let mut words = vec![0u32; raw.len().div_ceil(4)];
            // SAFETY : `words` couvre au moins `raw.len()` octets.
            unsafe { std::ptr::copy_nonoverlapping(raw.as_ptr(), words.as_mut_ptr() as *mut u8, raw.len()) };
            explicit = words;
            (explicit.as_ptr() as *const ACL, DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION)
        } else if control & SE_DACL_PROTECTED.0 != 0 {
            (dacl as *const ACL, DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION)
        } else {
            (dacl as *const ACL, DACL_SECURITY_INFORMATION | UNPROTECTED_DACL_SECURITY_INFORMATION)
        };
        // SAFETY : chaîne possédée, ACL valide pendant l'appel.
        let st = unsafe { SetNamedSecurityInfoW(&wide(to), SE_FILE_OBJECT, info, None, None, Some(acl), None) };
        if st != ERROR_SUCCESS {
            return Err(io_code(st));
        }
    }
    // Propriétaire en dernier, au mieux : le changer peut demander un privilège de restauration.
    // SAFETY : SID valide tant que le descripteur vit.
    let _ = unsafe { SetNamedSecurityInfoW(&wide(to), SE_FILE_OBJECT, OWNER_SECURITY_INFORMATION, Some(owner), None, None, None) };
    Ok(())
}

const COPIED_ATTRIBUTES: u32 =
    FILE_ATTRIBUTE_READONLY.0 | FILE_ATTRIBUTE_HIDDEN.0 | FILE_ATTRIBUTE_SYSTEM.0 | FILE_ATTRIBUTE_ARCHIVE.0 | FILE_ATTRIBUTE_NOT_CONTENT_INDEXED.0;

fn attributes(path: &WinPath) -> Option<u32> {
    // SAFETY : chaîne possédée.
    let a = unsafe { GetFileAttributesW(&wide(path)) };
    (a != INVALID_FILE_ATTRIBUTES).then_some(a)
}

/// Suppression définitive par handle d'un élément ordinaire (jamais un lien, sauf `link`).
fn delete_one(path: &WinPath, link: bool) -> Result<(), MoveIoError> {
    let h = open(path, DELETE.0 | FILE_READ_ATTRIBUTES.0, share_all()).map_err(|e| io(&e))?;
    let f = handle_facts(&h).map_err(probe_io)?;
    if f.attributes.reparse_point != link || f.attributes.cloud {
        return Err(MoveIoError::Io(format!("{path} a changé : non supprimé")));
    }
    mark_for_deletion(&h).map_err(|e| MoveIoError::Io(e.to_string()))
}

/// Cible d'une jonction (lue sans la suivre).
fn read_junction(link: &WinPath) -> Option<WinPath> {
    let h = open(link, FILE_READ_ATTRIBUTES.0, share_all()).ok()?;
    let f = handle_facts(&h).ok()?;
    if !f.attributes.reparse_point || !f.attributes.directory {
        return None;
    }
    let mut buf = vec![0u32; MAX_BUFFER / 4];
    let mut n = 0u32;
    // SAFETY : handle valide, tampon possédé de MAX_BUFFER octets.
    unsafe { DeviceIoControl(h.0, FSCTL_GET_REPARSE_POINT, None, 0, Some(buf.as_mut_ptr() as *mut c_void), MAX_BUFFER as u32, Some(&mut n), None) }.ok()?;
    // SAFETY : vue en octets du tampon possédé, limitée à ce que Windows a écrit.
    let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr() as *const u8, (n as usize).min(MAX_BUFFER)) };
    parse_mount_point(bytes)
}

impl MoveProbe for WinMover {
    fn volume(&self, drive: char) -> Option<VolumeInfo> {
        let root = format!("{}:\\", drive.to_ascii_uppercase());
        let name = HSTRING::from(root.as_str());
        let mut fs = [0u16; 64];
        // SAFETY : chaîne et tampon possédés.
        unsafe { GetVolumeInformationW(&name, None, None, None, None, Some(&mut fs)) }.ok()?;
        let len = fs.iter().position(|&c| c == 0).unwrap_or(fs.len());
        let ntfs = String::from_utf16_lossy(&fs[..len]).eq_ignore_ascii_case("NTFS");
        let mut free = 0u64;
        // SAFETY : chaîne possédée, sortie valide.
        unsafe { GetDiskFreeSpaceExW(&name, Some(&mut free), None, None) }.ok()?;
        let facts = volume_facts(&WinPath::parse(&root).ok()?).ok()?;
        Some(VolumeInfo { kind: facts.kind, subst: facts.subst, ntfs, free })
    }

    fn kind(&self, path: &WinPath) -> Option<PathKind> {
        match WinProbe.attributes(path) {
            Ok(a) if a.reparse_point => Some(PathKind::Link),
            Ok(a) if a.directory => Some(PathKind::Dir),
            Ok(_) => Some(PathKind::File),
            Err(ProbeError::NotFound) => None,
            // Présent mais illisible : jamais pris pour un emplacement libre.
            Err(_) => Some(PathKind::File),
        }
    }

    fn tree(&self, dir: &WinPath) -> Result<Vec<TreeEntry>, ProbeError> {
        let mut out = Vec::new();
        walk(dir, "", &mut out)?;
        Ok(out)
    }

    fn holders(&self, files: &[WinPath]) -> Vec<String> {
        if files.is_empty() {
            return Vec::new();
        }
        let unknown = || vec!["un programme inconnu (vérification impossible)".to_owned()];
        let mut session = 0u32;
        let mut key = [0u16; 64];
        // SAFETY : sorties possédées ; session fermée par RmEndSession.
        if unsafe { RmStartSession(&mut session, None, PWSTR(key.as_mut_ptr())) } != ERROR_SUCCESS {
            return unknown();
        }
        let result = (|| {
            for chunk in files.chunks(RM_BATCH) {
                let names: Vec<HSTRING> = chunk.iter().map(|f| HSTRING::from(f.display_string())).collect();
                let ptrs: Vec<PCWSTR> = names.iter().map(|n| PCWSTR(n.as_ptr())).collect();
                // SAFETY : chaînes possédées par `names` pendant l'appel.
                if unsafe { RmRegisterResources(session, Some(&ptrs), None, None) } != ERROR_SUCCESS {
                    return None;
                }
            }
            let mut infos: Vec<RM_PROCESS_INFO> = Vec::new();
            for _ in 0..4 {
                let mut needed = 0u32;
                let mut count = infos.len() as u32;
                let mut reasons = 0u32;
                let out = if infos.is_empty() { None } else { Some(infos.as_mut_ptr()) };
                // SAFETY : tampon possédé de `count` éléments.
                match unsafe { RmGetList(session, &mut needed, &mut count, out, &mut reasons) } {
                    ERROR_SUCCESS => {
                        infos.truncate(count as usize);
                        return Some(infos);
                    }
                    ERROR_MORE_DATA => infos = (0..needed as usize + 4).map(|_| RM_PROCESS_INFO::default()).collect(),
                    _ => return None,
                }
            }
            None
        })();
        // SAFETY : session ouverte ci-dessus.
        unsafe {
            let _ = RmEndSession(session);
        }
        let Some(infos) = result else { return unknown() };
        let mut apps: Vec<String> = Vec::new();
        for i in &infos {
            let len = i.strAppName.iter().position(|&c| c == 0).unwrap_or(i.strAppName.len());
            let name = String::from_utf16_lossy(&i.strAppName[..len]);
            let name = if name.trim().is_empty() { format!("processus {}", i.Process.dwProcessId) } else { name };
            if !apps.contains(&name) {
                apps.push(name);
            }
        }
        apps
    }

    fn hash(&self, file: &WinPath) -> Result<[u8; 32], ProbeError> {
        let main = hash_file(file, Digest::Full, None)?;
        let named = streams(file);
        if named.is_empty() {
            return Ok(main);
        }
        let mut h = blake3::Hasher::new();
        h.update(&main);
        for s in named {
            h.update(fold(&s).as_bytes());
            h.update(&hash_stream(file, &s)?);
        }
        Ok(*h.finalize().as_bytes())
    }

    fn junction_target(&self, link: &WinPath) -> Option<WinPath> {
        read_junction(link)
    }
}

impl MoveEffects for WinMover {
    fn create_dir(&self, path: &WinPath, _template: &WinPath) -> Result<(), MoveIoError> {
        // Dossier neuf, sans modèle : `CreateDirectoryExW` crée le dossier puis peut échouer en
        // recopiant les flux du modèle. Attributs, dates, flux et ACL suivent par `copy_dir_meta`.
        // SAFETY : chaîne possédée.
        unsafe { CreateDirectoryW(&wide(path), None) }.map_err(|e| io(&e))
    }

    fn copy_file(&self, from: &WinPath, to: &WinPath) -> Result<(), MoveIoError> {
        let src = open(from, FILE_READ_ATTRIBUTES.0, share_all()).map_err(|e| io(&e))?;
        let f = handle_facts(&src).map_err(probe_io)?;
        if f.attributes.reparse_point || f.attributes.cloud || f.attributes.directory {
            return Err(MoveIoError::Io(format!("{from} n'est plus un fichier ordinaire")));
        }
        let times = times_of(&src)?;
        drop(src);
        // SAFETY : chaînes possédées ; ni rappel ni annulation.
        unsafe { CopyFileExW(&wide(from), &wide(to), None, None, None, COPY_FILE_FAIL_IF_EXISTS) }.map_err(|e| io(&e))?;
        set_times(to, &times)?;
        copy_security(from, to, false)?;
        // La lecture de la copie a pu toucher la date de dernier accès de l'original : rétablie.
        if let Ok(h) = open(from, FILE_WRITE_ATTRIBUTES.0, share_all()) {
            // SAFETY : handle ouvert avec FILE_WRITE_ATTRIBUTES.
            let _ = unsafe { SetFileTime(h.0, None, Some(&times.accessed), None) };
        }
        Ok(())
    }

    fn copy_dir_meta(&self, from: &WinPath, to: &WinPath, top: bool) -> Result<(), MoveIoError> {
        copy_streams(from, to)?;
        let src = open(from, FILE_READ_ATTRIBUTES.0, share_all()).map_err(|e| io(&e))?;
        let times = times_of(&src)?;
        drop(src);
        set_times(to, &times)?;
        if let Some(a) = attributes(from) {
            // SAFETY : chaîne possédée.
            unsafe { SetFileAttributesW(&wide(to), FILE_FLAGS_AND_ATTRIBUTES(a & COPIED_ATTRIBUTES)) }.map_err(|e| io(&e))?;
        }
        copy_security(from, to, top)
    }

    fn rename(&self, from: &WinPath, to: &WinPath) -> Result<(), MoveIoError> {
        // Même disque seulement : sans MOVEFILE_COPY_ALLOWED, jamais de copie cachée.
        // SAFETY : chaînes possédées.
        unsafe { MoveFileExW(&wide(from), &wide(to), MOVE_FILE_FLAGS(0)) }.map_err(|e| io(&e))
    }

    fn create_junction(&self, link: &WinPath, target: &WinPath) -> Result<(), MoveIoError> {
        let buf = mount_point_buffer(target).ok_or(MoveIoError::Io("cible de jonction invalide".into()))?;
        // SAFETY : chaîne possédée.
        unsafe { CreateDirectoryW(&wide(link), None) }.map_err(|e| io(&e))?;
        let set = (|| {
            let h = open_with(&link.verbatim(), GENERIC_WRITE.0 | FILE_READ_ATTRIBUTES.0 | DELETE.0, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, OPEN_EXISTING)?;
            // SAFETY : handle valide, tampon possédé de la taille annoncée.
            unsafe { DeviceIoControl(h.0, FSCTL_SET_REPARSE_POINT, Some(buf.as_ptr() as *const c_void), buf.len() as u32, None, 0, None, None) }?;
            Ok::<_, windows::core::Error>(())
        })();
        if let Err(e) = set {
            // Le dossier vide créé pour la jonction est retiré.
            let _ = delete_one(link, false);
            return Err(io(&e));
        }
        Ok(())
    }

    fn remove_junction(&self, link: &WinPath, expected: Option<&WinPath>) -> Result<(), MoveIoError> {
        match (read_junction(link), expected) {
            (Some(actual), Some(e)) if actual == *e => {}
            (Some(_), None) => {}
            _ => return Err(MoveIoError::BadJunction),
        }
        delete_one(link, true)
    }

    fn delete_tree(&self, path: &WinPath) -> Result<(), MoveIoError> {
        // Parcours complet d'abord : un lien ou un fichier cloud apparu entre-temps arrête tout,
        // avant la moindre suppression.
        let mut entries = Vec::new();
        walk(path, "", &mut entries).map_err(probe_io)?;
        if entries.iter().any(|e| e.reparse || e.cloud) {
            return Err(MoveIoError::Io("un lien ou un fichier cloud est apparu : rien n'est supprimé".into()));
        }
        // Du plus profond au plus haut.
        entries.sort_by_key(|e| std::cmp::Reverse(e.rel.matches('\\').count()));
        let mut first = None;
        for e in &entries {
            if let Err(err) = join_rel(path, &e.rel).and_then(|p| delete_one(&p, false)) {
                first.get_or_insert(err);
            }
        }
        match first {
            Some(e) => Err(e),
            None => delete_one(path, false),
        }
    }

    fn recycle(&self, path: &WinPath) -> Result<(), MoveIoError> {
        let bytes = self.tree(path).map_err(probe_io)?.iter().map(|e| e.size).sum();
        match super::recycle::recycle_many(&[(path.clone(), bytes)]).pop() {
            Some(Ok(_)) => Ok(()),
            Some(Err(e)) => Err(MoveIoError::Io(e.to_string())),
            None => Err(MoveIoError::Io("Corbeille : aucun résultat".into())),
        }
    }
}

fn reg_keys(root: HKEY, path: &str) -> Vec<String> {
    let mut key = HKEY::default();
    // SAFETY : clé ouverte en lecture, fermée plus bas.
    if unsafe { RegOpenKeyExW(root, &HSTRING::from(path), None, KEY_READ, &mut key) } != ERROR_SUCCESS {
        return Vec::new();
    }
    let mut out = Vec::new();
    for index in 0..1024u32 {
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        // SAFETY : tampon possédé, longueur en caractères.
        if unsafe { RegEnumKeyExW(key, index, Some(PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None) } != ERROR_SUCCESS {
            break;
        }
        out.push(String::from_utf16_lossy(&name[..len as usize]));
    }
    // SAFETY : clé ouverte ci-dessus.
    unsafe {
        let _ = RegCloseKey(key);
    }
    out
}

/// Valeurs texte d'une clé (nom → donnée).
fn reg_strings(root: HKEY, path: &str) -> Vec<String> {
    let mut key = HKEY::default();
    // SAFETY : clé ouverte en lecture, fermée plus bas.
    if unsafe { RegOpenKeyExW(root, &HSTRING::from(path), None, KEY_READ, &mut key) } != ERROR_SUCCESS {
        return Vec::new();
    }
    let mut out = Vec::new();
    for index in 0..1024u32 {
        let mut name = [0u16; 512];
        let mut len = name.len() as u32;
        let mut data = vec![0u16; 2048];
        let mut bytes = (data.len() * 2) as u32;
        let mut kind = REG_VALUE_TYPE::default();
        // SAFETY : tampons possédés, tailles annoncées.
        let st = unsafe {
            RegEnumValueW(key, index, Some(PWSTR(name.as_mut_ptr())), &mut len, None, Some(&mut kind.0), Some(data.as_mut_ptr() as *mut u8), Some(&mut bytes))
        };
        if st == ERROR_MORE_DATA {
            continue;
        }
        if st != ERROR_SUCCESS {
            break;
        }
        if kind == REG_SZ {
            let n = (bytes as usize / 2).min(data.len());
            let end = data[..n].iter().position(|&c| c == 0).unwrap_or(n);
            out.push(String::from_utf16_lossy(&data[..end]));
        }
    }
    // SAFETY : clé ouverte ci-dessus.
    unsafe {
        let _ = RegCloseKey(key);
    }
    out
}

/// Dossiers synchronisés de la machine : OneDrive (variables et comptes) et toute racine
/// déclarée au gestionnaire de synchronisation de Windows (Dropbox, Google Drive, iCloud…).
pub fn synced_roots() -> Vec<WinPath> {
    let mut raw: Vec<String> = ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"].iter().filter_map(|v| std::env::var(v).ok()).collect();
    for account in reg_keys(HKEY_CURRENT_USER, "Software\\Microsoft\\OneDrive\\Accounts") {
        if let Some(folder) = super::volume::reg_string(HKEY_CURRENT_USER, &format!("Software\\Microsoft\\OneDrive\\Accounts\\{account}"), "UserFolder") {
            raw.push(folder);
        }
    }
    const SYNC: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Explorer\\SyncRootManager";
    for provider in reg_keys(HKEY_LOCAL_MACHINE, SYNC) {
        raw.extend(reg_strings(HKEY_LOCAL_MACHINE, &format!("{SYNC}\\{provider}\\UserSyncRoots")));
    }
    let mut out: Vec<WinPath> = Vec::new();
    for r in raw {
        if let Ok(p) = WinPath::parse(&super::long_path(r.trim())) {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

/// AppData (Local, LocalLow, Roaming) : dossiers d'applis déplaçables en « À vérifier ».
pub fn app_data_roots() -> Vec<WinPath> {
    [FOLDERID_LocalAppData, FOLDERID_LocalAppDataLow, FOLDERID_RoamingAppData].iter().filter_map(|id| known_folder(id).ok()).collect()
}
