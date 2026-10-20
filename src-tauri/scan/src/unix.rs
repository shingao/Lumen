//! Source portable (Linux), pour les tests et le développement : `lstat` uniquement, aucun
//! fichier ouvert. Sous Windows, c'est `scan::win::WinSource` qui sert.

use std::os::unix::fs::MetadataExt;

use crate::model::{Kind, RawEntry};
use crate::walk::{DirError, Source};

/// Secondes Unix → FILETIME (100 ns depuis 1601), comme les dates lues sous Windows.
pub fn filetime(secs: i64, nanos: i64) -> i64 {
    (secs + 11_644_473_600) * 10_000_000 + nanos / 100
}

fn io_error(e: &std::io::Error) -> DirError {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => DirError::AccessDenied,
        std::io::ErrorKind::NotFound => DirError::NotFound,
        _ => DirError::Other(e.raw_os_error().unwrap_or(-1)),
    }
}

fn entry(name: String, m: &std::fs::Metadata) -> RawEntry {
    let ft = m.file_type();
    let kind = if ft.is_symlink() {
        Kind::Link
    } else if ft.is_dir() {
        Kind::Dir
    } else {
        Kind::File
    };
    RawEntry {
        name,
        kind,
        attributes: 0,
        file_id: u128::from(m.ino()),
        logical: if kind == Kind::File { m.len() } else { 0 },
        allocated: if kind == Kind::File { m.blocks() * 512 } else { 0 },
        modified: filetime(m.mtime(), m.mtime_nsec()),
        accessed: filetime(m.atime(), m.atime_nsec()),
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct StdSource;

impl Source for StdSource {
    fn root(&self, path: &str) -> Result<RawEntry, DirError> {
        let m = std::fs::symlink_metadata(path).map_err(|e| io_error(&e))?;
        let name = std::path::Path::new(path).file_name().map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned());
        Ok(entry(name, &m))
    }

    fn read_dir(&self, path: &str, out: &mut Vec<RawEntry>) -> Result<(), DirError> {
        for item in std::fs::read_dir(path).map_err(|e| io_error(&e))? {
            let Ok(item) = item else { continue };
            // symlink_metadata : lstat, jamais d'ouverture du fichier ni de suivi de lien.
            if let Ok(m) = item.path().symlink_metadata() {
                out.push(entry(item.file_name().to_string_lossy().into_owned(), &m));
            }
        }
        Ok(())
    }

    fn join(&self, dir: &str, name: &str) -> String {
        if dir.ends_with('/') {
            format!("{dir}{name}")
        } else {
            format!("{dir}/{name}")
        }
    }

    fn volume_serial(&self, root: &str) -> u64 {
        std::fs::symlink_metadata(root).map_or(0, |m| m.dev())
    }

    fn last_access_reliable(&self, _root: &str) -> bool {
        false
    }
}
