//! Liste blanche (logique pure) : un chemin n'est jamais traité parce qu'il « a l'air » supprimable,
//! mais parce qu'une règle l'autorise explicitement, ou parce que l'utilisateur l'a coché dans une
//! sélection de fichiers (Dormants, Doublons).

use crate::check::Expected;
use crate::path::WinPath;

/// Ce qui autorise une action sur un élément.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Authority {
    /// Règle du moteur de règles (`rules.toml`).
    Rule(String),
    /// Sélection produite par Dormants ou Gros fichiers, cochée par l'utilisateur.
    Selection(u64),
    /// Exemplaires cochés dans des groupes de doublons : jamais tous ceux d'un groupe.
    Duplicates(u64),
}

/// Un exemplaire d'un groupe de doublons, tel qu'il a été lu et haché.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DupMember {
    pub path: WinPath,
    pub expected: Expected,
    /// BLAKE3 du contenu complet.
    pub hash: [u8; 32],
}

/// Fichiers au contenu identique ; `safety` en garde toujours au moins un intact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DupGroup {
    pub members: Vec<DupMember>,
}

#[derive(Debug, Clone)]
pub struct DuplicateSelection {
    pub id: u64,
    pub groups: Vec<DupGroup>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Le dossier de la règle lui-même, ou ce qu'il contient.
    Whole,
    /// Uniquement ce que contient le dossier (jamais le dossier lui-même).
    Contents,
}

/// Autorisation donnée par une règle, après expansion de ses chemins et globs par le moteur.
#[derive(Debug, Clone)]
pub struct RuleGrant {
    pub rule_id: String,
    pub root: WinPath,
    pub scope: Scope,
    /// Âge minimum (secondes depuis la dernière modification), ex. 48 h pour %TEMP%.
    pub min_age_secs: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub id: u64,
    /// Chemins exacts cochés par l'utilisateur.
    pub members: Vec<WinPath>,
}

#[derive(Debug, Clone, Default)]
pub struct Allowlist {
    grants: Vec<RuleGrant>,
    selections: Vec<Selection>,
    duplicates: Vec<DuplicateSelection>,
}

impl Allowlist {
    pub fn new() -> Allowlist {
        Allowlist::default()
    }

    pub fn grant(&mut self, grant: RuleGrant) {
        self.grants.push(grant);
    }

    pub fn add_selection(&mut self, selection: Selection) {
        self.selections.push(selection);
    }

    pub fn add_duplicates(&mut self, selection: DuplicateSelection) {
        self.duplicates.push(selection);
    }

    /// Groupe de doublons auquel appartient `path` dans la sélection `id`.
    pub fn duplicate_group(&self, id: u64, path: &WinPath) -> Option<&DupGroup> {
        self.duplicates.iter().find(|s| s.id == id)?.groups.iter().find(|g| g.members.iter().any(|m| &m.path == path))
    }

    /// L'autorisation qui couvre `path`, s'il y en a une.
    pub fn permits(&self, path: &WinPath, authority: &Authority) -> Option<Permit> {
        match authority {
            Authority::Rule(id) => self
                .grants
                .iter()
                .filter(|g| &g.rule_id == id)
                .find(|g| match g.scope {
                    Scope::Whole => path.starts_with(&g.root),
                    Scope::Contents => path.is_strictly_under(&g.root),
                })
                .map(|g| Permit { min_age_secs: g.min_age_secs }),
            Authority::Selection(id) => self
                .selections
                .iter()
                .find(|s| s.id == *id)
                .filter(|s| s.members.iter().any(|m| m == path))
                .map(|_| Permit { min_age_secs: None }),
            Authority::Duplicates(id) => self.duplicate_group(*id, path).map(|_| Permit { min_age_secs: None }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Permit {
    pub min_age_secs: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> WinPath {
        WinPath::parse(s).unwrap()
    }

    fn list() -> Allowlist {
        let mut a = Allowlist::new();
        a.grant(RuleGrant {
            rule_id: "npm-cache".into(),
            root: p("C:\\Users\\Arthur\\AppData\\Local\\npm-cache"),
            scope: Scope::Contents,
            min_age_secs: None,
        });
        a.grant(RuleGrant {
            rule_id: "build".into(),
            root: p("C:\\Users\\Arthur\\Projets\\app\\node_modules"),
            scope: Scope::Whole,
            min_age_secs: None,
        });
        a.grant(RuleGrant {
            rule_id: "temp".into(),
            root: p("C:\\Users\\Arthur\\AppData\\Local\\Temp"),
            scope: Scope::Contents,
            min_age_secs: Some(48 * 3600),
        });
        a.add_selection(Selection { id: 7, members: vec![p("D:\\Vidéos\\rush.mov")] });
        a
    }

    #[test]
    fn regle_contenu_seulement() {
        let a = list();
        let npm = Authority::Rule("npm-cache".into());
        assert!(a.permits(&p("C:\\Users\\Arthur\\AppData\\Local\\npm-cache\\_cacache"), &npm).is_some());
        assert!(a.permits(&p("c:\\users\\arthur\\appdata\\local\\NPM-CACHE\\x"), &npm).is_some());
        assert!(a.permits(&p("C:\\Users\\Arthur\\AppData\\Local\\npm-cache"), &npm).is_none());
        assert!(a.permits(&p("C:\\Users\\Arthur\\AppData\\Local\\npm-cache2\\x"), &npm).is_none());
        assert!(a.permits(&p("C:\\Users\\Arthur\\AppData\\Local\\npm-cache\\..\\Google"), &npm).is_none());
    }

    #[test]
    fn regle_dossier_entier_et_mauvaise_regle() {
        let a = list();
        let build = Authority::Rule("build".into());
        assert!(a.permits(&p("C:\\Users\\Arthur\\Projets\\app\\node_modules"), &build).is_some());
        assert!(a.permits(&p("C:\\Users\\Arthur\\Projets\\app\\src"), &build).is_none());
        // Une règle n'autorise que ses propres chemins.
        assert!(a.permits(&p("C:\\Users\\Arthur\\Projets\\app\\node_modules"), &Authority::Rule("npm-cache".into())).is_none());
        assert!(a.permits(&p("C:\\x"), &Authority::Rule("inconnue".into())).is_none());
    }

    #[test]
    fn age_minimum_transmis() {
        let a = list();
        let permit = a.permits(&p("C:\\Users\\Arthur\\AppData\\Local\\Temp\\a.tmp"), &Authority::Rule("temp".into()));
        assert_eq!(permit, Some(Permit { min_age_secs: Some(48 * 3600) }));
    }

    #[test]
    fn selection_exacte_uniquement() {
        let a = list();
        assert!(a.permits(&p("D:\\vidéos\\RUSH.mov"), &Authority::Selection(7)).is_some());
        assert!(a.permits(&p("D:\\Vidéos"), &Authority::Selection(7)).is_none());
        assert!(a.permits(&p("D:\\Vidéos\\rush.mov\\x"), &Authority::Selection(7)).is_none());
        assert!(a.permits(&p("D:\\Vidéos\\rush.mov"), &Authority::Selection(8)).is_none());
    }
}
