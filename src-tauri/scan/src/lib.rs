//! `scan` — analyse du disque de Lumen.
//!
//! - Métadonnées uniquement : aucun fichier n'est ouvert pour être lu ; les fichiers cloud non
//!   téléchargés ne sont jamais rapatriés.
//! - [`model`] : arène compacte, liens physiques comptés une fois (file ID), agrégation, tranches
//!   « top N + autres » pour le front.
//! - [`walk`] : parcours parallèle, progression ~10/s, annulation.
//! - `win` : source Windows (liste de dossier avec file ID et taille allouée), lecture de la MFT
//!   (droits administrateur), volumes, élévation.

pub mod classify;
pub mod codec;
pub mod kinds;
pub mod model;
pub mod search;
pub mod walk;

#[cfg(unix)]
pub mod unix;
#[cfg(windows)]
pub mod win;

pub use model::{Kind, Method, NodeId, RawEntry, Scan, SliceNode, Stats};
pub use walk::{walk, DirError, Progress, ScanError, Source, TopProgress};

#[cfg(test)]
#[cfg(unix)]
mod tests;
