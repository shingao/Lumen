//! Chemins Windows absolus, normalisés lexicalement (logique pure, testable partout).
//!
//! Tout ce qui n'est pas un chemin absolu ordinaire est refusé : chemins relatifs, chemins de
//! périphérique (`\\.\`, `\??\`, `\\?\Volume{…}`, `\\?\GLOBALROOT`), flux de données alternatifs,
//! noms réservés (`CON`, `NUL`…), composants terminés par un point ou une espace (Win32 les
//! retire en silence : `C:\Windows.` désignerait `C:\Windows`).
//! Les `..` sont résolus lexicalement, y compris après `\\?\` ; sortir de la racine est refusé.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    #[error("chemin vide")]
    Empty,
    #[error("chemin relatif : seuls les chemins absolus sont acceptés")]
    Relative,
    #[error("chemin de périphérique ou de volume brut refusé")]
    Device,
    #[error("le chemin sort de la racine du volume (« .. » en trop)")]
    EscapesRoot,
    #[error("caractère interdit dans « {0} »")]
    InvalidChar(String),
    #[error("nom réservé par Windows : « {0} »")]
    ReservedName(String),
    #[error("nom terminé par un point ou une espace : « {0} »")]
    TrailingDotOrSpace(String),
    #[error("flux de données alternatif refusé : « {0} »")]
    AlternateStream(String),
    #[error("nom trop long (plus de 255 caractères)")]
    ComponentTooLong,
    #[error("partage réseau incomplet")]
    BadUnc,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Prefix {
    /// Lettre de lecteur, en majuscule.
    Drive(char),
    /// `\\serveur\partage`.
    Unc { server: String, share: String },
}

/// Chemin absolu normalisé. Les comparaisons ignorent la casse (repli de casse simple, comme NTFS).
#[derive(Debug, Clone)]
pub struct WinPath {
    prefix: Prefix,
    components: Vec<String>,
}

/// Majuscule simple, caractère par caractère (table de NTFS : jamais de « ß » → « SS »).
pub fn fold(s: &str) -> String {
    s.chars()
        .map(|c| {
            let mut up = c.to_uppercase();
            match (up.next(), up.next()) {
                (Some(u), None) => u,
                _ => c,
            }
        })
        .collect()
}

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
    "COM7", "COM8", "COM9", "COM¹", "COM²", "COM³", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6",
    "LPT7", "LPT8", "LPT9", "LPT¹", "LPT²", "LPT³",
];

fn check_component(c: &str) -> Result<(), PathError> {
    if c.chars().count() > 255 {
        return Err(PathError::ComponentTooLong);
    }
    if c.contains(':') {
        return Err(PathError::AlternateStream(c.to_owned()));
    }
    if c.chars().any(|ch| (ch as u32) < 0x20 || matches!(ch, '<' | '>' | '"' | '|' | '?' | '*')) {
        return Err(PathError::InvalidChar(c.to_owned()));
    }
    if c.ends_with('.') || c.ends_with(' ') {
        return Err(PathError::TrailingDotOrSpace(c.to_owned()));
    }
    let base = fold(c.split('.').next().unwrap_or(c).trim_end());
    if RESERVED.contains(&base.as_str()) {
        return Err(PathError::ReservedName(c.to_owned()));
    }
    Ok(())
}

impl WinPath {
    pub fn parse(raw: &str) -> Result<WinPath, PathError> {
        if raw.is_empty() {
            return Err(PathError::Empty);
        }
        let s = raw.replace('/', "\\");
        let upper = fold(&s);

        // Préfixes « verbatim » et de périphérique.
        let rest: &str;
        let prefix: Prefix;
        if upper.starts_with("\\\\.\\") || upper.starts_with("\\??\\") {
            return Err(PathError::Device);
        }
        let (body, verbatim) = match s.strip_prefix("\\\\?\\") {
            Some(b) => (b, true),
            None => (s.as_str(), false),
        };
        let body_upper = fold(body);
        if verbatim && (body_upper.starts_with("VOLUME{") || body_upper.starts_with("GLOBALROOT")) {
            return Err(PathError::Device);
        }
        if verbatim && body_upper.starts_with("UNC\\") {
            let (server, share, tail) = split_unc(&body[4..])?;
            prefix = Prefix::Unc { server, share };
            rest = tail;
        } else if !verbatim && body.starts_with("\\\\") {
            let (server, share, tail) = split_unc(&body[2..])?;
            prefix = Prefix::Unc { server, share };
            rest = tail;
        } else {
            let mut chars = body.chars();
            match (chars.next(), chars.next(), chars.next()) {
                (Some(d), Some(':'), Some('\\')) if d.is_ascii_alphabetic() => {
                    prefix = Prefix::Drive(d.to_ascii_uppercase());
                    rest = &body[3..];
                }
                (Some(d), Some(':'), None) if d.is_ascii_alphabetic() => {
                    // « C: » seul désigne le dossier courant du lecteur, pas sa racine.
                    return Err(PathError::Relative);
                }
                _ => {
                    if verbatim {
                        return Err(PathError::Device);
                    }
                    return Err(PathError::Relative);
                }
            }
        }

        let mut components: Vec<String> = Vec::new();
        for part in rest.split('\\') {
            match part {
                "" | "." => {}
                ".." => {
                    if components.pop().is_none() {
                        return Err(PathError::EscapesRoot);
                    }
                }
                c => {
                    check_component(c)?;
                    components.push(c.to_owned());
                }
            }
        }
        Ok(WinPath { prefix, components })
    }

    pub fn prefix(&self) -> &Prefix {
        &self.prefix
    }

    pub fn components(&self) -> &[String] {
        &self.components
    }

    /// Nombre de composants sous la racine du volume (0 = racine).
    pub fn depth(&self) -> usize {
        self.components.len()
    }

    pub fn is_volume_root(&self) -> bool {
        self.components.is_empty()
    }

    pub fn file_name(&self) -> Option<&str> {
        self.components.last().map(String::as_str)
    }

    pub fn volume_root(&self) -> WinPath {
        WinPath { prefix: self.prefix.clone(), components: Vec::new() }
    }

    pub fn parent(&self) -> Option<WinPath> {
        if self.components.is_empty() {
            return None;
        }
        let mut p = self.clone();
        p.components.pop();
        Some(p)
    }

    /// Ce chemin et chacun de ses parents, de la racine du volume (exclue) jusqu'à lui.
    pub fn chain(&self) -> Vec<WinPath> {
        (1..=self.components.len())
            .map(|n| WinPath { prefix: self.prefix.clone(), components: self.components[..n].to_vec() })
            .collect()
    }

    pub fn join(&self, name: &str) -> Result<WinPath, PathError> {
        WinPath::parse(&format!("{}\\{}", self.display_string(), name))
    }

    fn same_prefix(&self, other: &WinPath) -> bool {
        match (&self.prefix, &other.prefix) {
            (Prefix::Drive(a), Prefix::Drive(b)) => a == b,
            (Prefix::Unc { server: s1, share: h1 }, Prefix::Unc { server: s2, share: h2 }) => {
                fold(s1) == fold(s2) && fold(h1) == fold(h2)
            }
            _ => false,
        }
    }

    /// `self` est `other` ou se trouve dessous.
    pub fn starts_with(&self, other: &WinPath) -> bool {
        self.same_prefix(other)
            && self.components.len() >= other.components.len()
            && self.components.iter().zip(&other.components).all(|(a, b)| fold(a) == fold(b))
    }

    /// `self` se trouve strictement sous `other`.
    pub fn is_strictly_under(&self, other: &WinPath) -> bool {
        self.starts_with(other) && self.components.len() > other.components.len()
    }

    /// Forme affichée : `C:\dossier\fichier`.
    pub fn display_string(&self) -> String {
        let root = match &self.prefix {
            Prefix::Drive(d) => format!("{d}:\\"),
            Prefix::Unc { server, share } => format!("\\\\{server}\\{share}\\"),
        };
        root + &self.components.join("\\")
    }

    /// Forme `\\?\` pour les API Win32 : pas de limite de 260 caractères, pas de réinterprétation.
    pub fn verbatim(&self) -> String {
        match &self.prefix {
            Prefix::Drive(d) => format!("\\\\?\\{d}:\\{}", self.components.join("\\")),
            Prefix::Unc { server, share } => {
                format!("\\\\?\\UNC\\{server}\\{share}\\{}", self.components.join("\\"))
            }
        }
    }
}

fn split_unc(s: &str) -> Result<(String, String, &str), PathError> {
    let mut it = s.splitn(3, '\\');
    let server = it.next().unwrap_or("");
    let share = it.next().unwrap_or("");
    let tail = it.next().unwrap_or("");
    if server.is_empty() || share.is_empty() || server == "." || server == "?" {
        return Err(PathError::BadUnc);
    }
    check_component(server)?;
    check_component(share)?;
    Ok((server.to_owned(), share.to_owned(), tail))
}

impl PartialEq for WinPath {
    fn eq(&self, other: &Self) -> bool {
        self.starts_with(other) && self.components.len() == other.components.len()
    }
}

impl Eq for WinPath {}

impl fmt::Display for WinPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> WinPath {
        WinPath::parse(s).unwrap()
    }

    #[test]
    fn normalise_separateurs_points_et_doublons() {
        assert_eq!(p("C:/Users//moi/./AppData/").display_string(), "C:\\Users\\moi\\AppData");
        assert_eq!(p("c:\\a\\b\\..\\c").display_string(), "C:\\a\\c");
        assert_eq!(p("C:\\").display_string(), "C:\\");
        assert!(p("C:\\").is_volume_root());
    }

    #[test]
    fn prefixe_verbatim_et_unc() {
        assert_eq!(p("\\\\?\\C:\\Windows\\System32"), p("C:\\WINDOWS\\system32"));
        assert_eq!(p("\\\\?\\UNC\\nas\\partage\\x"), p("\\\\NAS\\Partage\\X"));
        assert_eq!(p("\\\\nas\\partage\\x").verbatim(), "\\\\?\\UNC\\nas\\partage\\x");
        assert_eq!(p("C:\\a b\\é").verbatim(), "\\\\?\\C:\\a b\\é");
    }

    #[test]
    fn point_point_resolu_meme_apres_verbatim() {
        assert_eq!(p("\\\\?\\C:\\Users\\moi\\..\\..\\Windows"), p("C:\\Windows"));
        assert_eq!(WinPath::parse("C:\\..\\Windows"), Err(PathError::EscapesRoot));
        assert_eq!(WinPath::parse("C:\\a\\..\\..\\b"), Err(PathError::EscapesRoot));
    }

    #[test]
    fn chemins_relatifs_refuses() {
        for raw in ["Windows", ".\\Windows", "..\\Windows", "\\Windows", "C:Windows", "C:", "a/b"] {
            assert_eq!(WinPath::parse(raw), Err(PathError::Relative), "{raw}");
        }
        assert_eq!(WinPath::parse(""), Err(PathError::Empty));
    }

    #[test]
    fn chemins_de_peripherique_refuses() {
        for raw in [
            "\\\\.\\C:",
            "\\\\.\\PhysicalDrive0",
            "\\??\\C:\\Windows",
            "\\\\?\\Volume{0f1e2d3c-0000-0000-0000-000000000000}\\EFI",
            "\\\\?\\GLOBALROOT\\Device\\HarddiskVolume1",
            "\\\\?\\pipe\\x",
        ] {
            assert_eq!(WinPath::parse(raw), Err(PathError::Device), "{raw}");
        }
    }

    #[test]
    fn noms_suspects_refuses() {
        assert!(matches!(WinPath::parse("C:\\Windows."), Err(PathError::TrailingDotOrSpace(_))));
        assert!(matches!(WinPath::parse("C:\\Windows \\x"), Err(PathError::TrailingDotOrSpace(_))));
        assert!(matches!(WinPath::parse("C:\\x\\NUL"), Err(PathError::ReservedName(_))));
        assert!(matches!(WinPath::parse("C:\\x\\con.txt"), Err(PathError::ReservedName(_))));
        assert!(matches!(WinPath::parse("C:\\x\\COM1 .log"), Err(PathError::ReservedName(_))));
        assert!(matches!(WinPath::parse("C:\\x\\f.txt:secret"), Err(PathError::AlternateStream(_))));
        assert!(matches!(WinPath::parse("C:\\x\\f*"), Err(PathError::InvalidChar(_))));
        assert!(matches!(WinPath::parse("C:\\x\\a\nb"), Err(PathError::InvalidChar(_))));
        assert_eq!(WinPath::parse(&format!("C:\\{}", "a".repeat(256))), Err(PathError::ComponentTooLong));
    }

    #[test]
    fn comparaison_insensible_a_la_casse_et_unicode() {
        assert!(p("C:\\USERS\\ÉLODIE\\x").starts_with(&p("c:\\users\\élodie")));
        assert!(!p("C:\\Users\\moi2").starts_with(&p("C:\\Users\\moi")));
        assert!(p("C:\\a\\b").is_strictly_under(&p("C:\\a")));
        assert!(!p("C:\\a").is_strictly_under(&p("C:\\a")));
        assert!(!p("D:\\a").starts_with(&p("C:\\a")));
        assert_eq!(fold("straße"), "STRAßE");
    }

    #[test]
    fn chemins_longs_espaces_unicode() {
        let long = format!("C:\\{}", ["dossier avec espaces é漢字🙂"; 12].join("\\"));
        let parsed = p(&long);
        assert!(parsed.display_string().chars().count() > 260);
        assert_eq!(parsed.depth(), 12);
        assert_eq!(p(&parsed.verbatim()), parsed);
    }

    #[test]
    fn chaine_des_parents() {
        let chain: Vec<String> = p("C:\\a\\b\\c").chain().iter().map(WinPath::display_string).collect();
        assert_eq!(chain, ["C:\\a", "C:\\a\\b", "C:\\a\\b\\c"]);
    }
}
