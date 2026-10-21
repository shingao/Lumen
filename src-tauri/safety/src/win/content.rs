//! Lecture du contenu des fichiers : la seule de Lumen, pour les doublons (recherche, puis
//! nouveau hash juste avant la suppression).
//!
//! - Date de dernier accès gelée avant toute lecture (`SetFileTime` à `0xFFFFFFFF` sur le handle,
//!   ce qui vaut pour toutes les opérations faites par ce handle) ; si le gel est refusé, le
//!   fichier n'est pas lu.
//! - E/S en basse priorité (`THREAD_MODE_BACKGROUND_BEGIN`) pendant toute la lecture.
//! - Jamais un fichier cloud (attributs vérifiés avant l'ouverture, puis `FILE_FLAG_OPEN_NO_RECALL`),
//!   jamais un lien (`FILE_FLAG_OPEN_REPARSE_POINT`, attribut vérifié sur le handle).
//! - Annulable entre deux blocs.

use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::HSTRING;
use windows::Win32::Foundation::FILETIME;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, SetFilePointerEx, SetFileTime, FILE_BEGIN, FILE_FLAG_OPEN_NO_RECALL, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_FLAG_SEQUENTIAL_SCAN, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, OPEN_EXISTING,
};
use windows::Win32::Foundation::GENERIC_READ;
use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN, THREAD_MODE_BACKGROUND_END};

use super::{handle_facts, probe_error, Handle, WinProbe};
use crate::path::WinPath;
use crate::probe::{Probe, ProbeError};

/// Taille des bords lus par l'empreinte rapide.
pub const EDGE: u64 = 64 * 1024;
const BLOCK: usize = 1 << 20;

/// Fil en E/S basse priorité tant que la garde vit (rien si le fil y était déjà).
pub struct BackgroundIo(bool);

impl BackgroundIo {
    pub fn begin() -> BackgroundIo {
        // SAFETY : pseudo-handle du fil courant.
        BackgroundIo(unsafe { SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN) }.is_ok())
    }
}

impl Drop for BackgroundIo {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY : pseudo-handle du fil courant ; fin du mode commencé par `begin`.
            unsafe {
                let _ = SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_END);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Digest {
    /// Premiers et derniers 64 Kio (et la taille) : tri rapide avant le hash complet.
    Edges,
    /// Contenu complet.
    Full,
}

fn read_block(h: &Handle, buf: &mut [u8]) -> Result<usize, ProbeError> {
    let mut n = 0u32;
    // SAFETY : handle valide ouvert en lecture, tampon possédé de longueur connue.
    unsafe { ReadFile(h.0, Some(buf), Some(&mut n), None) }.map_err(|e| probe_error(&e))?;
    Ok(n as usize)
}

/// Empreinte BLAKE3 d'un fichier ordinaire, sans toucher à sa date de dernier accès.
pub fn hash_file(path: &WinPath, digest: Digest, cancel: Option<&AtomicBool>) -> Result<[u8; 32], ProbeError> {
    // Attributs d'abord : un fichier cloud n'est jamais ouvert en lecture.
    let a = WinProbe.attributes(path)?;
    if a.cloud {
        return Err(ProbeError::Cloud);
    }
    if a.directory || a.reparse_point {
        return Err(ProbeError::Os(5));
    }
    let _background = BackgroundIo::begin();
    let name = HSTRING::from(path.verbatim());
    let flags = FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_OPEN_NO_RECALL | FILE_FLAG_SEQUENTIAL_SCAN;
    // SAFETY : chaîne terminée par un nul possédée pendant l'appel.
    let h = unsafe {
        CreateFileW(
            &name,
            GENERIC_READ.0 | FILE_WRITE_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            flags,
            None,
        )
    }
    .map(Handle)
    .map_err(|e| probe_error(&e))?;
    let f = handle_facts(&h)?;
    if f.attributes.cloud {
        return Err(ProbeError::Cloud);
    }
    if f.attributes.directory || f.attributes.reparse_point {
        return Err(ProbeError::Os(5));
    }
    let frozen = FILETIME { dwLowDateTime: u32::MAX, dwHighDateTime: u32::MAX };
    // SAFETY : handle valide ouvert avec FILE_WRITE_ATTRIBUTES ; aucune date n'est modifiée.
    unsafe { SetFileTime(h.0, None, Some(&frozen), None) }.map_err(|_| ProbeError::AccessTimeNotFrozen)?;

    let mut hasher = blake3::Hasher::new();
    hasher.update(&f.size.to_le_bytes());
    let mut buf = vec![0u8; BLOCK];
    let stop = || cancel.is_some_and(|c| c.load(Ordering::SeqCst));
    match digest {
        Digest::Full => loop {
            if stop() {
                return Err(ProbeError::Cancelled);
            }
            let n = read_block(&h, &mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        },
        Digest::Edges => {
            let head = (f.size.min(EDGE)) as usize;
            let n = read_block(&h, &mut buf[..head])?;
            hasher.update(&buf[..n]);
            if f.size > EDGE {
                let tail_start = f.size.saturating_sub(EDGE).max(EDGE);
                // SAFETY : handle valide ; déplacement absolu dans le fichier.
                unsafe { SetFilePointerEx(h.0, tail_start as i64, None, FILE_BEGIN) }.map_err(|e| probe_error(&e))?;
                let len = (f.size - tail_start) as usize;
                let n = read_block(&h, &mut buf[..len])?;
                hasher.update(&buf[..n]);
            }
        }
    }
    Ok(*hasher.finalize().as_bytes())
}
