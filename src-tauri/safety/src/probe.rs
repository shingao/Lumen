//! Ce que `safety` a besoin de savoir du système de fichiers, derrière une interface.
//! L'implémentation réelle est `safety::win::WinProbe` (Win32) ; les tests portables utilisent
//! une sonde en mémoire pour vérifier la logique du pipeline.

use crate::bin::VolumeFacts;
use crate::path::WinPath;

/// Identité NTFS d'un fichier : numéro de série du volume + file ID 128 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FileId {
    pub volume_serial: u64,
    pub id: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attributes {
    pub directory: bool,
    pub reparse_point: bool,
    pub system: bool,
    /// Fichier ou dossier dans le cloud (OneDrive…) : jamais ouvert, listé ni supprimé.
    pub cloud: bool,
}

/// État d'un élément lu sans suivre son dernier composant s'il est un lien.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    pub attributes: Attributes,
    pub file_id: FileId,
    pub size: u64,
    /// Dernière écriture, en intervalles de 100 ns depuis 1601 (FILETIME).
    pub modified: i64,
    /// Le dossier contient un `.git`.
    pub contains_git: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProbeError {
    #[error("introuvable")]
    NotFound,
    #[error("accès refusé")]
    AccessDenied,
    #[error("erreur système {0}")]
    Os(i32),
    #[error("chemin renvoyé par Windows illisible : {0}")]
    BadPath(String),
}

pub trait Probe {
    /// Chemin réel : noms courts 8.3 développés, casse réelle, parents liens résolus ;
    /// le dernier composant n'est pas suivi s'il est lui-même un lien.
    fn canonicalize(&self, path: &WinPath) -> Result<WinPath, ProbeError>;
    /// Attributs sans suivre le dernier composant.
    fn attributes(&self, path: &WinPath) -> Result<Attributes, ProbeError>;
    fn facts(&self, path: &WinPath) -> Result<Facts, ProbeError>;
    /// Un autre programme a l'élément ouvert.
    fn is_locked(&self, path: &WinPath) -> Result<bool, ProbeError>;
    /// Type de lecteur et état de la Corbeille du volume qui porte `path`.
    fn volume_facts(&self, path: &WinPath) -> Result<VolumeFacts, ProbeError>;
    /// Noms contenus dans le dossier, lus par un handle ouvert sur le dossier lui-même (jamais à
    /// travers un lien), avec son identité : l'appelant vérifie que c'est bien le dossier attendu.
    fn list(&self, dir: &WinPath) -> Result<Listing, ProbeError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub file_id: FileId,
    pub names: Vec<String>,
}

/// Sonde en mémoire pour les tests portables de la logique (pas pour les tests Windows).
#[cfg(test)]
pub(crate) mod memory {
    use super::*;
    use crate::bin::DriveKind;
    use crate::path::{fold, Prefix};
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Debug, Clone)]
    pub struct Entry {
        /// Chemin avec sa casse d'origine.
        pub path: WinPath,
        pub facts: Facts,
        /// Cible d'une jonction ou d'un lien symbolique.
        pub target: Option<WinPath>,
        pub locked: bool,
    }

    #[derive(Default)]
    pub struct MemoryProbe {
        entries: RefCell<HashMap<String, Entry>>,
        /// Alias 8.3 → nom long, par dossier parent.
        short_names: HashMap<String, String>,
        next_id: RefCell<u128>,
        /// État de volume par lettre de lecteur (disque fixe avec Corbeille par défaut).
        volumes: HashMap<char, VolumeFacts>,
    }

    fn key(p: &WinPath) -> String {
        fold(&p.display_string())
    }

    impl MemoryProbe {
        pub fn new() -> MemoryProbe {
            MemoryProbe { next_id: RefCell::new(1), ..Default::default() }
        }

        fn insert(&self, raw: &str, directory: bool, size: u64, target: Option<&str>) -> WinPath {
            let path = WinPath::parse(raw).unwrap();
            let mut id = self.next_id.borrow_mut();
            *id += 1;
            let facts = Facts {
                attributes: Attributes { directory, reparse_point: target.is_some(), system: false, cloud: false },
                file_id: FileId { volume_serial: 0xC0DE, id: *id },
                size,
                modified: 133_000_000_000_000_000,
                contains_git: false,
            };
            let entry = Entry { path: path.clone(), facts, target: target.map(|t| WinPath::parse(t).unwrap()), locked: false };
            self.entries.borrow_mut().insert(key(&path), entry);
            path
        }

        pub fn dir(&self, raw: &str) -> WinPath {
            self.insert(raw, true, 0, None)
        }

        pub fn file(&self, raw: &str, size: u64) -> WinPath {
            self.insert(raw, false, size, None)
        }

        pub fn junction(&self, raw: &str, target: &str) -> WinPath {
            self.insert(raw, true, 0, Some(target))
        }

        pub fn volume(&mut self, drive: char, facts: VolumeFacts) {
            self.volumes.insert(drive, facts);
        }

        pub fn short_name(&mut self, short: &str, long: &str) {
            self.short_names.insert(fold(short), long.to_owned());
        }

        pub fn update(&self, path: &WinPath, f: impl FnOnce(&mut Entry)) {
            if let Some(e) = self.entries.borrow_mut().get_mut(&key(path)) {
                f(e);
            }
        }

        /// Retire l'élément et tout ce qu'il contient (effet d'une suppression).
        pub fn remove(&self, path: &WinPath) {
            self.entries.borrow_mut().retain(|_, e| !e.path.starts_with(path));
        }

        pub fn exists(&self, raw: &str) -> bool {
            self.entries.borrow().contains_key(&key(&WinPath::parse(raw).unwrap()))
        }

        /// Remplace un fichier par un autre de même nom, même taille et même date (nouvel ID).
        pub fn replace(&self, path: &WinPath) {
            let mut id = self.next_id.borrow_mut();
            *id += 1;
            let new_id = *id;
            self.update(path, |e| e.facts.file_id.id = new_id);
        }

        /// Contenu d'un dossier tel qu'un scan le verrait maintenant (pour construire des cibles).
        pub fn tree(&self, dir: &WinPath) -> Vec<crate::check::ExpectedNode> {
            let Ok(listing) = self.list(dir) else { return Vec::new() };
            listing
                .names
                .iter()
                .map(|name| {
                    let path = dir.join(name).unwrap();
                    let facts = self.facts(&path).unwrap();
                    let children = if facts.attributes.directory && !facts.attributes.reparse_point { self.tree(&path) } else { Vec::new() };
                    let bytes = if facts.attributes.directory { children.iter().map(|c| c.bytes).sum() } else { facts.size };
                    crate::check::ExpectedNode { name: name.clone(), expected: crate::check::Expected::from_facts(&facts), bytes, children }
                })
                .collect()
        }

        /// Cible « telle que le scan l'aurait vue » maintenant, contenu compris pour un dossier.
        pub fn target(&self, raw: &str, authority: crate::allow::Authority) -> crate::check::Target {
            let path = WinPath::parse(raw).unwrap();
            let real = self.canonicalize(&path).unwrap();
            let facts = self.facts(&real).unwrap();
            let contents = (facts.attributes.directory && !facts.attributes.reparse_point).then(|| self.tree(&real));
            let bytes = contents.as_ref().map_or(facts.size, |c| c.iter().map(|n| n.bytes).sum());
            crate::check::Target { path: raw.into(), authority, expected: crate::check::Expected::from_facts(&facts), bytes, contents }
        }

        /// Résout les alias 8.3 et les liens des composants intermédiaires, comme Windows.
        fn resolve(&self, path: &WinPath) -> Result<WinPath, ProbeError> {
            let mut current = path.volume_root();
            let names = path.components().to_vec();
            for (i, name) in names.iter().enumerate() {
                let long = self.short_names.get(&fold(name)).cloned().unwrap_or_else(|| name.clone());
                let next = current.join(&long).map_err(|e| ProbeError::BadPath(e.to_string()))?;
                let last = i == names.len() - 1;
                let entry = self.entries.borrow().get(&key(&next)).cloned().ok_or(ProbeError::NotFound)?;
                current = match (&entry.target, last) {
                    (Some(target), false) => target.clone(),
                    _ => next,
                };
            }
            Ok(current)
        }
    }

    impl Probe for MemoryProbe {
        fn canonicalize(&self, path: &WinPath) -> Result<WinPath, ProbeError> {
            self.resolve(path)
        }

        fn attributes(&self, path: &WinPath) -> Result<Attributes, ProbeError> {
            let real = self.resolve(path)?;
            Ok(self.entries.borrow().get(&key(&real)).ok_or(ProbeError::NotFound)?.facts.attributes)
        }

        fn facts(&self, path: &WinPath) -> Result<Facts, ProbeError> {
            let real = self.resolve(path)?;
            let entries = self.entries.borrow();
            let entry = entries.get(&key(&real)).ok_or(ProbeError::NotFound)?;
            let mut facts = entry.facts;
            let git = real.join(".git").map(|g| entries.contains_key(&key(&g))).unwrap_or(false);
            facts.contains_git = facts.contains_git || git;
            Ok(facts)
        }

        fn is_locked(&self, path: &WinPath) -> Result<bool, ProbeError> {
            let real = self.resolve(path)?;
            Ok(self.entries.borrow().get(&key(&real)).ok_or(ProbeError::NotFound)?.locked)
        }

        fn list(&self, dir: &WinPath) -> Result<Listing, ProbeError> {
            let entries = self.entries.borrow();
            let entry = entries.get(&key(dir)).ok_or(ProbeError::NotFound)?;
            if !entry.facts.attributes.directory || entry.target.is_some() {
                return Err(ProbeError::Os(267));
            }
            let depth = dir.components().len() + 1;
            let mut names: Vec<String> = entries
                .values()
                .filter(|e| e.path.components().len() == depth && e.path.starts_with(dir))
                .filter_map(|e| e.path.components().last().cloned())
                .collect();
            names.sort();
            Ok(Listing { file_id: entry.facts.file_id, names })
        }

        fn volume_facts(&self, path: &WinPath) -> Result<VolumeFacts, ProbeError> {
            let fixed = VolumeFacts { kind: DriveKind::Fixed, subst: false, recycle_disabled: false, max_bytes: None };
            Ok(match path.prefix() {
                Prefix::Drive(d) => self.volumes.get(d).copied().unwrap_or(fixed),
                Prefix::Unc { .. } => VolumeFacts { kind: DriveKind::Remote, ..fixed },
            })
        }
    }
}
