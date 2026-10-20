//! Lectures de fichiers du moteur de règles (`.git`, `.gitignore`) sans changer leur date de
//! dernier accès : sous Windows, le handle est ouvert avec `FILE_WRITE_ATTRIBUTES` et la date est
//! gelée (`SetFileTime` à `0xFFFFFFFF`) avant toute lecture ; repli en lecture simple si ce droit
//! est refusé. Les fichiers dormants du jalon 6 reposent sur cette date.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

#[cfg(windows)]
mod win {
    #![allow(unsafe_code)]

    use std::fs::{File, OpenOptions};
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    use windows::Win32::Foundation::{FILETIME, HANDLE};
    use windows::Win32::Storage::FileSystem::{SetFileTime, FILE_GENERIC_READ, FILE_WRITE_ATTRIBUTES};

    pub fn open_frozen(path: &Path) -> Option<File> {
        let f = OpenOptions::new().access_mode(FILE_GENERIC_READ.0 | FILE_WRITE_ATTRIBUTES.0).share_mode(7).open(path).ok()?;
        let frozen = FILETIME { dwLowDateTime: u32::MAX, dwHighDateTime: u32::MAX };
        // SAFETY : handle valide ouvert avec FILE_WRITE_ATTRIBUTES ; aucune date n'est modifiée.
        unsafe { SetFileTime(HANDLE(f.as_raw_handle()), None, Some(&frozen), None) }.ok()?;
        Some(f)
    }
}

/// Ouvre un fichier en lecture, date de dernier accès gelée quand c'est possible.
pub fn open(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    if let Some(f) = win::open_frozen(path) {
        return Ok(f);
    }
    File::open(path)
}

/// Lit au plus `max` octets d'un fichier (petits fichiers : HEAD, refs, `.gitignore`…).
pub fn read_limited(path: &Path, max: u64) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    open(path)?.take(max).read_to_end(&mut out)?;
    Ok(out)
}

/// Lit jusqu'à `len` octets à partir de `offset`.
pub fn read_at(file: &mut File, offset: u64, len: usize) -> io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut out = Vec::with_capacity(len.min(1 << 20));
    file.take(len as u64).read_to_end(&mut out)?;
    Ok(out)
}
