//! Restauration depuis la Corbeille d'un élément envoyé par Lumen, d'après le journal.
//!
//! Windows range un élément supprimé en deux fichiers voisins de `<volume>\$Recycle.Bin\<SID>\` :
//! `$R…` (l'élément lui-même, simplement renommé, donc même file ID) et `$I…` (chemin d'origine,
//! taille, date). La restauration n'accepte qu'un `$R…` de ce dossier, dont le `$I…` désigne bien
//! le chemin d'origine du journal et dont le file ID est celui de l'élément supprimé ; elle le
//! renomme vers son emplacement d'origine sans jamais rien écraser, puis retire le `$I…`, comme le
//! fait l'Explorateur.

use windows::core::HSTRING;
use windows::Win32::Storage::FileSystem::{DeleteFileW, MoveFileExW, MOVEFILE_WRITE_THROUGH};

use super::WinProbe;
use crate::path::{fold, WinPath};
use crate::probe::{FileId, Probe, ProbeError};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RestoreError {
    #[error("emplacement dans la Corbeille invalide")]
    InvalidBinPath,
    #[error("l'élément n'est plus dans la Corbeille")]
    Gone,
    #[error("l'élément de la Corbeille ne correspond pas au journal")]
    Mismatch,
    #[error("un élément porte déjà ce nom à l'emplacement d'origine")]
    Occupied,
    #[error("le dossier d'origine n'existe plus")]
    ParentMissing,
    #[error("lien ou jonction sur le chemin d'origine : restauration refusée")]
    ReparsePoint,
    #[error("restauration impossible : {0}")]
    Io(String),
}

/// Chemins du `$R…` et du `$I…`, si `bin_path` est bien un élément de Corbeille sur ce volume.
fn bin_pair(bin_path: &str, original: &WinPath) -> Result<(WinPath, WinPath), RestoreError> {
    let r = WinPath::parse(bin_path).map_err(|_| RestoreError::InvalidBinPath)?;
    let comps = r.components();
    let in_bin = comps.len() == 3 && fold(&comps[0]) == "$RECYCLE.BIN" && comps[1].starts_with("S-") && comps[2].starts_with("$R");
    if !in_bin || r.volume_root() != original.volume_root() {
        return Err(RestoreError::InvalidBinPath);
    }
    let info_name = format!("$I{}", &comps[2][2..]);
    let parent = r.parent().ok_or(RestoreError::InvalidBinPath)?;
    let info = parent.join(&info_name).map_err(|_| RestoreError::InvalidBinPath)?;
    Ok((r, info))
}

/// Chemin d'origine inscrit dans un fichier `$I…` (formats 1 et 2 de Windows).
pub(crate) fn original_from_info(bytes: &[u8]) -> Option<String> {
    let version = i64::from_le_bytes(bytes.get(0..8)?.try_into().ok()?);
    let units: Vec<u16> = match version {
        2 => {
            let chars = u32::from_le_bytes(bytes.get(24..28)?.try_into().ok()?) as usize;
            bytes.get(28..28 + chars.checked_mul(2)?)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
        }
        1 => bytes.get(24..24 + 520)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect(),
        _ => return None,
    };
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    String::from_utf16(&units[..end]).ok()
}

fn verify(bin_path: &str, original: &WinPath, expected: FileId) -> Result<(WinPath, WinPath), RestoreError> {
    let (r, info) = bin_pair(bin_path, original)?;
    let facts = match WinProbe.facts(&r) {
        Ok(f) => f,
        Err(ProbeError::NotFound) => return Err(RestoreError::Gone),
        Err(e) => return Err(RestoreError::Io(e.to_string())),
    };
    if facts.file_id != expected {
        return Err(RestoreError::Mismatch);
    }
    let bytes = std::fs::read(info.verbatim()).map_err(|_| RestoreError::Gone)?;
    let recorded = original_from_info(&bytes).ok_or(RestoreError::Mismatch)?;
    if WinPath::parse(&recorded).ok().as_ref() != Some(original) {
        return Err(RestoreError::Mismatch);
    }
    Ok((r, info))
}

/// L'élément est-il encore dans la Corbeille, intact (même file ID, même chemin d'origine) ?
pub fn in_recycle_bin(bin_path: &str, original: &WinPath, expected: FileId) -> bool {
    verify(bin_path, original, expected).is_ok()
}

/// Remet l'élément à son emplacement d'origine. Rien n'est jamais écrasé.
pub fn restore(bin_path: &str, original: &WinPath, expected: FileId) -> Result<(), RestoreError> {
    let (r, info) = verify(bin_path, original, expected)?;
    let parent = original.parent().ok_or(RestoreError::ParentMissing)?;
    for p in parent.chain() {
        match WinProbe.attributes(&p) {
            Ok(a) if a.reparse_point => return Err(RestoreError::ReparsePoint),
            Ok(a) if !a.directory => return Err(RestoreError::ParentMissing),
            Ok(_) => {}
            Err(_) => return Err(RestoreError::ParentMissing),
        }
    }
    if WinProbe.attributes(original).is_ok() {
        return Err(RestoreError::Occupied);
    }
    // Même volume : simple renommage (même file ID), sans MOVEFILE_REPLACE_EXISTING.
    // SAFETY : chaînes terminées par un nul possédées pendant l'appel.
    unsafe { MoveFileExW(&HSTRING::from(r.verbatim()), &HSTRING::from(original.verbatim()), MOVEFILE_WRITE_THROUGH) }
        .map_err(|e| RestoreError::Io(e.message()))?;
    // L'élément est revenu : la fiche `$I…` de la Corbeille n'a plus d'objet.
    // SAFETY : idem.
    let _ = unsafe { DeleteFileW(&HSTRING::from(info.verbatim())) };
    Ok(())
}
