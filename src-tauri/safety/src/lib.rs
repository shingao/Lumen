//! `safety` — le seul endroit de Lumen qui décide si un élément peut être supprimé, déplacé ou
//! confié à une commande officielle.
//!
//! - Logique pure, testable partout : [`path`] (canonisation lexicale), [`denylist`], [`allow`]
//!   (liste blanche), [`check`] (pipeline dans l'ordre imposé), [`plan`] (plan, simulation,
//!   empreinte, confirmation), [`script`] (PowerShell), [`commands`] (liste fermée).
//! - Appels Win32, testés sur Windows uniquement : `win` (sonde réelle, dossiers connus,
//!   Corbeille via `IFileOperation`).
//!
//! Le front n'envoie jamais de chemin libre : les cibles viennent du scan (jalon 3), les
//! autorisations des règles (jalon 4) ou des sélections de l'utilisateur. Au jalon 2, seule la
//! simulation est exposée : [`plan::execute`] refuse toute exécution réelle.

pub mod allow;
pub mod check;
pub mod commands;
pub mod denylist;
pub mod path;
pub mod plan;
pub mod probe;
pub mod script;

#[cfg(windows)]
pub mod win;
