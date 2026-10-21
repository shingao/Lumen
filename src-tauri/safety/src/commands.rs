//! Commandes officielles des outils : liste fermée de programmes, arguments construits ici en
//! tableau (jamais par concaténation), arguments variables validés.
//!
//! Un binaire n'est jamais lancé par son seul nom : [`Program::candidates`] liste ses chemins
//! complets possibles (emplacements d'installation connus, puis entrées absolues du PATH), et
//! `safety::win` lance le premier qui existe vraiment (fichier ordinaire, pas un lien), par son
//! chemin réel. Les outils système ne viennent que de `%SystemRoot%\System32`.

use crate::path::WinPath;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Program {
    Npm,
    Pnpm,
    Yarn,
    Pip,
    Uv,
    Gradle,
    Dotnet,
    Docker,
    Ollama,
    Wsl,
    Dism,
    Powercfg,
    Vssadmin,
    Cleanmgr,
}

impl Program {
    /// Nom du binaire. Les outils système sont résolus dans `%SystemRoot%\System32`, jamais via PATH.
    pub fn binary(self) -> &'static str {
        match self {
            Program::Npm => "npm",
            Program::Pnpm => "pnpm",
            Program::Yarn => "yarn",
            Program::Pip => "pip",
            Program::Uv => "uv",
            Program::Gradle => "gradle",
            Program::Dotnet => "dotnet",
            Program::Docker => "docker",
            Program::Ollama => "ollama",
            Program::Wsl => "wsl.exe",
            Program::Dism => "dism.exe",
            Program::Powercfg => "powercfg.exe",
            Program::Vssadmin => "vssadmin.exe",
            Program::Cleanmgr => "cleanmgr.exe",
        }
    }

    pub fn is_system_tool(self) -> bool {
        matches!(self, Program::Wsl | Program::Dism | Program::Powercfg | Program::Vssadmin | Program::Cleanmgr)
    }

    /// Dossiers d'installation connus, avant le PATH.
    fn known_dirs(self, d: &ToolDirs) -> Vec<WinPath> {
        let under = |base: &Option<WinPath>, rel: &[&str]| base.as_ref().and_then(|b| join_all(b, rel));
        let mut out: Vec<Option<WinPath>> = Vec::new();
        match self {
            Program::Npm => {
                out.extend(d.program_files.iter().map(|pf| join_all(pf, &["nodejs"])));
                out.push(under(&d.local_app_data, &["Programs", "nodejs"]));
            }
            Program::Pnpm => {
                out.push(under(&d.local_app_data, &["pnpm"]));
                out.push(under(&d.app_data, &["npm"]));
            }
            Program::Yarn => {
                out.extend(d.program_files.iter().map(|pf| join_all(pf, &["Yarn", "bin"])));
                out.push(under(&d.app_data, &["npm"]));
            }
            Program::Uv => {
                out.push(under(&d.user_profile, &[".local", "bin"]));
                out.push(under(&d.user_profile, &[".cargo", "bin"]));
            }
            Program::Dotnet => out.extend(d.program_files.iter().map(|pf| join_all(pf, &["dotnet"]))),
            Program::Docker => out.extend(d.program_files.iter().map(|pf| join_all(pf, &["Docker", "Docker", "resources", "bin"]))),
            Program::Ollama => out.push(under(&d.local_app_data, &["Programs", "Ollama"])),
            Program::Pip | Program::Gradle => {}
            Program::Wsl | Program::Dism | Program::Powercfg | Program::Vssadmin | Program::Cleanmgr => {}
        }
        out.into_iter().flatten().collect()
    }

    /// Chemins complets possibles, dans l'ordre de préférence. Outils système : `System32` seul.
    pub fn candidates(self, d: &ToolDirs) -> Vec<Candidate> {
        if self.is_system_tool() {
            return d
                .system_root
                .as_ref()
                .and_then(|r| join_all(r, &["System32", self.binary()]))
                .map(|exe| Candidate { exe, script: None })
                .into_iter()
                .collect();
        }
        let mut dirs = self.known_dirs(d);
        for p in &d.path {
            if !dirs.contains(p) {
                dirs.push(p.clone());
            }
        }
        let mut out = Vec::new();
        for dir in dirs {
            if self == Program::Npm {
                // npm.cmd ne fait que lancer node sur npm-cli.js : on lance node directement.
                if let (Some(exe), Some(script)) =
                    (join_all(&dir, &["node.exe"]), join_all(&dir, &["node_modules", "npm", "bin", "npm-cli.js"]))
                {
                    out.push(Candidate { exe, script: Some(script) });
                }
                continue;
            }
            for ext in ["exe", "cmd", "bat"] {
                if let Some(exe) = join_all(&dir, &[&format!("{}.{ext}", self.binary())]) {
                    out.push(Candidate { exe, script: None });
                }
            }
        }
        out
    }
}

fn join_all(base: &WinPath, rel: &[&str]) -> Option<WinPath> {
    rel.iter().try_fold(base.clone(), |p, c| p.join(c).ok())
}

/// Où chercher les binaires : dossiers connus de Windows et entrées absolues du PATH.
#[derive(Debug, Clone, Default)]
pub struct ToolDirs {
    pub system_root: Option<WinPath>,
    pub program_files: Vec<WinPath>,
    pub local_app_data: Option<WinPath>,
    pub app_data: Option<WinPath>,
    pub user_profile: Option<WinPath>,
    /// Entrées du PATH déjà filtrées par [`path_entries`].
    pub path: Vec<WinPath>,
}

/// Entrées du PATH utilisables : chemins absolus sur un lecteur local (ni relatifs, ni réseau,
/// ni `.`), hors des dossiers exclus (dossier temporaire), sans doublon.
pub fn path_entries(raw: &str, exclude: &[WinPath]) -> Vec<WinPath> {
    let mut out: Vec<WinPath> = Vec::new();
    for entry in raw.split(';') {
        let entry = entry.trim().trim_matches('"');
        let drive_absolute = entry.len() >= 3 && entry.as_bytes()[1] == b':' && matches!(entry.as_bytes()[2], b'\\' | b'/');
        if !drive_absolute || entry.contains('%') {
            continue;
        }
        let Ok(p) = WinPath::parse(entry) else { continue };
        if exclude.iter().any(|x| p.starts_with(x)) || out.contains(&p) {
            continue;
        }
        out.push(p);
    }
    out
}

/// Un emplacement possible du binaire : l'exécutable, et pour npm le script lancé par node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub exe: WinPath,
    pub script: Option<WinPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgError {
    #[error("nom de modèle Ollama invalide : « {0} »")]
    ModelName(String),
    #[error("lettre de lecteur invalide : « {0} »")]
    Drive(String),
}

/// Nom de modèle Ollama validé (`deepseek-coder`, `llama3:8b`, `library/mistral`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelName(String);

impl ModelName {
    pub fn parse(raw: &str) -> Result<ModelName, ArgError> {
        let ok = !raw.is_empty()
            && raw.len() <= 128
            && !raw.starts_with(['-', '.', '/', ':'])
            && !raw.contains("..")
            && raw.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.' | ':' | '/'));
        if ok {
            Ok(ModelName(raw.to_owned()))
        } else {
            Err(ArgError::ModelName(raw.to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Lettre de lecteur validée (`C`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drive(char);

impl Drive {
    pub fn parse(raw: &str) -> Result<Drive, ArgError> {
        let mut chars = raw.trim_end_matches(':').chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if c.is_ascii_alphabetic() => Ok(Drive(c.to_ascii_uppercase())),
            _ => Err(ArgError::Drive(raw.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OfficialCommand {
    NpmCacheClean,
    NpmCacheVerify,
    PnpmStorePrune,
    YarnCacheClean,
    PipCachePurge,
    UvCacheClean,
    GradleStop,
    NugetLocalsClear,
    /// Images Docker inutilisées (confirmation spécifique dans l'interface).
    DockerImagePrune,
    OllamaRm(ModelName),
    WslShutdown,
    DismComponentCleanup,
    /// Désactive l'hibernation : plus de démarrage rapide ni de mise en veille prolongée.
    HibernateOff,
    /// Hibernation réduite : garde le démarrage rapide, plus de mise en veille prolongée.
    HibernateReduced,
    /// Supprime uniquement le plus ancien point de restauration du lecteur, jamais tous.
    ShadowDeleteOldest(Drive),
    /// Nettoyage de disque de Windows (Windows.old, fichiers système).
    DiskCleanup(Drive),
}

impl OfficialCommand {
    pub fn program(&self) -> Program {
        use OfficialCommand::*;
        match self {
            NpmCacheClean | NpmCacheVerify => Program::Npm,
            PnpmStorePrune => Program::Pnpm,
            YarnCacheClean => Program::Yarn,
            PipCachePurge => Program::Pip,
            UvCacheClean => Program::Uv,
            GradleStop => Program::Gradle,
            NugetLocalsClear => Program::Dotnet,
            DockerImagePrune => Program::Docker,
            OllamaRm(_) => Program::Ollama,
            WslShutdown => Program::Wsl,
            DismComponentCleanup => Program::Dism,
            HibernateOff | HibernateReduced => Program::Powercfg,
            ShadowDeleteOldest(_) => Program::Vssadmin,
            DiskCleanup(_) => Program::Cleanmgr,
        }
    }

    /// Arguments, un par case : jamais interprétés par un shell.
    pub fn args(&self) -> Vec<String> {
        use OfficialCommand::*;
        let v = |a: &[&str]| a.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        match self {
            NpmCacheClean => v(&["cache", "clean", "--force"]),
            NpmCacheVerify => v(&["cache", "verify"]),
            PnpmStorePrune => v(&["store", "prune"]),
            YarnCacheClean => v(&["cache", "clean"]),
            PipCachePurge => v(&["cache", "purge"]),
            UvCacheClean => v(&["cache", "clean"]),
            GradleStop => v(&["--stop"]),
            NugetLocalsClear => v(&["nuget", "locals", "all", "--clear"]),
            DockerImagePrune => v(&["image", "prune", "--all", "--force"]),
            OllamaRm(model) => vec!["rm".into(), model.as_str().to_owned()],
            WslShutdown => v(&["--shutdown"]),
            DismComponentCleanup => v(&["/Online", "/Cleanup-Image", "/StartComponentCleanup"]),
            HibernateOff => v(&["/h", "off"]),
            HibernateReduced => v(&["/h", "/type", "reduced"]),
            ShadowDeleteOldest(Drive(d)) => vec!["delete".into(), "shadows".into(), format!("/for={d}:"), "/oldest".into(), "/quiet".into()],
            DiskCleanup(Drive(d)) => vec!["/d".into(), format!("{d}:")],
        }
    }

    /// Délai maximal : au-delà, la commande et ses processus enfants sont arrêtés.
    pub fn timeout_secs(&self) -> u64 {
        use OfficialCommand::*;
        match self {
            GradleStop | WslShutdown => 120,
            NpmCacheClean | PnpmStorePrune | YarnCacheClean | PipCachePurge | UvCacheClean | NugetLocalsClear => 1800,
            NpmCacheVerify | DockerImagePrune | OllamaRm(_) => 900,
            DismComponentCleanup | DiskCleanup(_) => 3600,
            HibernateOff | HibernateReduced | ShadowDeleteOldest(_) => 300,
        }
    }

    pub fn needs_elevation(&self) -> bool {
        use OfficialCommand::*;
        matches!(self, DismComponentCleanup | HibernateOff | HibernateReduced | ShadowDeleteOldest(_) | DiskCleanup(_))
    }

    /// Ce que fait la commande, en une phrase (affichée et commentée dans les scripts).
    pub fn explanation(&self) -> &'static str {
        use OfficialCommand::*;
        match self {
            NpmCacheClean => "Outil officiel : vide le cache npm",
            NpmCacheVerify => "Vérifie l'intégrité du cache npm",
            PnpmStorePrune => "Retire du store pnpm uniquement les paquets qu'aucun projet ne référence",
            YarnCacheClean => "Outil officiel : vide le cache Yarn",
            PipCachePurge => "Outil officiel : vide le cache pip",
            UvCacheClean => "Outil officiel : vide le cache uv",
            GradleStop => "Arrête les démons Gradle avant de toucher à leurs caches",
            NugetLocalsClear => "Outil officiel : vide les caches locaux NuGet",
            DockerImagePrune => "Supprime les images Docker qu'aucun conteneur n'utilise",
            OllamaRm(_) => "Supprime le modèle d'Ollama (retéléchargeable avec ollama pull)",
            WslShutdown => "Arrête WSL et Docker avant le compactage de leur disque virtuel",
            DismComponentCleanup => "Nettoie le magasin de composants de Windows (WinSxS) avec l'outil officiel",
            HibernateOff => "Désactive l'hibernation : plus de démarrage rapide ni de mise en veille prolongée",
            HibernateReduced => "Hibernation réduite : garde le démarrage rapide, plus de veille prolongée",
            ShadowDeleteOldest(_) => "Supprime uniquement le plus ancien point de restauration",
            DiskCleanup(_) => "Ouvre le Nettoyage de disque de Windows",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_en_tableau() {
        assert_eq!(OfficialCommand::NpmCacheClean.args(), ["cache", "clean", "--force"]);
        assert_eq!(
            OfficialCommand::ShadowDeleteOldest(Drive::parse("c:").unwrap()).args(),
            ["delete", "shadows", "/for=C:", "/oldest", "/quiet"]
        );
        let rm = OfficialCommand::OllamaRm(ModelName::parse("llama3:8b").unwrap());
        assert_eq!(rm.program(), Program::Ollama);
        assert_eq!(rm.args(), ["rm", "llama3:8b"]);
    }

    #[test]
    fn points_de_restauration_jamais_tous() {
        let args = OfficialCommand::ShadowDeleteOldest(Drive::parse("C").unwrap()).args();
        assert!(args.contains(&"/oldest".to_owned()));
        assert!(!args.iter().any(|a| a.eq_ignore_ascii_case("/all")));
    }

    #[test]
    fn arguments_variables_valides() {
        for bad in ["", "-rf", "--help", "../x", "a b", "a;b", "A", "x&y", "modèle", ".cache", "a..b"] {
            assert!(ModelName::parse(bad).is_err(), "{bad}");
        }
        for bad in ["", "CD", "1", "C:\\", "-"] {
            assert!(Drive::parse(bad).is_err(), "{bad}");
        }
        assert!(ModelName::parse("library/mistral").is_ok());
    }

    fn tool_dirs(path: &str) -> ToolDirs {
        let p = |s: &str| WinPath::parse(s).unwrap();
        ToolDirs {
            system_root: Some(p("C:\\Windows")),
            program_files: vec![p("C:\\Program Files"), p("C:\\Program Files (x86)")],
            local_app_data: Some(p("C:\\Users\\Arthur\\AppData\\Local")),
            app_data: Some(p("C:\\Users\\Arthur\\AppData\\Roaming")),
            user_profile: Some(p("C:\\Users\\Arthur")),
            path: path_entries(path, &[p("C:\\Users\\Arthur\\AppData\\Local\\Temp")]),
        }
    }

    #[test]
    fn entrees_du_path_absolues_seulement() {
        let raw = "C:\\Windows\\system32;;.;bin;\\\\serveur\\outils;\"C:\\Program Files\\nodejs\\\";%USERPROFILE%\\bin;C:\\Users\\Arthur\\AppData\\Local\\Temp\\x;c:/tools;C:\\WINDOWS\\System32";
        let entries: Vec<String> = tool_dirs(raw).path.iter().map(|p| p.display_string()).collect();
        assert_eq!(entries, ["C:\\Windows\\system32", "C:\\Program Files\\nodejs", "C:\\tools"]);
    }

    #[test]
    fn binaires_par_chemin_complet() {
        let d = tool_dirs("D:\\outils;C:\\Program Files\\nodejs");
        // Outils système : System32 uniquement, jamais le PATH.
        let dism = Program::Dism.candidates(&d);
        assert_eq!(dism, [Candidate { exe: WinPath::parse("C:\\Windows\\System32\\dism.exe").unwrap(), script: None }]);
        // npm : node.exe + npm-cli.js, emplacement connu d'abord, PATH ensuite, sans doublon.
        let npm = Program::Npm.candidates(&d);
        assert_eq!(npm[0].exe.display_string(), "C:\\Program Files\\nodejs\\node.exe");
        assert_eq!(npm[0].script.as_ref().unwrap().display_string(), "C:\\Program Files\\nodejs\\node_modules\\npm\\bin\\npm-cli.js");
        assert_eq!(npm.iter().filter(|c| c.exe.display_string() == "C:\\Program Files\\nodejs\\node.exe").count(), 1);
        assert!(npm.iter().any(|c| c.exe.display_string() == "D:\\outils\\node.exe"));
        // Autres : .exe avant .cmd / .bat, emplacements connus avant le PATH.
        let pnpm: Vec<String> = Program::Pnpm.candidates(&d).iter().map(|c| c.exe.display_string()).collect();
        assert_eq!(pnpm[0], "C:\\Users\\Arthur\\AppData\\Local\\pnpm\\pnpm.exe");
        assert_eq!(pnpm[1], "C:\\Users\\Arthur\\AppData\\Local\\pnpm\\pnpm.cmd");
        assert!(pnpm.contains(&"D:\\outils\\pnpm.exe".to_owned()));
        assert!(Program::Gradle.candidates(&d).iter().all(|c| c.exe.components().len() >= 2));
        assert!(Program::Gradle.candidates(&tool_dirs("")).is_empty());
    }

    #[test]
    fn delais_maximaux() {
        assert_eq!(OfficialCommand::GradleStop.timeout_secs(), 120);
        assert!(OfficialCommand::NpmCacheClean.timeout_secs() <= 1800);
    }

    #[test]
    fn outils_systeme_et_elevation() {
        assert!(Program::Dism.is_system_tool());
        assert!(!Program::Npm.is_system_tool());
        assert!(OfficialCommand::DismComponentCleanup.needs_elevation());
        assert!(!OfficialCommand::PipCachePurge.needs_elevation());
    }
}
