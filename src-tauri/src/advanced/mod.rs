//! Nettoyage avancé (jalon 6) : fichiers dormants, doublons, gros fichiers, gouffres système.

pub mod dormant;
pub mod duplicates;
pub mod files;
pub mod largest;
pub mod ipc;

#[cfg(test)]
mod saves_tests;
