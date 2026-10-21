//! Contrôle d'un élément avant toute action, dans l'ordre imposé :
//!
//! 1. canonisation ;
//! 2. liste blanche ;
//! 3. denylist ;
//! 4. liens (reparse points) sur le chemin et ses parents ;
//! 5. identité inchangée depuis le scan (anti-TOCTOU) ;
//! 6. fichier non verrouillé.
//!
//! Le premier échec arrête le contrôle. Un fichier verrouillé est ignoré et compté, pas refusé.

use crate::allow::{Allowlist, Authority};
use crate::bin::RecycleBlock;
use crate::denylist::{DenyList, DenyReason};
use crate::path::{PathError, WinPath};
use crate::probe::{Facts, FileId, Probe, ProbeError};

/// Ce que le scan a vu de l'élément : l'action est refusée si ça a changé depuis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Expected {
    pub file_id: FileId,
    pub directory: bool,
    pub size: u64,
    pub modified: i64,
}

impl Expected {
    pub fn from_facts(f: &Facts) -> Expected {
        Expected { file_id: f.file_id, directory: f.attributes.directory, size: f.size, modified: f.modified }
    }
}

/// Ce que le scan a vu dans un dossier, élément par élément : au moment de l'action, chaque
/// élément est recontrôlé contre cette liste, et tout ce qui a changé ou est apparu est laissé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedNode {
    pub name: String,
    pub expected: Expected,
    /// Espace occupé selon le scan (cumulé pour un dossier).
    pub bytes: u64,
    pub children: Vec<ExpectedNode>,
}

impl ExpectedNode {
    /// Nombre d'éléments, celui-ci compris.
    pub fn count(&self) -> u64 {
        1 + self.children.iter().map(ExpectedNode::count).sum::<u64>()
    }
}

/// Un élément à traiter, tel que le scan l'a enregistré. Le chemin vient du scan, jamais du front.
#[derive(Debug, Clone)]
pub struct Target {
    pub path: String,
    pub authority: Authority,
    pub expected: Expected,
    /// Espace occupé selon le scan (cumulé pour un dossier), pour les totaux affichés.
    pub bytes: u64,
    /// Dossier : son contenu tel que le scan l'a vu (obligatoire). Fichier : `None`.
    pub contents: Option<Vec<ExpectedNode>>,
    /// Doublon : BLAKE3 du contenu lu par la recherche, recalculé juste avant la suppression.
    pub hash: Option<[u8; 32]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Identity,
    Kind,
    Size,
    Modified,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("chemin invalide : {0}")]
    InvalidPath(#[from] PathError),
    #[error("élément introuvable")]
    NotFound,
    #[error("accès refusé")]
    AccessDenied,
    #[error("lecture impossible : {0}")]
    Probe(String),
    #[error("nom court 8.3 non résolu : « {0} »")]
    UnresolvedShortName(String),
    #[error("aucune règle ni sélection n'autorise cet élément")]
    NotAllowed,
    #[error("zone protégée : {0}")]
    Denied(DenyReason),
    #[error("lien ou jonction sur le chemin : {0}")]
    ReparsePoint(String),
    #[error("l'élément a changé depuis l'analyse ({0:?})")]
    Changed(Change),
    #[error("trop récent pour cette règle")]
    TooRecent,
    #[error("Corbeille indisponible : {0}")]
    NoRecycleBin(RecycleBlock),
    #[error("fichier dans le cloud : jamais touché")]
    Cloud,
    #[error("contenu du dossier inconnu : rien n'est fait sans la liste de l'analyse")]
    ContentsUnknown,
    #[error("dernier exemplaire de ce groupe de doublons : il en reste toujours au moins un")]
    LastCopy,
    #[error("même fichier que les autres exemplaires (lien physique) : le supprimer ne libère rien")]
    HardLink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Ouvert par un autre programme : ignoré, compté, réessayable plus tard.
    Locked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validated {
    /// Chemin réel (canonique), celui sur lequel on agit.
    pub path: WinPath,
    /// Chemin tel que demandé (peut passer par un lecteur SUBST, un nom 8.3…).
    pub requested: WinPath,
    pub facts: Facts,
    pub authority: Authority,
    pub bytes: u64,
    /// Identité vue par le scan, recontrôlée au moment de l'action.
    pub expected: Expected,
    /// Contenu attendu d'un dossier, recontrôlé élément par élément au moment de l'action.
    pub contents: Option<Vec<ExpectedNode>>,
    /// Doublon : empreinte du contenu, recalculée au moment de l'action.
    pub hash: Option<[u8; 32]>,
}

// Verdict éphémère, aussitôt consommé : la taille de `Accepted` est sans importance.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Accepted(Validated),
    Skipped { path: WinPath, reason: SkipReason },
    Refused { path: String, refusal: Refusal },
}

impl Verdict {
    pub fn refusal(&self) -> Option<&Refusal> {
        match self {
            Verdict::Refused { refusal, .. } => Some(refusal),
            _ => None,
        }
    }

    pub fn is_accepted(&self) -> bool {
        matches!(self, Verdict::Accepted(_))
    }
}

pub struct Checker<'a, P: Probe> {
    pub probe: &'a P,
    pub deny: &'a DenyList,
    pub allow: &'a Allowlist,
    /// Maintenant, en FILETIME (100 ns depuis 1601).
    pub now: i64,
}

const TICKS_PER_SECOND: i64 = 10_000_000;

fn probe_refusal(e: ProbeError) -> Refusal {
    match e {
        ProbeError::NotFound => Refusal::NotFound,
        ProbeError::AccessDenied => Refusal::AccessDenied,
        other => Refusal::Probe(other.to_string()),
    }
}

/// Alias 8.3 resté tel quel (ex. `PROGRA~1`) : Windows n'a pas pu le développer, on refuse.
fn looks_like_short_name(name: &str) -> bool {
    let (base, ext) = name.split_once('.').unwrap_or((name, ""));
    let Some((head, digits)) = base.rsplit_once('~') else { return false };
    !head.is_empty()
        && base.len() <= 8
        && ext.len() <= 3
        && !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii() && !c.is_ascii_lowercase())
}

impl<P: Probe> Checker<'_, P> {
    pub fn check(&self, target: &Target) -> Verdict {
        match self.run(target) {
            Ok(v) => v,
            Err(refusal) => Verdict::Refused { path: target.path.clone(), refusal },
        }
    }

    fn run(&self, target: &Target) -> Result<Verdict, Refusal> {
        // 1. Canonisation : forme lexicale stricte, puis chemin réel donné par Windows.
        let lexical = WinPath::parse(&target.path)?;
        let canonical = self.probe.canonicalize(&lexical).map_err(probe_refusal)?;
        if let Some(short) = canonical.components().iter().find(|c| looks_like_short_name(c)) {
            return Err(Refusal::UnresolvedShortName(short.clone()));
        }

        // 2. Liste blanche, sur le chemin réel. Un doublon doit être exactement l'exemplaire haché :
        //    même identité, même empreinte, et jamais un dossier.
        let permit = self.allow.permits(&canonical, &target.authority).ok_or(Refusal::NotAllowed)?;
        if let Authority::Duplicates(id) = &target.authority {
            let member = self.allow.duplicate_group(*id, &canonical).and_then(|g| g.members.iter().find(|m| m.path == canonical));
            let same = member.is_some_and(|m| Some(m.hash) == target.hash && m.expected == target.expected && !target.expected.directory);
            if !same {
                return Err(Refusal::NotAllowed);
            }
        }

        // 3. Denylist sur le chemin réel (le chemin demandé peut contenir des noms courts 8.3,
        //    qui fausseraient la comparaison), puis attributs (SYSTEM, dossier git).
        if let Some(reason) = self.deny.check(&canonical) {
            return Err(Refusal::Denied(reason));
        }
        let facts = self.probe.facts(&canonical).map_err(probe_refusal)?;
        if let Some(reason) =
            self.deny.check_facts(facts.attributes.system, facts.attributes.directory, facts.contains_git)
        {
            return Err(Refusal::Denied(reason));
        }

        // 4. Aucun lien : ni l'élément, ni un de ses parents (chemin demandé et chemin réel).
        for p in lexical.chain().iter().chain(canonical.chain().iter()) {
            let attrs = self.probe.attributes(p).map_err(probe_refusal)?;
            if attrs.reparse_point {
                return Err(Refusal::ReparsePoint(p.display_string()));
            }
        }
        if facts.attributes.cloud {
            return Err(Refusal::Cloud);
        }

        // 5. Anti-TOCTOU : même fichier qu'au scan.
        let e = &target.expected;
        if facts.file_id != e.file_id {
            return Err(Refusal::Changed(Change::Identity));
        }
        if facts.attributes.directory != e.directory {
            return Err(Refusal::Changed(Change::Kind));
        }
        // Un dossier change de date dès qu'un fichier y bouge : son contenu sera revérifié élément
        // par élément au moment de l'action. Un fichier doit être strictement identique.
        if e.directory && target.contents.is_none() {
            return Err(Refusal::ContentsUnknown);
        }
        if !e.directory {
            if facts.size != e.size {
                return Err(Refusal::Changed(Change::Size));
            }
            if facts.modified != e.modified {
                return Err(Refusal::Changed(Change::Modified));
            }
        }
        if let Some(min_age) = permit.min_age_secs {
            let age = self.now.saturating_sub(facts.modified);
            if age < (min_age as i64).saturating_mul(TICKS_PER_SECOND) {
                return Err(Refusal::TooRecent);
            }
        }

        // 6. Verrou : ignoré et compté, sans erreur.
        if self.probe.is_locked(&canonical).map_err(probe_refusal)? {
            return Ok(Verdict::Skipped { path: canonical, reason: SkipReason::Locked });
        }

        Ok(Verdict::Accepted(Validated {
            path: canonical,
            requested: lexical,
            facts,
            authority: target.authority.clone(),
            bytes: target.bytes,
            expected: target.expected,
            contents: target.contents.clone(),
            hash: target.hash,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allow::{RuleGrant, Scope, Selection};
    use crate::denylist::test_layout;
    use crate::probe::memory::MemoryProbe;

    const NOW: i64 = 133_000_000_000_000_000 + 100 * 3600 * TICKS_PER_SECOND;

    struct World {
        probe: MemoryProbe,
        deny: DenyList,
        allow: Allowlist,
    }

    impl World {
        fn new() -> World {
            let mut probe = MemoryProbe::new();
            for d in [
                "C:\\Users",
                "C:\\Users\\Arthur",
                "C:\\Users\\Arthur\\AppData",
                "C:\\Users\\Arthur\\AppData\\Local",
                "C:\\Users\\Arthur\\AppData\\Local\\npm-cache",
                "C:\\Users\\Arthur\\AppData\\Local\\Temp",
                "C:\\Windows",
                "C:\\Windows\\System32",
                "C:\\Program Files",
            ] {
                probe.dir(d);
            }
            probe.file("C:\\Windows\\System32\\kernel32.dll", 900);
            probe.file("C:\\Users\\Arthur\\AppData\\Local\\npm-cache\\index.json", 10);
            probe.short_name("PROGRA~1", "Program Files");
            let mut allow = Allowlist::new();
            allow.grant(RuleGrant {
                rule_id: "npm-cache".into(),
                root: WinPath::parse("C:\\Users\\Arthur\\AppData\\Local\\npm-cache").unwrap(),
                scope: Scope::Contents,
                min_age_secs: None,
            });
            allow.grant(RuleGrant {
                rule_id: "temp".into(),
                root: WinPath::parse("C:\\Users\\Arthur\\AppData\\Local\\Temp").unwrap(),
                scope: Scope::Contents,
                min_age_secs: Some(48 * 3600),
            });
            World { probe, deny: DenyList::new(&test_layout()), allow }
        }

        fn checker(&self) -> Checker<'_, MemoryProbe> {
            Checker { probe: &self.probe, deny: &self.deny, allow: &self.allow, now: NOW }
        }

        fn target(&self, raw: &str, rule: &str) -> Target {
            let path = WinPath::parse(raw).unwrap();
            let facts = self.probe.facts(&self.probe.canonicalize(&path).unwrap()).unwrap();
            Target { path: raw.into(), authority: Authority::Rule(rule.into()), expected: Expected::from_facts(&facts), bytes: facts.size, contents: None, hash: None }
        }

        fn verdict(&self, raw: &str, rule: &str) -> Verdict {
            self.checker().check(&self.target(raw, rule))
        }
    }

    const NPM: &str = "C:\\Users\\Arthur\\AppData\\Local\\npm-cache\\index.json";

    #[test]
    fn element_autorise_accepte() {
        let w = World::new();
        assert!(w.verdict(NPM, "npm-cache").is_accepted());
        assert!(w.verdict("c:/users/ARTHUR/appdata/local/npm-cache/INDEX.json", "npm-cache").is_accepted());
    }

    #[test]
    fn hors_liste_blanche_refuse() {
        let w = World::new();
        assert_eq!(w.verdict(NPM, "temp").refusal(), Some(&Refusal::NotAllowed));
        assert_eq!(w.verdict("C:\\Users\\Arthur\\AppData\\Local\\npm-cache", "npm-cache").refusal(), Some(&Refusal::NotAllowed));
    }

    #[test]
    fn chemins_relatifs_et_invalides_refuses() {
        let w = World::new();
        let mut t = w.target(NPM, "npm-cache");
        for raw in ["AppData\\Local\\npm-cache\\index.json", "..\\npm-cache\\index.json", "\\\\.\\C:\\x", "C:\\x\\NUL"] {
            t.path = raw.into();
            assert!(matches!(w.checker().check(&t).refusal(), Some(Refusal::InvalidPath(_))), "{raw}");
        }
    }

    #[test]
    fn nom_court_developpe_puis_denylist() {
        // Une règle mal écrite qui autoriserait C:\ tout entier ne passe pas pour autant.
        let mut w = World::new();
        w.allow.grant(RuleGrant {
            rule_id: "trop-large".into(),
            root: WinPath::parse("C:\\").unwrap(),
            scope: Scope::Contents,
            min_age_secs: None,
        });
        let mut t = w.target("C:\\Program Files", "trop-large");
        t.path = "C:\\PROGRA~1".into();
        assert_eq!(w.checker().check(&t).refusal(), Some(&Refusal::Denied(DenyReason::ProgramFiles)));
        t.path = "c:\\progra~1".into();
        assert_eq!(w.checker().check(&t).refusal(), Some(&Refusal::Denied(DenyReason::ProgramFiles)));
    }

    #[test]
    fn nom_court_non_resolu_refuse() {
        assert!(looks_like_short_name("PROGRA~1"));
        assert!(looks_like_short_name("DOCUM~12.TXT"));
        assert!(!looks_like_short_name("backup~1.txt"));
        assert!(!looks_like_short_name("Program Files"));
        assert!(!looks_like_short_name("~$rapport.docx"));
    }

    #[test]
    fn jonction_vers_windows_refusee() {
        let w = World::new();
        w.probe.junction("C:\\Users\\Arthur\\AppData\\Local\\npm-cache\\piege", "C:\\Windows");
        // La jonction elle-même : c'est un lien, refusée.
        let j = w.verdict("C:\\Users\\Arthur\\AppData\\Local\\npm-cache\\piege", "npm-cache");
        assert!(matches!(j.refusal(), Some(Refusal::ReparsePoint(_))), "{j:?}");
        // Un fichier à travers elle : le chemin réel est dans Windows, hors liste blanche.
        let through = w.verdict("C:\\Users\\Arthur\\AppData\\Local\\npm-cache\\piege\\System32\\kernel32.dll", "npm-cache");
        assert_eq!(through.refusal(), Some(&Refusal::NotAllowed));
    }

    #[test]
    fn denylist_prioritaire_sur_une_regle() {
        let mut w = World::new();
        w.allow.grant(RuleGrant {
            rule_id: "folle".into(),
            root: WinPath::parse("C:\\Windows").unwrap(),
            scope: Scope::Whole,
            min_age_secs: None,
        });
        let v = w.verdict("C:\\Windows\\System32\\kernel32.dll", "folle");
        assert_eq!(v.refusal(), Some(&Refusal::Denied(DenyReason::Windows)));
    }

    #[test]
    fn attribut_system_et_dossier_git() {
        let w = World::new();
        let p = WinPath::parse(NPM).unwrap();
        w.probe.update(&p, |e| e.facts.attributes.system = true);
        assert_eq!(w.verdict(NPM, "npm-cache").refusal(), Some(&Refusal::Denied(DenyReason::SystemAttribute)));

        let w = World::new();
        w.probe.dir("C:\\Users\\Arthur\\AppData\\Local\\npm-cache\\projet");
        w.probe.dir("C:\\Users\\Arthur\\AppData\\Local\\npm-cache\\projet\\.git");
        let v = w.verdict("C:\\Users\\Arthur\\AppData\\Local\\npm-cache\\projet", "npm-cache");
        assert_eq!(v.refusal(), Some(&Refusal::Denied(DenyReason::GitProject)));
    }

    #[test]
    fn toctou_fichier_remplace_ou_modifie() {
        let w = World::new();
        let t = w.target(NPM, "npm-cache");
        let p = WinPath::parse(NPM).unwrap();
        w.probe.replace(&p);
        assert_eq!(w.checker().check(&t).refusal(), Some(&Refusal::Changed(Change::Identity)));

        let w = World::new();
        let t = w.target(NPM, "npm-cache");
        w.probe.update(&p, |e| e.facts.size += 1);
        assert_eq!(w.checker().check(&t).refusal(), Some(&Refusal::Changed(Change::Size)));

        let w = World::new();
        let t = w.target(NPM, "npm-cache");
        w.probe.update(&p, |e| e.facts.modified += 1);
        assert_eq!(w.checker().check(&t).refusal(), Some(&Refusal::Changed(Change::Modified)));
    }

    #[test]
    fn age_minimum_de_la_regle() {
        let w = World::new();
        let recent = w.probe.file("C:\\Users\\Arthur\\AppData\\Local\\Temp\\recent.tmp", 5);
        w.probe.update(&recent, |e| e.facts.modified = NOW - 3600 * TICKS_PER_SECOND);
        assert_eq!(w.verdict("C:\\Users\\Arthur\\AppData\\Local\\Temp\\recent.tmp", "temp").refusal(), Some(&Refusal::TooRecent));
        // Créé par la sonde 100 h avant NOW : plus de 48 h.
        w.probe.file("C:\\Users\\Arthur\\AppData\\Local\\Temp\\vieux.tmp", 5);
        assert!(w.verdict("C:\\Users\\Arthur\\AppData\\Local\\Temp\\vieux.tmp", "temp").is_accepted());
    }

    #[test]
    fn fichier_verrouille_ignore_sans_erreur() {
        let w = World::new();
        w.probe.update(&WinPath::parse(NPM).unwrap(), |e| e.locked = true);
        assert!(matches!(w.verdict(NPM, "npm-cache"), Verdict::Skipped { reason: SkipReason::Locked, .. }));
    }

    #[test]
    fn introuvable_refuse() {
        let w = World::new();
        let mut t = w.target(NPM, "npm-cache");
        t.path = "C:\\Users\\Arthur\\AppData\\Local\\npm-cache\\absent.json".into();
        assert_eq!(w.checker().check(&t).refusal(), Some(&Refusal::NotFound));
    }

    #[test]
    fn selection_explicite() {
        let mut w = World::new();
        w.probe.dir("D:\\Vidéos");
        w.probe.file("D:\\Vidéos\\rush.mov", 4096);
        w.allow.add_selection(Selection { id: 3, members: vec![WinPath::parse("D:\\Vidéos\\rush.mov").unwrap()] });
        let path = WinPath::parse("D:\\Vidéos\\rush.mov").unwrap();
        let facts = w.probe.facts(&path).unwrap();
        let t = Target {
            path: "D:\\Vidéos\\rush.mov".into(),
            authority: Authority::Selection(3),
            expected: Expected::from_facts(&facts),
            bytes: facts.size,
            contents: None,
            hash: None,
        };
        assert!(w.checker().check(&t).is_accepted());
        let other = Target { authority: Authority::Selection(4), ..t };
        assert_eq!(w.checker().check(&other).refusal(), Some(&Refusal::NotAllowed));
    }
}
