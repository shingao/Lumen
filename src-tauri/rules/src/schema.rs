//! Schéma des règles (`rules.toml`) et validation stricte au chargement.
//!
//! Tout champ inconnu, tout identifiant en double, tout texte manquant (le français est la
//! référence, l'anglais reprend les mêmes champs), toute commande hors de la liste fermée et
//! **tout chemin absolu écrit en dur** font échouer le chargement : une cible commence toujours
//! par une variable connue (`%LOCALAPPDATA%`, `%USERPROFILE%`, `%TEMP%`…).

use std::collections::HashSet;

use serde::Deserialize;

/// Statut d'une règle de nettoyage. Un « À vérifier » n'est jamais coché par défaut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Safe,
    Review,
}

/// Catégories de la légende (mêmes valeurs que le front).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    Media,
    Code,
    Packages,
    Tools,
    Android,
    Ai,
    Games,
    System,
}

impl Category {
    pub const ALL: [Category; 8] =
        [Category::Media, Category::Code, Category::Packages, Category::Tools, Category::Android, Category::Ai, Category::Games, Category::System];

    pub fn as_str(self) -> &'static str {
        match self {
            Category::Media => "media",
            Category::Code => "code",
            Category::Packages => "packages",
            Category::Tools => "tools",
            Category::Android => "android",
            Category::Ai => "ai",
            Category::Games => "games",
            Category::System => "system",
        }
    }
}

/// Texte affiché, en français (référence) et en anglais.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Text {
    pub fr: String,
    pub en: String,
}

/// Variables autorisées en tête d'une cible : les seuls points d'ancrage d'un chemin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Var {
    UserProfile,
    LocalAppData,
    AppData,
    Temp,
    SystemDrive,
    Documents,
    Pictures,
    Videos,
    Music,
    Desktop,
    Downloads,
    WinDir,
    ProgramFiles,
    ProgramFilesX86,
    ProgramData,
}

impl Var {
    pub fn parse(name: &str) -> Option<Var> {
        Some(match name {
            "USERPROFILE" => Var::UserProfile,
            "LOCALAPPDATA" => Var::LocalAppData,
            "APPDATA" => Var::AppData,
            "TEMP" => Var::Temp,
            "SYSTEMDRIVE" => Var::SystemDrive,
            "DOCUMENTS" => Var::Documents,
            "PICTURES" => Var::Pictures,
            "VIDEOS" => Var::Videos,
            "MUSIC" => Var::Music,
            "DESKTOP" => Var::Desktop,
            "DOWNLOADS" => Var::Downloads,
            "WINDIR" => Var::WinDir,
            "PROGRAMFILES" => Var::ProgramFiles,
            "PROGRAMFILESX86" => Var::ProgramFilesX86,
            "PROGRAMDATA" => Var::ProgramData,
            _ => return None,
        })
    }

    /// Variables utilisables par une règle de nettoyage : uniquement des dossiers de
    /// l'utilisateur. `%SYSTEMDRIVE%` n'est permis qu'aux règles « commande » (Windows.old par le
    /// Nettoyage de disque, par exemple), jamais à une suppression.
    pub fn allowed_for_rules(self) -> bool {
        matches!(self, Var::UserProfile | Var::LocalAppData | Var::AppData | Var::Temp | Var::SystemDrive)
    }
}

/// Cible : une variable puis des composants (littéraux, motifs `*`/`?`, ou `**` pour toute
/// profondeur).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub raw: String,
    pub var: Var,
    pub rest: Vec<String>,
}

/// Ce que la règle couvre sous chaque cible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// La cible elle-même.
    Whole,
    /// Ce qu'elle contient (le dossier reste).
    Contents,
    /// Ce qu'elle contient et qui n'a pas été modifié depuis `hours` heures (sous-arbre entier).
    OlderThan { hours: u32 },
}

/// Gain annoncé : exact, ou maximum (la commande officielle ne retire qu'une partie).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Gain {
    #[default]
    Exact,
    UpTo,
}

/// Suppression (par `safety`, jalon 5) ou commande officielle seule (les cibles sont alors
/// seulement mesurées : Windows.old, par exemple, n'est jamais supprimé directement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    #[default]
    Delete,
    Command,
}

/// Commandes officielles autorisées dans les règles (liste fermée, voir `safety::commands`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Command {
    NpmCacheClean,
    NpmCacheVerify,
    PnpmStorePrune,
    YarnCacheClean,
    PipCachePurge,
    UvCacheClean,
    GradleStop,
    NugetLocalsClear,
    WslShutdown,
    DiskCleanupSystemDrive,
}

impl Command {
    /// Commande `safety` correspondante (le lecteur système vient de `%SYSTEMDRIVE%`).
    pub fn official(self, system_drive: Option<char>) -> Option<safety::commands::OfficialCommand> {
        use safety::commands::OfficialCommand as O;
        Some(match self {
            Command::NpmCacheClean => O::NpmCacheClean,
            Command::NpmCacheVerify => O::NpmCacheVerify,
            Command::PnpmStorePrune => O::PnpmStorePrune,
            Command::YarnCacheClean => O::YarnCacheClean,
            Command::PipCachePurge => O::PipCachePurge,
            Command::UvCacheClean => O::UvCacheClean,
            Command::GradleStop => O::GradleStop,
            Command::NugetLocalsClear => O::NugetLocalsClear,
            Command::WslShutdown => O::WslShutdown,
            Command::DiskCleanupSystemDrive => O::DiskCleanup(safety::commands::Drive::parse(&system_drive?.to_string()).ok()?),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Activity {
    /// Dernière activité depuis moins de `inactive_days` jours.
    Active,
    /// Aucune activité depuis `inactive_days` jours ou plus.
    Inactive,
}

/// Artefact de build : un dossier qui n'est compté que si un fichier « marqueur » du projet est
/// à côté de lui (le projet sait le régénérer).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub dir: String,
    pub markers: Vec<String>,
    /// Au moins l'un de ces fichiers doit aussi être là (fichier de verrouillage : le projet
    /// sait régénérer exactement ce dossier). Vide : aucune exigence.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Seulement si le `.gitignore` du projet l'exclut (dossier ambigu comme `dist`).
    #[serde(default)]
    pub gitignored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectSpec {
    pub roots: Vec<Target>,
    /// Jamais de projet ici (applis installées dans AppData, extensions d'éditeur…).
    pub exclude: Vec<Target>,
    pub artifacts: Vec<Artifact>,
    pub activity: Activity,
    pub inactive_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub id: String,
    pub status: Status,
    pub category: Category,
    pub name: Text,
    pub explanation: Text,
    /// Règle « Sans risque » : qui régénère ces données (relu dans `sans-risque.txt`).
    pub regenerated_by: Option<Text>,
    pub targets: Vec<Target>,
    pub scope: Scope,
    pub gain: Gain,
    pub action: Action,
    /// Action « command » : les commandes ; action « delete » : préalables (`gradle --stop`).
    pub commands: Vec<Command>,
    pub project: Option<ProjectSpec>,
}

/// Classement (couleur de la légende) sans aucune action : dossiers connus, projets…
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Area {
    pub id: String,
    pub category: Category,
    pub targets: Vec<Target>,
    /// Un dossier qui contient l'un de ces fichiers est un projet de code.
    pub project_markers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSet {
    pub version: u32,
    pub rules: Vec<Rule>,
    pub areas: Vec<Area>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("règles invalides :\n- {}", .0.join("\n- "))]
pub struct LoadError(pub Vec<String>);

// --- Forme brute du fichier -------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRaw {
    version: u32,
    #[serde(default)]
    rule: Vec<RuleRaw>,
    #[serde(default)]
    area: Vec<AreaRaw>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
enum ScopeRaw {
    Whole,
    Contents,
    OlderThan,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct ProjectRaw {
    roots: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    artifacts: Vec<Artifact>,
    activity: Activity,
    inactive_days: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct RuleRaw {
    id: String,
    status: Status,
    category: Category,
    name: Text,
    explanation: Text,
    regenerated_by: Option<Text>,
    #[serde(default)]
    targets: Vec<String>,
    scope: Option<ScopeRaw>,
    min_age_hours: Option<u32>,
    #[serde(default)]
    gain: Gain,
    #[serde(default)]
    action: Action,
    #[serde(default)]
    commands: Vec<Command>,
    project: Option<ProjectRaw>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct AreaRaw {
    id: String,
    category: Category,
    #[serde(default)]
    targets: Vec<String>,
    #[serde(default)]
    project_markers: Vec<String>,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.split('-').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

/// Composant de chemin d'une règle : jamais `.`, `..`, séparateur, deux-points ni variable.
fn valid_component(c: &str, allow_wildcards: bool) -> bool {
    if c == "**" {
        return allow_wildcards;
    }
    !c.is_empty()
        && c.len() <= 255
        && c != "."
        && c != ".."
        && !c.ends_with(['.', ' '])
        && !c.contains("**")
        && (allow_wildcards || !c.contains(['*', '?']))
        && !c.chars().any(|ch| (ch as u32) < 0x20 || matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '%'))
}

/// Marqueur de projet : un nom de fichier, ou `*.ext`.
fn valid_marker(m: &str) -> bool {
    let body = m.strip_prefix("*.").unwrap_or(m);
    valid_component(body, false)
}

pub(crate) fn parse_target(raw: &str, allow_var: impl Fn(Var) -> bool) -> Result<Target, String> {
    let Some(after) = raw.strip_prefix('%') else {
        return Err(format!("« {raw} » : chemin absolu ou relatif interdit, une cible commence par une variable connue (%USERPROFILE%, %LOCALAPPDATA%, %TEMP%…)"));
    };
    let Some((name, rest)) = after.split_once('%') else {
        return Err(format!("« {raw} » : variable non fermée"));
    };
    let var = Var::parse(name).ok_or_else(|| format!("« {raw} » : variable inconnue %{name}%"))?;
    if !allow_var(var) {
        return Err(format!("« {raw} » : %{name}% n'est pas autorisée ici"));
    }
    let rest = match rest {
        "" => Vec::new(),
        r => {
            let Some(r) = r.strip_prefix('\\') else {
                return Err(format!("« {raw} » : « \\ » attendu après la variable"));
            };
            r.split('\\').map(str::to_owned).collect::<Vec<_>>()
        }
    };
    if let Some(bad) = rest.iter().find(|c| !valid_component(c, true)) {
        return Err(format!("« {raw} » : composant invalide « {bad} »"));
    }
    if rest.last().map(String::as_str) == Some("**") {
        return Err(format!("« {raw} » : « ** » ne peut pas terminer une cible"));
    }
    Ok(Target { raw: raw.to_owned(), var, rest })
}

fn check_text(errors: &mut Vec<String>, id: &str, field: &str, t: &Text, max: usize) {
    for (lang, s) in [("fr", &t.fr), ("en", &t.en)] {
        if s.trim().is_empty() {
            errors.push(format!("{id} : {field}.{lang} vide"));
        } else if s.chars().count() > max {
            errors.push(format!("{id} : {field}.{lang} trop long (> {max} caractères)"));
        } else if s != s.trim() {
            errors.push(format!("{id} : {field}.{lang} commence ou finit par une espace"));
        }
    }
}

/// Charge et valide un fichier de règles. Aucune règle n'est gardée si une seule est invalide.
pub fn load(source: &str) -> Result<RuleSet, LoadError> {
    let raw: FileRaw = toml::from_str(source).map_err(|e| LoadError(vec![format!("TOML : {e}")]))?;
    let mut errors = Vec::new();
    if raw.version != 1 {
        errors.push(format!("version {} inconnue (attendu : 1)", raw.version));
    }
    let mut ids = HashSet::new();
    let mut check_id = |errors: &mut Vec<String>, id: &str| {
        if !valid_id(id) {
            errors.push(format!("identifiant invalide « {id} » (minuscules, chiffres, tirets)"));
        }
        if !ids.insert(id.to_owned()) {
            errors.push(format!("identifiant en double « {id} »"));
        }
    };

    let mut rules = Vec::new();
    for r in raw.rule {
        let id = r.id.clone();
        check_id(&mut errors, &id);
        check_text(&mut errors, &id, "name", &r.name, 80);
        check_text(&mut errors, &id, "explanation", &r.explanation, 400);
        match (&r.regenerated_by, r.status) {
            (Some(t), _) => check_text(&mut errors, &id, "regenerated-by", t, 300),
            (None, Status::Safe) => errors.push(format!("{id} : une règle « safe » doit dire qui régénère ces données (regenerated-by)")),
            (None, Status::Review) => {}
        }
        let for_rule = |v: Var| v.allowed_for_rules() && (v != Var::SystemDrive || r.action == Action::Command);
        let mut targets = Vec::new();
        for t in &r.targets {
            match parse_target(t, for_rule) {
                Ok(t) => targets.push(t),
                Err(e) => errors.push(format!("{id} : {e}")),
            }
        }
        let scope = match (r.scope, r.min_age_hours) {
            (Some(ScopeRaw::OlderThan), Some(h)) if (1..=24 * 365).contains(&h) => Scope::OlderThan { hours: h },
            (Some(ScopeRaw::OlderThan), _) => {
                errors.push(format!("{id} : scope « older-than » exige min-age-hours entre 1 et 8760"));
                Scope::Whole
            }
            (_, Some(_)) => {
                errors.push(format!("{id} : min-age-hours n'a de sens qu'avec scope « older-than »"));
                Scope::Whole
            }
            (Some(ScopeRaw::Contents), None) => Scope::Contents,
            (Some(ScopeRaw::Whole) | None, None) => Scope::Whole,
        };
        if r.action == Action::Command && r.commands.is_empty() {
            errors.push(format!("{id} : action « command » sans commande"));
        }
        let mut seen = HashSet::new();
        if r.commands.iter().any(|c| !seen.insert(*c)) {
            errors.push(format!("{id} : commande en double"));
        }
        let project = match r.project {
            None => {
                if targets.is_empty() && r.targets.is_empty() {
                    errors.push(format!("{id} : ni cible ni projet"));
                }
                None
            }
            Some(p) => {
                if !r.targets.is_empty() {
                    errors.push(format!("{id} : une règle de projet n'a pas de cible directe"));
                }
                if r.action != Action::Delete || scope != Scope::Whole {
                    errors.push(format!("{id} : une règle de projet supprime des artefacts entiers (action delete, scope whole)"));
                }
                if !(1..=3650).contains(&p.inactive_days) {
                    errors.push(format!("{id} : inactive-days entre 1 et 3650"));
                }
                if p.artifacts.is_empty() {
                    errors.push(format!("{id} : aucun artefact"));
                }
                for a in &p.artifacts {
                    if !valid_component(&a.dir, false) {
                        errors.push(format!("{id} : nom d'artefact invalide « {} »", a.dir));
                    }
                    if a.markers.is_empty() || a.markers.iter().chain(&a.requires).any(|m| !valid_marker(m)) {
                        errors.push(format!("{id} : marqueurs invalides pour « {} »", a.dir));
                    }
                    if a.dir.eq_ignore_ascii_case(".git") {
                        errors.push(format!("{id} : .git n'est jamais un artefact"));
                    }
                }
                let mut roots = Vec::new();
                for t in &p.roots {
                    match parse_target(t, |v| v.allowed_for_rules() && v != Var::SystemDrive) {
                        Ok(t) => roots.push(t),
                        Err(e) => errors.push(format!("{id} : {e}")),
                    }
                }
                if roots.is_empty() && p.roots.is_empty() {
                    errors.push(format!("{id} : aucune racine de recherche de projets"));
                }
                let mut exclude = Vec::new();
                for t in &p.exclude {
                    // Une exclusion ne supprime rien : tous les dossiers connus sont permis.
                    match parse_target(t, |v| v != Var::SystemDrive) {
                        Ok(t) => exclude.push(t),
                        Err(e) => errors.push(format!("{id} : {e}")),
                    }
                }
                Some(ProjectSpec { roots, exclude, artifacts: p.artifacts, activity: p.activity, inactive_days: p.inactive_days })
            }
        };
        rules.push(Rule {
            id,
            status: r.status,
            category: r.category,
            name: r.name,
            explanation: r.explanation,
            regenerated_by: r.regenerated_by,
            targets,
            scope,
            gain: r.gain,
            action: r.action,
            commands: r.commands,
            project,
        });
    }

    let mut areas = Vec::new();
    for a in raw.area {
        check_id(&mut errors, &a.id);
        if a.targets.is_empty() && a.project_markers.is_empty() {
            errors.push(format!("{} : ni cible ni marqueur", a.id));
        }
        if a.project_markers.iter().any(|m| !valid_marker(m)) {
            errors.push(format!("{} : marqueur de projet invalide", a.id));
        }
        let mut targets = Vec::new();
        for t in &a.targets {
            match parse_target(t, |_| true) {
                Ok(t) => targets.push(t),
                Err(e) => errors.push(format!("{} : {e}", a.id)),
            }
        }
        areas.push(Area { id: a.id, category: a.category, targets, project_markers: a.project_markers });
    }

    if errors.is_empty() {
        Ok(RuleSet { version: raw.version, rules, areas })
    } else {
        Err(LoadError(errors))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
version = 1
[[rule]]
id = "cache-npm"
status = "safe"
category = "packages"
name = { fr = "Cache npm", en = "npm cache" }
explanation = { fr = "Copies de paquets.", en = "Package copies." }
regenerated-by = { fr = "npm", en = "npm" }
targets = ['%LOCALAPPDATA%\npm-cache']
scope = "contents"
"#;

    fn with(extra: &str) -> String {
        MINIMAL.replace("scope = \"contents\"", extra)
    }

    fn errors(src: &str) -> Vec<String> {
        load(src).err().map(|e| e.0).unwrap_or_default()
    }

    #[test]
    fn regle_valide() {
        let set = load(MINIMAL).unwrap();
        assert_eq!(set.rules.len(), 1);
        assert_eq!(set.rules[0].scope, Scope::Contents);
        assert_eq!(set.rules[0].targets[0].var, Var::LocalAppData);
        assert_eq!(set.rules[0].targets[0].rest, ["npm-cache"]);
    }

    #[test]
    fn schema_strict() {
        // Champ inconnu, statut inconnu, version inconnue.
        assert!(!errors(&with("scope = \"contents\"\nsuppression = \"tout\"")).is_empty());
        assert!(!errors(&MINIMAL.replace("\"safe\"", "\"garanti\"")).is_empty());
        assert!(!errors(&MINIMAL.replace("version = 1", "version = 2")).is_empty());
        // Texte anglais manquant, identifiant en double ou mal formé.
        assert!(!errors(&MINIMAL.replace(", en = \"npm cache\"", "")).is_empty());
        let twice = format!("{MINIMAL}\n{}", &MINIMAL[MINIMAL.find("[[rule]]").unwrap()..]);
        assert!(errors(&twice).iter().any(|e| e.contains("en double")));
        assert!(!errors(&MINIMAL.replace("cache-npm", "Cache_NPM")).is_empty());
        // « safe » sans dire qui régénère.
        assert!(errors(&MINIMAL.replace("regenerated-by = { fr = \"npm\", en = \"npm\" }\n", "")).iter().any(|e| e.contains("regenerated-by")));
        // Commande hors liste fermée.
        assert!(!errors(&with("action = \"command\"\ncommands = [\"format-c\"]")).is_empty());
        assert!(errors(&with("action = \"command\"")).iter().any(|e| e.contains("sans commande")));
    }

    #[test]
    fn aucun_chemin_absolu_en_dur() {
        for bad in [
            "'C:\\Users\\Arthur\\AppData\\Local\\npm-cache'",
            "'\\\\serveur\\partage'",
            "'npm-cache'",
            "'%LOCALAPPDATA%\\..\\..\\Windows'",
            "'%LOCALAPPDATA%\\.'",
            "'%INCONNUE%\\x'",
            "'%WINDIR%\\Temp'",
            "'%PROGRAMDATA%\\cache'",
            "'%SYSTEMDRIVE%\\Windows.old'",
            "'%LOCALAPPDATA%\\C:\\x'",
            "'%LOCALAPPDATA%\\a\\**'",
            "'%LOCALAPPDATA%\\a**b'",
            "'%LOCALAPPDATA%npm-cache'",
            "'%LOCALAPPDATA%\\a\\%TEMP%'",
            "'%LOCALAPPDATA%\\a\\\\b'",
        ] {
            let src = MINIMAL.replace("'%LOCALAPPDATA%\\npm-cache'", bad);
            assert!(!errors(&src).is_empty(), "{bad} accepté");
        }
        // %SYSTEMDRIVE% seulement pour une commande officielle.
        let cmd = MINIMAL
            .replace("'%LOCALAPPDATA%\\npm-cache'", "'%SYSTEMDRIVE%\\Windows.old'")
            .replace("scope = \"contents\"", "action = \"command\"\ncommands = [\"disk-cleanup-system-drive\"]");
        assert_eq!(errors(&cmd), Vec::<String>::new());
    }

    #[test]
    fn regles_de_projet() {
        let src = r#"
version = 1
[[rule]]
id = "artefacts"
status = "review"
category = "code"
name = { fr = "Artefacts", en = "Artifacts" }
explanation = { fr = "Régénérés.", en = "Rebuilt." }
project = { roots = ['%USERPROFILE%'], activity = "inactive", inactive-days = 180, artifacts = [
  { dir = "node_modules", markers = ["package.json"] },
  { dir = "bin", markers = ["*.csproj"], gitignored = true },
] }
"#;
        let set = load(src).unwrap();
        let p = set.rules[0].project.as_ref().unwrap();
        assert_eq!(p.inactive_days, 180);
        assert!(p.artifacts[1].gitignored);
        assert!(!errors(&src.replace("node_modules", ".git")).is_empty());
        assert!(!errors(&src.replace("\"package.json\"", "\"..\\\\x\"")).is_empty());
        assert!(!errors(&src.replace("180", "0")).is_empty());
    }
}
