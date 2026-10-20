//! Variables connues → chemins réels, et motifs de chemins (`*`, `?`, `**`) comparés sans tenir
//! compte de la casse, comme Windows.

use std::collections::HashMap;

use safety::path::{fold, Prefix, WinPath};

use crate::schema::{Target, Var};

/// Valeurs des variables sur la machine (dossiers connus de Windows, ou disposition de test).
#[derive(Debug, Clone, Default)]
pub struct Env {
    values: HashMap<Var, WinPath>,
}

impl Env {
    pub fn new() -> Env {
        Env::default()
    }

    pub fn set(&mut self, var: Var, path: WinPath) -> &mut Env {
        self.values.insert(var, path);
        self
    }

    pub fn get(&self, var: Var) -> Option<&WinPath> {
        self.values.get(&var)
    }

    /// Lettre du lecteur système (`%SYSTEMDRIVE%`).
    pub fn system_drive(&self) -> Option<char> {
        match self.get(Var::SystemDrive)?.prefix() {
            Prefix::Drive(d) => Some(*d),
            Prefix::Unc { .. } => None,
        }
    }

    /// Cible développée en motif absolu ; `None` si la variable n'est pas connue ici.
    pub fn expand(&self, target: &Target) -> Option<Pattern> {
        let base = self.get(target.var)?;
        let mut comps = path_components(base).into_iter().map(Comp::Lit).collect::<Vec<_>>();
        for c in &target.rest {
            comps.push(Comp::parse(c));
        }
        Some(Pattern { comps })
    }
}

/// Composants comparables d'un chemin : `C:` (ou `\\serveur\partage`) puis les noms, en
/// majuscules (`fold`).
pub fn path_components(p: &WinPath) -> Vec<String> {
    let head = match p.prefix() {
        Prefix::Drive(d) => format!("{d}:"),
        Prefix::Unc { server, share } => fold(&format!("\\\\{server}\\{share}")),
    };
    std::iter::once(head).chain(p.components().iter().map(|c| fold(c))).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comp {
    Lit(String),
    Glob(String),
    /// `**` : zéro composant ou plus.
    Any,
}

impl Comp {
    fn parse(c: &str) -> Comp {
        if c == "**" {
            Comp::Any
        } else if c.contains(['*', '?']) {
            Comp::Glob(fold(c))
        } else {
            Comp::Lit(fold(c))
        }
    }

    /// Le composant correspond-il à `name` (déjà en majuscules) ? `Any` est traité par l'automate.
    fn accepts(&self, name: &str) -> bool {
        match self {
            Comp::Lit(l) => l == name,
            Comp::Glob(g) => glob(g.as_bytes(), name.as_bytes()),
            Comp::Any => true,
        }
    }
}

/// `*` et `?` dans un nom (octets UTF-8 : suffisant pour des motifs ASCII).
pub fn glob(pattern: &[u8], name: &[u8]) -> bool {
    let (mut p, mut n) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            mark = n;
            p += 1;
        } else if let Some(s) = star {
            p = s + 1;
            mark += 1;
            n = mark;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

/// Motif absolu : suite de composants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    pub comps: Vec<Comp>,
}

/// États d'un automate de correspondance (indices dans le motif), toujours fermés sur `**`.
pub type States = Vec<u16>;

impl Pattern {
    fn close(&self, states: &mut States) {
        let mut i = 0;
        while i < states.len() {
            let s = states[i] as usize;
            if matches!(self.comps.get(s), Some(Comp::Any)) && !states.contains(&((s + 1) as u16)) {
                states.push((s + 1) as u16);
            }
            i += 1;
        }
        states.sort_unstable();
        states.dedup();
    }

    pub fn start(&self) -> States {
        let mut s = vec![0];
        self.close(&mut s);
        s
    }

    /// États après avoir lu le composant `name` (en majuscules).
    pub fn step(&self, states: &[u16], name: &str) -> States {
        let mut next = Vec::new();
        for &s in states {
            match self.comps.get(s as usize) {
                Some(Comp::Any) => next.push(s),
                Some(c) if c.accepts(name) => next.push(s + 1),
                _ => {}
            }
        }
        self.close(&mut next);
        next
    }

    /// Le motif est-il entièrement lu ?
    pub fn complete(&self, states: &[u16]) -> bool {
        states.contains(&(self.comps.len() as u16))
    }

    /// Préfixe fixe (avant tout joker) : utile pour vérifier la denylist sur la règle elle-même.
    pub fn fixed_prefix(&self) -> Vec<String> {
        self.comps
            .iter()
            .map_while(|c| match c {
                Comp::Lit(l) => Some(l.clone()),
                _ => None,
            })
            .collect()
    }

    /// Correspondance d'un chemin complet (composants en majuscules).
    pub fn matches(&self, comps: &[String]) -> bool {
        let mut s = self.start();
        for c in comps {
            s = self.step(&s, c);
            if s.is_empty() {
                return false;
            }
        }
        self.complete(&s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::parse_target;

    fn env() -> Env {
        let mut e = Env::new();
        e.set(Var::UserProfile, WinPath::parse("C:\\Users\\Arthur").unwrap());
        e.set(Var::LocalAppData, WinPath::parse("C:\\Users\\Arthur\\AppData\\Local").unwrap());
        e
    }

    fn comps(p: &str) -> Vec<String> {
        path_components(&WinPath::parse(p).unwrap())
    }

    #[test]
    fn jokers() {
        assert!(glob(b"*.TMP", b"A.TMP"));
        assert!(glob(b"*", b""));
        assert!(glob(b"A?C*", b"ABCDEF"));
        assert!(!glob(b"*.TMP", b"A.TMPX"));
        assert!(glob(b"*.*", b"X.Y"));
    }

    #[test]
    fn motifs_insensibles_a_la_casse() {
        let e = env();
        let t = parse_target("%LOCALAPPDATA%\\npm-cache", |_| true).unwrap();
        let p = e.expand(&t).unwrap();
        assert!(p.matches(&comps("c:\\users\\ARTHUR\\appdata\\local\\NPM-Cache")));
        assert!(!p.matches(&comps("C:\\Users\\Arthur\\AppData\\Local\\npm-cache\\x")));
        assert_eq!(p.fixed_prefix().len(), 6);

        let nm = e.expand(&parse_target("%USERPROFILE%\\**\\node_modules", |_| true).unwrap()).unwrap();
        assert!(nm.matches(&comps("C:\\Users\\Arthur\\node_modules")));
        assert!(nm.matches(&comps("C:\\Users\\Arthur\\Projets\\a\\b\\node_modules")));
        assert!(!nm.matches(&comps("C:\\Users\\Bob\\node_modules")));
        assert!(!nm.matches(&comps("C:\\Users\\Arthur\\Projets\\node_modules\\x")));
        // Variable absente de l'environnement : pas de motif.
        assert!(e.expand(&parse_target("%TEMP%", |_| true).unwrap()).is_none());
    }
}
