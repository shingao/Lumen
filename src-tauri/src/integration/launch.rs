//! Arguments de lancement (logique pure) : « Analyser avec Lumen » de l'Explorateur et démarrage
//! avec Windows. Le chemin reçu est une entrée comme une autre : canonisé lexicalement par
//! `safety::path`, vérifié contre la liste des volumes, jamais exécuté ni passé à un shell. Il
//! ne sert qu'à ouvrir l'Explorer de Lumen sur ce dossier.

use safety::path::{PathError, Prefix, WinPath};

/// `lumen.exe --lumen-open "<dossier>"` : entrée du menu contextuel.
pub const OPEN_ARG: &str = "--lumen-open";
/// `lumen.exe --lumen-background` : entrée Run (démarrage de Windows).
pub const BACKGROUND_ARG: &str = "--lumen-background";
/// Au-delà, le chemin est refusé sans être analysé (limite des chemins `\\?\` de Windows).
const MAX_PATH_CHARS: usize = 32_767;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Launch {
    /// Démarrage avec Windows : zone de notification si l'icône est active, sinon fenêtre réduite.
    pub background: bool,
    /// Dossier demandé par le menu contextuel, tel que reçu (validé ensuite par `validate`).
    pub open: Option<String>,
}

/// Lit les arguments (sans le nom de l'exécutable). Seules deux formes sont comprises,
/// exactement : `--lumen-background`, ou `--lumen-open <dossier>`. Tout le reste est ignoré
/// (lancement ordinaire).
pub fn parse(args: &[String]) -> Launch {
    match args {
        [flag] if flag == BACKGROUND_ARG => Launch { background: true, open: None },
        [flag, path] if flag == OPEN_ARG => Launch { background: false, open: Some(path.clone()) },
        _ => Launch::default(),
    }
}

/// Dossier à ouvrir : lecteur de la liste des volumes, chemin complet canonisé (`C:\Users\…`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRequest {
    /// « C: »
    pub letter: String,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpenRefusal {
    #[error("chemin refusé : {0}")]
    Invalid(PathError),
    #[error("chemin trop long")]
    TooLong,
    #[error("partage réseau : seuls les lecteurs locaux sont analysés")]
    Network,
    #[error("lecteur absent ou non analysable")]
    UnknownVolume,
    #[error("ce n'est pas un dossier")]
    NotDirectory,
}

impl OpenRefusal {
    /// Code pour l'interface (texte traduit côté front).
    pub fn code(&self) -> &'static str {
        match self {
            OpenRefusal::Invalid(_) | OpenRefusal::TooLong => "invalid",
            OpenRefusal::Network => "network",
            OpenRefusal::UnknownVolume => "volume",
            OpenRefusal::NotDirectory => "notDirectory",
        }
    }
}

/// Volume présent : lettre (« C: ») et sorte (`fixed`, `removable`, `network`, `other`).
pub struct KnownVolume<'a> {
    pub letter: &'a str,
    pub kind: &'a str,
}

/// Valide le dossier reçu. `is_dir` ne consulte que les métadonnées du chemin canonisé.
pub fn validate(raw: &str, volumes: &[KnownVolume<'_>], is_dir: impl Fn(&WinPath) -> bool) -> Result<OpenRequest, OpenRefusal> {
    if raw.chars().count() > MAX_PATH_CHARS {
        return Err(OpenRefusal::TooLong);
    }
    let raw = raw.trim();
    // `"C:\"` dans une ligne de commande : la barre oblique inverse échappe le guillemet final,
    // le programme reçoit `C:"`. On rend la racine du lecteur, rien d'autre.
    let fixed;
    let raw = match raw.strip_suffix('"') {
        Some(head) if head.len() == 2 && head.ends_with(':') => {
            fixed = format!("{head}\\");
            fixed.as_str()
        }
        _ => raw,
    };
    let path = WinPath::parse(raw).map_err(OpenRefusal::Invalid)?;
    let letter = match path.prefix() {
        Prefix::Drive(d) => format!("{d}:"),
        Prefix::Unc { .. } => return Err(OpenRefusal::Network),
    };
    let volume = volumes.iter().find(|v| v.letter.eq_ignore_ascii_case(&letter)).ok_or(OpenRefusal::UnknownVolume)?;
    if !matches!(volume.kind, "fixed" | "removable") {
        return Err(if volume.kind == "network" { OpenRefusal::Network } else { OpenRefusal::UnknownVolume });
    }
    if !is_dir(&path) {
        return Err(OpenRefusal::NotDirectory);
    }
    Ok(OpenRequest { letter: volume.letter.to_owned(), path: path.display_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    const VOLUMES: &[KnownVolume<'static>] = &[
        KnownVolume { letter: "C:", kind: "fixed" },
        KnownVolume { letter: "E:", kind: "removable" },
        KnownVolume { letter: "Z:", kind: "network" },
        KnownVolume { letter: "R:", kind: "other" },
    ];

    fn ok(raw: &str) -> Result<OpenRequest, OpenRefusal> {
        validate(raw, VOLUMES, |_| true)
    }

    #[test]
    fn seules_deux_formes_d_arguments_sont_comprises() {
        assert_eq!(parse(&[]), Launch::default());
        assert_eq!(parse(&args(&["--lumen-background"])), Launch { background: true, open: None });
        assert_eq!(parse(&args(&["--lumen-open", "C:\\Jeux"])), Launch { background: false, open: Some("C:\\Jeux".into()) });
        // Arguments en trop, inconnus ou incomplets : lancement ordinaire.
        assert_eq!(parse(&args(&["--lumen-open"])), Launch::default());
        assert_eq!(parse(&args(&["--lumen-open", "C:\\a", "C:\\b"])), Launch::default());
        assert_eq!(parse(&args(&["--lumen-background", "--lumen-open", "C:\\a"])), Launch::default());
        assert_eq!(parse(&args(&["C:\\Jeux"])), Launch::default());
        assert_eq!(parse(&args(&["--LUMEN-OPEN", "C:\\Jeux"])), Launch::default());
    }

    #[test]
    fn dossier_canonise_sur_un_lecteur_connu() {
        assert_eq!(ok("C:\\Users\\Arthur\\..\\Arthur\\.\\Videos\\"), Ok(OpenRequest { letter: "C:".into(), path: "C:\\Users\\Arthur\\Videos".into() }));
        assert_eq!(ok("c:/Users"), Ok(OpenRequest { letter: "C:".into(), path: "C:\\Users".into() }));
        assert_eq!(ok("\\\\?\\C:\\Users"), Ok(OpenRequest { letter: "C:".into(), path: "C:\\Users".into() }));
        assert_eq!(ok("E:\\"), Ok(OpenRequest { letter: "E:".into(), path: "E:\\".into() }));
        // Racine reçue avec le guillemet échappé par la ligne de commande.
        assert_eq!(ok("C:\""), Ok(OpenRequest { letter: "C:".into(), path: "C:\\".into() }));
        assert_eq!(ok("  C:\\Users  "), Ok(OpenRequest { letter: "C:".into(), path: "C:\\Users".into() }));
    }

    #[test]
    fn tout_le_reste_est_refuse() {
        assert!(matches!(ok(""), Err(OpenRefusal::Invalid(_))));
        assert!(matches!(ok("Users\\Arthur"), Err(OpenRefusal::Invalid(_))));
        assert!(matches!(ok("C:"), Err(OpenRefusal::Invalid(_))));
        assert!(matches!(ok("C:\\..\\.."), Err(OpenRefusal::Invalid(_))));
        assert!(matches!(ok("\\\\.\\PhysicalDrive0"), Err(OpenRefusal::Invalid(_))));
        assert!(matches!(ok("\\\\?\\Volume{0000}\\"), Err(OpenRefusal::Invalid(_))));
        assert!(matches!(ok("C:\\a\" & calc.exe & \""), Err(OpenRefusal::Invalid(_))));
        assert!(matches!(ok("C:\\fichier.txt:flux"), Err(OpenRefusal::Invalid(_))));
        assert!(matches!(ok("C:\\CON"), Err(OpenRefusal::Invalid(_))));
        assert_eq!(ok(&format!("C:\\{}", "a\\".repeat(20_000))), Err(OpenRefusal::TooLong));
        assert_eq!(ok("\\\\serveur\\partage\\dossier"), Err(OpenRefusal::Network));
        assert_eq!(ok("Z:\\Partage"), Err(OpenRefusal::Network));
        assert_eq!(ok("R:\\"), Err(OpenRefusal::UnknownVolume));
        assert_eq!(ok("Q:\\Absent"), Err(OpenRefusal::UnknownVolume));
        assert_eq!(validate("C:\\pagefile.sys", VOLUMES, |_| false), Err(OpenRefusal::NotDirectory));
        assert_eq!(OpenRefusal::Network.code(), "network");
    }
}
