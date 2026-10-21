//! `safety` — le seul endroit de Lumen qui décide si un élément peut être supprimé, déplacé ou
//! confié à une commande officielle.
//!
//! - Logique pure, testable partout : [`path`] (canonisation lexicale), [`denylist`], [`allow`]
//!   (liste blanche), [`check`] (pipeline dans l'ordre imposé), [`bin`] (pré-contrôle de la
//!   Corbeille), [`plan`] (plan, simulation, empreinte, confirmation), [`exec`] (recontrôle
//!   complet et exécution), [`script`] (PowerShell autonome), [`commands`] (liste fermée,
//!   résolution des binaires).
//! - Appels Win32, testés sur Windows uniquement : `win` (sonde réelle, dossiers connus,
//!   Corbeille via `IFileOperation`, suppression définitive par handle, commandes officielles,
//!   restauration depuis la Corbeille).
//!
//! Le front n'envoie jamais de chemin libre : les cibles viennent du scan (jalon 3), les
//! autorisations des règles (jalon 4) ou des sélections de l'utilisateur. Une exécution réelle
//! passe uniquement par [`plan::execute`] : confirmation de l'empreinte, puis recontrôle de
//! chaque élément, contenu des dossiers compris, au moment de l'action.

pub mod allow;
pub mod bin;
pub mod check;
pub mod commands;
pub mod denylist;
pub mod exec;
pub mod guard;
pub mod launch;
pub mod path;
pub mod plan;
pub mod probe;
pub mod script;

#[cfg(windows)]
pub mod win;
