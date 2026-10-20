//! Commandes officielles des outils : liste fermée de programmes, arguments construits ici en
//! tableau (jamais par concaténation), arguments variables validés. Aucune n'est lancée avant le
//! jalon 5 ; ce module ne fait que décrire ce qui serait exécuté.

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

    #[test]
    fn outils_systeme_et_elevation() {
        assert!(Program::Dism.is_system_tool());
        assert!(!Program::Npm.is_system_tool());
        assert!(OfficialCommand::DismComponentCleanup.needs_elevation());
        assert!(!OfficialCommand::PipCachePurge.needs_elevation());
    }
}
