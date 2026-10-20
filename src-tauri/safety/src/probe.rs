//! Ce que `safety` a besoin de savoir du système de fichiers, derrière une interface.
//! L'implémentation réelle est `safety::win::WinProbe` (Win32) ; les tests portables utilisent
//! une sonde en mémoire pour vérifier la logique du pipeline.

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
}

/// Sonde en mémoire pour les tests portables de la logique (pas pour les tests Windows).
#[cfg(test)]
pub(crate) mod memory {
    use super::*;
    use crate::path::fold;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Debug, Clone)]
    pub struct Entry {
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
                attributes: Attributes { directory, reparse_point: target.is_some(), system: false },
                file_id: FileId { volume_serial: 0xC0DE, id: *id },
                size,
                modified: 133_000_000_000_000_000,
                contains_git: false,
            };
            let entry = Entry { facts, target: target.map(|t| WinPath::parse(t).unwrap()), locked: false };
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

        pub fn short_name(&mut self, short: &str, long: &str) {
            self.short_names.insert(fold(short), long.to_owned());
        }

        pub fn update(&self, path: &WinPath, f: impl FnOnce(&mut Entry)) {
            if let Some(e) = self.entries.borrow_mut().get_mut(&key(path)) {
                f(e);
            }
        }

        /// Remplace un fichier par un autre de même nom, même taille et même date (nouvel ID).
        pub fn replace(&self, path: &WinPath) {
            let mut id = self.next_id.borrow_mut();
            *id += 1;
            let new_id = *id;
            self.update(path, |e| e.facts.file_id.id = new_id);
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
    }
}
