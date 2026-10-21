//! Accès au registre, en lecture seule, derrière un trait : l'appli le branche sur
//! `scan::win::registry` (vues 64 et 32 bits fusionnées), les tests sur un registre en mémoire.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Hive {
    LocalMachine,
    CurrentUser,
}

pub trait Registry {
    /// Noms des sous-clés directes (vide si la clé n'existe pas).
    fn subkeys(&self, hive: Hive, path: &str) -> Vec<String>;
    fn string(&self, hive: Hive, path: &str, name: &str) -> Option<String>;
    fn dword(&self, hive: Hive, path: &str, name: &str) -> Option<u32>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Data {
    Str(String),
    Dword(u32),
}

/// Registre en mémoire (tests, mode navigateur) ; clés et noms sans tenir compte de la casse.
#[derive(Debug, Clone, Default)]
pub struct MemRegistry {
    values: BTreeMap<(Hive, String, String), Data>,
}

fn k(s: &str) -> String {
    s.trim_matches('\\').to_lowercase()
}

impl MemRegistry {
    pub fn new() -> MemRegistry {
        MemRegistry::default()
    }

    pub fn set_str(&mut self, hive: Hive, path: &str, name: &str, value: &str) -> &mut MemRegistry {
        self.values.insert((hive, k(path), k(name)), Data::Str(value.to_owned()));
        self
    }

    pub fn set_dword(&mut self, hive: Hive, path: &str, name: &str, value: u32) -> &mut MemRegistry {
        self.values.insert((hive, k(path), k(name)), Data::Dword(value));
        self
    }
}

impl Registry for MemRegistry {
    fn subkeys(&self, hive: Hive, path: &str) -> Vec<String> {
        let prefix = format!("{}\\", k(path));
        let mut out: Vec<String> = self
            .values
            .keys()
            .filter(|(h, p, _)| *h == hive && p.starts_with(&prefix))
            .filter_map(|(_, p, _)| p[prefix.len()..].split('\\').next().map(str::to_owned))
            .collect();
        out.dedup();
        out
    }

    fn string(&self, hive: Hive, path: &str, name: &str) -> Option<String> {
        match self.values.get(&(hive, k(path), k(name)))? {
            Data::Str(s) => Some(s.clone()),
            Data::Dword(_) => None,
        }
    }

    fn dword(&self, hive: Hive, path: &str, name: &str) -> Option<u32> {
        match self.values.get(&(hive, k(path), k(name)))? {
            Data::Dword(v) => Some(*v),
            Data::Str(_) => None,
        }
    }
}
