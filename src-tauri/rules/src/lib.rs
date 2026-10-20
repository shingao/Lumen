//! `rules` — moteur de règles de Lumen, en données (`rules.toml`, embarqué).
//!
//! - [`schema`] : schéma strict, validé au chargement (variables connues seulement, aucun chemin
//!   absolu en dur, commandes de la liste fermée de `safety`).
//! - [`pattern`] : variables → chemins réels, motifs `*` / `**`.
//! - [`engine`] : application à une analyse — éléments couverts (jamais dans la denylist),
//!   statuts, catégories, gains, recommandations, projets actifs ou inactifs.
//! - [`git`] : date du dernier commit lue dans `.git`, sans lancer git.
//!
//! Rien ici ne supprime ni ne lance quoi que ce soit : `safety` décide et exécute (jalon 5).

pub mod engine;
pub mod fsread;
pub mod git;
pub mod pattern;
pub mod schema;

pub use engine::{analyze, Analysis, Context, Reco};
pub use pattern::Env;
pub use schema::{load, Category, RuleSet, Status};

/// Règles livrées avec Lumen.
pub const BUILTIN: &str = include_str!("../rules.toml");

/// Règles livrées, validées.
pub fn builtin() -> Result<RuleSet, schema::LoadError> {
    load(BUILTIN)
}

#[cfg(test)]
mod tests;
