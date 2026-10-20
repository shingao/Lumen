//! Pré-contrôle de la Corbeille (logique pure) : avant tout envoi à la Corbeille, on vérifie que le
//! volume en a une et qu'elle acceptera l'élément. Sinon l'élément est refusé, jamais détruit.
//!
//! Windows supprime définitivement, sans Corbeille, sur les lecteurs réseau, amovibles et SUBST,
//! quand la Corbeille est désactivée pour le volume (« Ne pas déplacer les fichiers vers la
//! Corbeille ») ou par stratégie, et quand l'élément dépasse la taille maximale de la Corbeille.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    Fixed,
    Removable,
    Remote,
    CdRom,
    RamDisk,
    Unknown,
}

/// État du volume, lu par la sonde (`GetDriveTypeW`, `QueryDosDeviceW`, registre BitBucket).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeFacts {
    pub kind: DriveKind,
    /// Lecteur créé par SUBST (alias d'un dossier) : pas de Corbeille propre.
    pub subst: bool,
    /// Corbeille désactivée pour ce volume (NukeOnDelete) ou par stratégie (NoRecycleFiles).
    pub recycle_disabled: bool,
    /// Taille maximale de la Corbeille du volume, si elle est configurée.
    pub max_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RecycleBlock {
    #[error("lecteur réseau : pas de Corbeille")]
    Network,
    #[error("lecteur amovible ou non fixe : pas de Corbeille")]
    NotFixed(DriveKind),
    #[error("lecteur SUBST : pas de Corbeille")]
    Subst,
    #[error("Corbeille désactivée pour ce volume")]
    Disabled,
    #[error("trop gros pour la Corbeille de ce volume")]
    TooLarge { bytes: u64, max: u64 },
}

/// L'élément peut-il aller à la Corbeille sans risque de destruction silencieuse ?
pub fn recycle_allowed(facts: &VolumeFacts, bytes: u64) -> Result<(), RecycleBlock> {
    match facts.kind {
        DriveKind::Fixed => {}
        DriveKind::Remote => return Err(RecycleBlock::Network),
        other => return Err(RecycleBlock::NotFixed(other)),
    }
    if facts.subst {
        return Err(RecycleBlock::Subst);
    }
    if facts.recycle_disabled {
        return Err(RecycleBlock::Disabled);
    }
    if let Some(max) = facts.max_bytes {
        if bytes > max {
            return Err(RecycleBlock::TooLarge { bytes, max });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXED: VolumeFacts = VolumeFacts { kind: DriveKind::Fixed, subst: false, recycle_disabled: false, max_bytes: None };

    #[test]
    fn disque_fixe_accepte() {
        assert_eq!(recycle_allowed(&FIXED, u64::MAX), Ok(()));
    }

    #[test]
    fn lecteurs_sans_corbeille_refuses() {
        assert_eq!(recycle_allowed(&VolumeFacts { kind: DriveKind::Remote, ..FIXED }, 1), Err(RecycleBlock::Network));
        for kind in [DriveKind::Removable, DriveKind::CdRom, DriveKind::RamDisk, DriveKind::Unknown] {
            assert_eq!(recycle_allowed(&VolumeFacts { kind, ..FIXED }, 1), Err(RecycleBlock::NotFixed(kind)));
        }
        assert_eq!(recycle_allowed(&VolumeFacts { subst: true, ..FIXED }, 1), Err(RecycleBlock::Subst));
        assert_eq!(recycle_allowed(&VolumeFacts { recycle_disabled: true, ..FIXED }, 1), Err(RecycleBlock::Disabled));
    }

    #[test]
    fn quota_de_la_corbeille() {
        let small = VolumeFacts { max_bytes: Some(1024), ..FIXED };
        assert_eq!(recycle_allowed(&small, 1024), Ok(()));
        assert_eq!(recycle_allowed(&small, 1025), Err(RecycleBlock::TooLarge { bytes: 1025, max: 1024 }));
    }
}
