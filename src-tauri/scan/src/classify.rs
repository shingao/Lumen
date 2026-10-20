//! Classement d'une entrée d'après ses attributs et son étiquette de point d'analyse, commun au
//! parcours et à la MFT (logique pure) : les deux méthodes doivent donner exactement le même total.

use crate::model::Kind;

pub const ATTR_DIRECTORY: u32 = 0x10;
pub const ATTR_SPARSE: u32 = 0x200;
pub const ATTR_REPARSE: u32 = 0x400;
pub const ATTR_COMPRESSED: u32 = 0x800;
pub const ATTR_OFFLINE: u32 = 0x1000;
/// Dossier cloud incomplet : l'énumérer demanderait la liste au fournisseur (téléchargement).
pub const ATTR_RECALL_ON_OPEN: u32 = 0x4_0000;
/// Fichier cloud non téléchargé : lire son contenu le rapatrierait.
pub const ATTR_RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;

pub const TAG_MOUNT_POINT: u32 = 0xA000_0003;
pub const TAG_SYMLINK: u32 = 0xA000_000C;
pub const TAG_APPEXECLINK: u32 = 0x8000_001B;
pub const TAG_DEDUP: u32 = 0x8000_0013;
/// Compression de Windows (« Compact OS », WinSxS) : le contenu est dans un flux nommé.
pub const TAG_WOF: u32 = 0x8000_0017;
const TAG_NAME_SURROGATE: u32 = 0x2000_0000;

/// Le point d'analyse redirige vers un autre emplacement (jonction, lien, montage…).
pub fn is_link_tag(tag: u32) -> bool {
    tag & TAG_NAME_SURROGATE != 0 || tag == TAG_APPEXECLINK
}

pub fn classify(attributes: u32, tag: u32) -> Kind {
    let reparse = attributes & ATTR_REPARSE != 0;
    if reparse && is_link_tag(tag) {
        return Kind::Link;
    }
    if attributes & ATTR_DIRECTORY != 0 {
        if attributes & ATTR_RECALL_ON_OPEN != 0 {
            Kind::CloudDir
        } else {
            Kind::Dir
        }
    } else if attributes & (ATTR_RECALL_ON_DATA_ACCESS | ATTR_OFFLINE) != 0 {
        Kind::CloudFile
    } else {
        Kind::File
    }
}

/// La taille allouée de la liste de dossier ne suffit pas : fichier compressé, clairsemé, WOF ou
/// dédupliqué. Sa taille réelle sur disque se lit à part (`GetCompressedFileSizeW`, métadonnée).
pub fn needs_compressed_size(attributes: u32, tag: u32) -> bool {
    attributes & (ATTR_SPARSE | ATTR_COMPRESSED) != 0 || (attributes & ATTR_REPARSE != 0 && matches!(tag, TAG_WOF | TAG_DEDUP))
}

/// Taille allouée d'un fichier ordinaire d'après sa liste de dossier : une taille qui n'est pas un
/// multiple de la taille de cluster est celle d'un petit fichier résident dans la MFT, qui
/// n'occupe aucun cluster (« 0 octet sur le disque » dans l'Explorateur).
pub fn listed_allocation(allocation: u64, cluster: u64) -> u64 {
    if cluster > 0 && allocation % cluster != 0 {
        0
    } else {
        allocation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liens_non_traverses() {
        assert_eq!(classify(ATTR_DIRECTORY | ATTR_REPARSE, TAG_MOUNT_POINT), Kind::Link);
        assert_eq!(classify(ATTR_DIRECTORY | ATTR_REPARSE, TAG_SYMLINK), Kind::Link);
        assert_eq!(classify(ATTR_REPARSE, TAG_SYMLINK), Kind::Link);
        assert_eq!(classify(ATTR_REPARSE, TAG_APPEXECLINK), Kind::Link);
        assert_eq!(classify(ATTR_REPARSE, 0xA000_001D), Kind::Link, "lien WSL");
    }

    #[test]
    fn fichiers_compresses_restent_des_fichiers() {
        assert_eq!(classify(ATTR_REPARSE, TAG_WOF), Kind::File);
        assert_eq!(classify(ATTR_REPARSE, TAG_DEDUP), Kind::File);
        assert!(needs_compressed_size(ATTR_REPARSE, TAG_WOF));
        assert!(needs_compressed_size(ATTR_SPARSE, 0));
        assert!(needs_compressed_size(ATTR_COMPRESSED, 0));
        assert!(!needs_compressed_size(0, 0));
    }

    #[test]
    fn cloud_jamais_rapatrie() {
        // Dossier OneDrive peuplé : parcouru normalement ; incomplet : jamais énuméré.
        assert_eq!(classify(ATTR_DIRECTORY | ATTR_REPARSE, 0x9000_001A), Kind::Dir);
        assert_eq!(classify(ATTR_DIRECTORY | ATTR_REPARSE | ATTR_RECALL_ON_OPEN, 0x9000_301A), Kind::CloudDir);
        assert_eq!(classify(ATTR_REPARSE | ATTR_RECALL_ON_DATA_ACCESS, 0x9000_101A), Kind::CloudFile);
        assert_eq!(classify(ATTR_OFFLINE, 0), Kind::CloudFile);
        // Fichier cloud téléchargé (« toujours conserver ») : un fichier local comme un autre.
        assert_eq!(classify(ATTR_REPARSE, 0x9000_001A), Kind::File);
    }

    #[test]
    fn petits_fichiers_residents() {
        assert_eq!(listed_allocation(704, 4096), 0);
        assert_eq!(listed_allocation(8192, 4096), 8192);
        assert_eq!(listed_allocation(0, 4096), 0);
    }
}
