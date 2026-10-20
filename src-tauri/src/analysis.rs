//! Règles appliquées à une analyse terminée : statuts, catégories et gains des nœuds,
//! recommandations et KPI. Les règles livrées (`rules/rules.toml`) sont chargées et validées une
//! fois ; une règle invalide n'est jamais appliquée à moitié (aucune règle si le fichier échoue).

use std::sync::OnceLock;

use serde::Serialize;

use rules::engine::{ActivitySource, STATUS_REVIEW, STATUS_SAFE};
use rules::schema::{Action, Gain, Status, Text};
use rules::{Analysis, RuleSet};
use scan::{Kind, NodeId, Scan};

/// Éléments mis en avant au survol d'une recommandation, au plus.
const MAX_HIGHLIGHT: usize = 200;
/// Fichiers listés dans le détail, au plus.
const MAX_FILES: usize = 8;

fn builtin() -> &'static Result<RuleSet, String> {
    static RULES: OnceLock<Result<RuleSet, String>> = OnceLock::new();
    RULES.get_or_init(|| rules::builtin().map_err(|e| e.to_string()))
}

/// Profil de l'utilisateur, pour afficher `~\…`.
pub struct Applied {
    pub analysis: Analysis,
    profile: Option<String>,
}

#[cfg(windows)]
fn context() -> Option<(rules::Env, safety::denylist::DenyList, Option<String>)> {
    let layout = safety::win::system_layout().ok()?;
    let mut env = rules::Env::new();
    for (name, path) in safety::win::rule_variables() {
        if let Some(var) = rules::schema::Var::parse(name) {
            env.set(var, path);
        }
    }
    let profile = Some(layout.profile.display_string());
    Some((env, safety::denylist::DenyList::new(&layout), profile))
}

#[cfg(not(windows))]
fn context() -> Option<(rules::Env, safety::denylist::DenyList, Option<String>)> {
    None
}

/// FILETIME actuel.
fn now() -> i64 {
    let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    (unix + 11_644_473_600) * 10_000_000
}

/// Applique les règles livrées ; `None` hors de Windows ou si elles sont invalides (journalisé).
pub fn apply(scan: &Scan) -> Option<Applied> {
    let set = match builtin() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return None;
        }
    };
    let (env, deny, profile) = context()?;
    let ctx = rules::Context { env: &env, deny: &deny, now: now(), read_disk: true };
    let analysis = rules::analyze(set, scan, &ctx);
    for d in &analysis.disabled {
        eprintln!("règle {} : cible {} neutralisée ({})", d.rule, d.target, d.reason);
    }
    Some(Applied { analysis, profile })
}

#[derive(Debug, Clone, Serialize)]
pub struct TextDto {
    pub fr: String,
    pub en: String,
}

fn text(t: &Text) -> TextDto {
    TextDto { fr: t.fr.clone(), en: t.en.clone() }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoDto {
    /// Identifiant de la règle.
    pub id: String,
    pub status: &'static str,
    pub category: &'static str,
    pub name: TextDto,
    pub path: String,
    pub size: u64,
    pub logical: u64,
    pub files: u64,
    pub dirs: u64,
    /// Dernière utilisation la plus récente (ms depuis 1970), 0 si inconnue.
    pub last_used: i64,
    pub up_to: bool,
    pub action: &'static str,
    pub projects: usize,
    /// Dernière activité du projet le plus récemment actif (ms), 0 sans projet.
    pub last_activity: i64,
    /// Éléments couverts (les plus hauts).
    pub covered: usize,
    pub highlight: Vec<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDto {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandDto {
    pub line: String,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    pub path: String,
    pub last_activity: i64,
    pub source: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoDetailDto {
    #[serde(flatten)]
    pub reco: RecoDto,
    pub explanation: TextDto,
    pub regenerated_by: Option<TextDto>,
    pub file_list: Vec<FileDto>,
    pub commands: Vec<CommandDto>,
    pub project_list: Vec<ProjectDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleKpisDto {
    pub safe_bytes: u64,
    pub safe_count: usize,
    pub review_bytes: u64,
    pub review_count: usize,
}

/// Ce que l'interface affiche d'un nœud de la treemap.
#[derive(Debug, Clone, Copy)]
pub struct NodeView {
    pub status: &'static str,
    pub category: &'static str,
    pub reco: Option<usize>,
    pub gain: u64,
    pub gain_status: Option<&'static str>,
}

/// Tuiles à mettre en avant au survol : les éléments couverts et tous leurs dossiers parents, pour
/// que la treemap les retrouve quel que soit le niveau de zoom.
fn highlight(scan: &Scan, covered: &[NodeId]) -> Vec<i64> {
    let mut out = std::collections::BTreeSet::new();
    for &c in covered.iter().take(MAX_HIGHLIGHT) {
        let mut cur = c;
        while out.insert(i64::from(cur)) && cur != scan.root() {
            cur = scan.node(cur).map_or(scan.root(), |n| n.parent);
        }
    }
    out.into_iter().collect()
}

/// FILETIME → millisecondes depuis 1970.
pub fn epoch_ms(filetime: i64) -> i64 {
    if filetime <= 0 {
        return 0;
    }
    filetime / 10_000 - 11_644_473_600_000
}

fn status_name(s: Status) -> &'static str {
    match s {
        Status::Safe => "safe",
        Status::Review => "review",
    }
}

impl Applied {
    fn set(&self) -> Option<&'static RuleSet> {
        builtin().as_ref().ok()
    }

    /// `C:\Users\Arthur\AppData\…` → `~\AppData\…`.
    fn display(&self, path: &str) -> String {
        match &self.profile {
            Some(p) if path.len() >= p.len() && path[..p.len()].eq_ignore_ascii_case(p) && path[p.len()..].starts_with('\\') => {
                format!("~{}", &path[p.len()..])
            }
            _ => path.to_owned(),
        }
    }

    pub fn node(&self, id: NodeId) -> NodeView {
        let a = &self.analysis;
        let i = id as usize;
        let status = match a.status.get(i) {
            Some(&STATUS_SAFE) => "safe",
            Some(&STATUS_REVIEW) => "review",
            _ => "keep",
        };
        let category = a.category.get(i).and_then(|&c| rules::Category::ALL.get(c as usize)).map_or("tools", |c| c.as_str());
        let reco = a.reco_of.get(i).copied().filter(|&r| r != u16::MAX).map(usize::from);
        let (safe, review) = (a.gain_safe.get(i).copied().unwrap_or(0), a.gain_review.get(i).copied().unwrap_or(0));
        let gain_status = if safe > 0 {
            Some("safe")
        } else if review > 0 {
            Some("review")
        } else {
            None
        };
        NodeView { status, category, reco, gain: safe + review, gain_status }
    }

    /// Identifiant (règle) de la recommandation `idx`.
    pub fn reco_id(&self, idx: usize) -> Option<String> {
        let set = self.set()?;
        self.analysis.recos.get(idx).map(|r| set.rules[r.rule].id.clone())
    }

    pub fn reco_up_to(&self, idx: usize) -> bool {
        self.set().zip(self.analysis.recos.get(idx)).is_some_and(|(s, r)| s.rules[r.rule].gain == Gain::UpTo)
    }

    fn reco_dto(&self, scan: &Scan, idx: usize) -> Option<RecoDto> {
        let set = self.set()?;
        let r = self.analysis.recos.get(idx)?;
        let rule = &set.rules[r.rule];
        let path = if r.covered.len() == 1 {
            self.display(&scan.path(r.covered[0]))
        } else {
            // Dossier commun aux éléments couverts.
            let paths: Vec<String> = r.covered.iter().map(|&c| scan.path(c)).collect();
            let mut common: Vec<&str> = paths[0].split('\\').collect();
            for p in &paths[1..] {
                let parts: Vec<&str> = p.split('\\').collect();
                let k = common.iter().zip(&parts).take_while(|(a, b)| a.eq_ignore_ascii_case(b)).count();
                common.truncate(k);
            }
            format!("{}\\…", self.display(&common.join("\\")))
        };
        Some(RecoDto {
            id: rule.id.clone(),
            status: status_name(rule.status),
            category: rule.category.as_str(),
            name: text(&rule.name),
            path,
            size: r.allocated,
            logical: r.logical,
            files: r.files,
            dirs: r.dirs,
            last_used: epoch_ms(r.last_used),
            up_to: rule.gain == Gain::UpTo,
            action: match rule.action {
                Action::Delete => "delete",
                Action::Command => "command",
            },
            projects: r.projects.len(),
            last_activity: r.projects.iter().map(|p| epoch_ms(p.last_activity)).max().unwrap_or(0),
            covered: r.covered.len(),
            highlight: highlight(scan, &r.covered),
        })
    }

    /// Recommandations : sans risque d'abord, puis à vérifier, chacune de la plus grosse à la plus petite.
    pub fn recommendations(&self, scan: &Scan) -> Vec<RecoDto> {
        let mut out: Vec<RecoDto> = (0..self.analysis.recos.len()).filter_map(|i| self.reco_dto(scan, i)).collect();
        out.sort_by(|a, b| (a.status != "safe").cmp(&(b.status != "safe")).then(b.size.cmp(&a.size)));
        out
    }

    pub fn detail(&self, scan: &Scan, id: &str) -> Option<RecoDetailDto> {
        let set = self.set()?;
        let idx = self.analysis.recos.iter().position(|r| set.rules[r.rule].id == id)?;
        let r = &self.analysis.recos[idx];
        let rule = &set.rules[r.rule];
        let drive = scan.root_path.chars().next().filter(char::is_ascii_alphabetic);
        let file_list = r
            .covered
            .iter()
            .take(MAX_FILES)
            .map(|&c| {
                let mut path = self.display(&scan.path(c));
                if scan.node(c).is_some_and(|n| n.kind == Kind::Dir) {
                    path.push('\\');
                }
                FileDto { path, size: scan.node(c).map_or(0, |n| n.allocated) }
            })
            .collect();
        let commands = rule
            .commands
            .iter()
            .filter_map(|c| c.official(drive))
            .map(|c| CommandDto { line: format!("{} {}", c.program().binary(), c.args().join(" ")), explanation: c.explanation().to_owned() })
            .collect();
        let project_list = r
            .projects
            .iter()
            .map(|p| ProjectDto {
                path: self.display(&scan.path(p.root)),
                last_activity: epoch_ms(p.last_activity),
                source: match p.source {
                    ActivitySource::Git => "git",
                    ActivitySource::Files => "files",
                },
            })
            .collect();
        Some(RecoDetailDto {
            reco: self.reco_dto(scan, idx)?,
            explanation: text(&rule.explanation),
            regenerated_by: rule.regenerated_by.as_ref().map(text),
            file_list,
            commands,
            project_list,
        })
    }

    pub fn kpis(&self) -> RuleKpisDto {
        let mut k = RuleKpisDto { safe_bytes: 0, safe_count: 0, review_bytes: 0, review_count: 0 };
        let Some(set) = self.set() else { return k };
        for r in &self.analysis.recos {
            if set.rules[r.rule].status == Status::Safe {
                k.safe_bytes += r.allocated;
                k.safe_count += 1;
            } else {
                k.review_bytes += r.allocated;
                k.review_count += 1;
            }
        }
        k
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rules::schema::Var;
    use safety::denylist::{DenyList, SystemLayout};
    use safety::path::WinPath;
    use scan::model::Builder;
    use scan::{Method, RawEntry, Stats};

    fn wp(s: &str) -> WinPath {
        WinPath::parse(s).unwrap()
    }

    fn e(name: &str, kind: Kind, size: u64) -> RawEntry {
        RawEntry { name: name.into(), kind, attributes: 0, file_id: 0, logical: size, allocated: size, modified: 1, accessed: 1 }
    }

    /// Profil analysé : un cache npm (commande officielle) et un cache de shaders (suppression).
    fn applied() -> (Applied, Scan) {
        let mut b = Builder::new(&e("Arthur", Kind::Dir, 0));
        let appdata = b.add_children(0, &[e("AppData", Kind::Dir, 0)]).start;
        let local = b.add_children(appdata, &[e("Local", Kind::Dir, 0)]).start;
        let r = b.add_children(local, &[e("npm-cache", Kind::Dir, 0), e("D3DSCache", Kind::Dir, 0)]);
        b.add_children(r.start, &[e("index", Kind::File, 1000)]);
        b.add_children(r.start + 1, &[e("a.bin", Kind::File, 300), e("b.bin", Kind::File, 200)]);
        let scan = b.finish("C:\\Users\\Arthur".into(), 1, Method::Walk, false, Stats::default());
        let layout = SystemLayout {
            windows: wp("C:\\Windows"),
            program_files: vec![wp("C:\\Program Files")],
            program_data: wp("C:\\ProgramData"),
            users: wp("C:\\Users"),
            profile: wp("C:\\Users\\Arthur"),
            public: wp("C:\\Users\\Public"),
            other_profiles: Vec::new(),
        };
        let mut env = rules::Env::new();
        env.set(Var::UserProfile, wp("C:\\Users\\Arthur")).set(Var::LocalAppData, wp("C:\\Users\\Arthur\\AppData\\Local"));
        let deny = DenyList::new(&layout);
        let ctx = rules::Context { env: &env, deny: &deny, now: now(), read_disk: false };
        let analysis = rules::analyze(builtin().as_ref().unwrap(), &scan, &ctx);
        (Applied { analysis, profile: Some("C:\\Users\\Arthur".into()) }, scan)
    }

    #[test]
    fn recommandations_kpi_et_details() {
        assert_eq!(epoch_ms(116_444_736_000_000_000), 0);
        let (a, scan) = applied();
        let recos = a.recommendations(&scan);
        let ids: Vec<&str> = recos.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["cache-npm", "caches-shaders"], "sans risque, du plus gros au plus petit");
        assert_eq!(recos[0].path, "~\\AppData\\Local\\npm-cache");
        assert_eq!(recos[0].action, "command");
        assert_eq!(recos[1].path, "~\\AppData\\Local\\D3DSCache\\…", "contenu : dossier commun");
        assert_eq!(recos[1].size, 500);
        // Survol : éléments couverts et tous leurs parents (racine comprise).
        assert!(recos[1].highlight.contains(&0) && recos[1].highlight.len() == 6, "{:?}", recos[1].highlight);
        let k = a.kpis();
        assert_eq!((k.safe_bytes, k.safe_count, k.review_count), (1500, 2, 0));
        let d = a.detail(&scan, "cache-npm").unwrap();
        assert_eq!(d.commands.iter().map(|c| c.line.as_str()).collect::<Vec<_>>(), ["npm cache clean --force", "npm cache verify"]);
        assert_eq!(d.file_list[0].path, "~\\AppData\\Local\\npm-cache\\");
        assert!(a.detail(&scan, "inconnue").is_none());
        // Nœuds de la treemap : statut, catégorie, gain.
        let shaders = scan.find("AppData\\Local\\D3DSCache").unwrap();
        let v = a.node(shaders);
        assert_eq!((v.status, v.gain, v.gain_status), ("keep", 500, Some("safe")), "le dossier reste, son contenu part");
        let npm = a.node(scan.find("AppData\\Local\\npm-cache").unwrap());
        assert_eq!((npm.status, npm.category), ("safe", "packages"));
        assert_eq!(a.node(0).gain, 1500);
    }
}
