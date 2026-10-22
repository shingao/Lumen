//! Onglet Dev : projets détectés dans l'analyse affichée (`.git`, `package.json`, `Cargo.toml`,
//! `pyproject.toml`, `build.gradle`, `*.csproj`, `go.mod`), avec leur langage, leur dernière
//! activité (dernier commit lu sans git, sinon fichier le plus récent), la taille du source et
//! celle des artefacts régénérables. Les artefacts sont exactement ceux que les règles
//! `artefacts-projets-*` couvrent : la purge ne touche jamais le source ni `.git`, et passe par
//! `safety` comme tout nettoyage. Caches globaux par leurs commandes officielles, Docker par
//! `docker image prune` (images sans étiquette, listées avant).

pub mod docker;
pub mod wsl;

use std::collections::HashMap;

use rules::engine::ActivitySource;
use safety::allow::{Allowlist, Authority};
use safety::check::Target;
use scan::{Kind, NodeId, Scan};
use serde::Serialize;

use crate::analysis::{epoch_ms, Applied};
use crate::cleanup::{bytes_of, expected, grant_for, tree, Selection};
use crate::error::LumenError;

/// Règles des artefacts de projets (rules.toml) : projets actifs (sans risque), inactifs (à vérifier).
pub const ARTIFACT_RULES: [&str; 2] = ["artefacts-projets-actifs", "artefacts-projets-inactifs"];

const LOCKFILES: [&str; 6] = ["package-lock.json", "npm-shrinkwrap.json", "yarn.lock", "pnpm-lock.yaml", "bun.lockb", "bun.lock"];
/// Manifeste lu au plus (nom du projet).
const MAX_MANIFEST: u64 = 1 << 20;

fn is_marker(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(n.as_str(), ".git" | "package.json" | "cargo.toml" | "pyproject.toml" | "build.gradle" | "build.gradle.kts" | "settings.gradle" | "settings.gradle.kts" | "go.mod")
        || n.ends_with(".csproj")
        || n.ends_with(".fsproj")
        || n.ends_with(".vbproj")
}

/// Dossiers où l'on ne cherche pas de projet : applis, caches, artefacts, dossiers cachés.
fn skipped(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with('.') || n.starts_with('$') || matches!(n.as_str(), "appdata" | "node_modules" | "windows" | "program files" | "program files (x86)" | "programdata")
}

/// Langage d'un projet d'après ses fichiers de premier niveau.
pub fn language(names: &[String]) -> Option<&'static str> {
    let has = |m: &str| names.iter().any(|n| n.eq_ignore_ascii_case(m));
    let ends = |e: &str| names.iter().any(|n| n.to_ascii_lowercase().ends_with(e));
    Some(if has("Cargo.toml") {
        "Rust"
    } else if has("go.mod") {
        "Go"
    } else if has("pyproject.toml") {
        "Python"
    } else if ends(".csproj") {
        "C#"
    } else if ends(".fsproj") {
        "F#"
    } else if has("build.gradle.kts") || has("settings.gradle.kts") {
        "Kotlin"
    } else if has("build.gradle") || has("settings.gradle") {
        "Java"
    } else if has("package.json") && has("tsconfig.json") {
        "TypeScript"
    } else if has("package.json") {
        "JavaScript"
    } else {
        return None;
    })
}

/// Nom déclaré dans un manifeste (lecture tolérante : tronqué ou mal formé = aucun nom).
pub fn manifest_name(file: &str, text: &str) -> Option<String> {
    let name = if file.eq_ignore_ascii_case("package.json") {
        serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')).ok()?.get("name")?.as_str()?.trim().to_owned()
    } else {
        // Cargo.toml ([package]) et pyproject.toml ([project] ou [tool.poetry]) : `name = "…"`.
        let mut section = String::new();
        let mut found = None;
        for line in text.lines() {
            let l = line.trim();
            if l.starts_with('[') {
                section = l.trim_matches(['[', ']']).trim().to_owned();
            } else if matches!(section.as_str(), "package" | "project" | "tool.poetry") {
                if let Some(v) = l.strip_prefix("name").map(str::trim_start).and_then(|r| r.strip_prefix('=')) {
                    let v = v.trim();
                    let q = v.chars().next().filter(|c| *c == '"' || *c == '\'')?;
                    found = v[1..].find(q).map(|end| v[1..1 + end].trim().to_owned());
                    break;
                }
            }
        }
        found?
    };
    (!name.is_empty() && name.len() <= 214).then_some(name)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactDto {
    pub id: i64,
    /// Chemin relatif au projet (`node_modules`, `packages\web\.next`…).
    pub path: String,
    pub size: u64,
    /// `safe` (projet actif, fichier de verrouillage présent pour node_modules) ou `review`.
    pub status: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub language: Option<&'static str>,
    /// Dernière activité (ms) : dernier commit, sinon fichier modifié le plus récemment.
    pub last_activity: Option<i64>,
    /// `git` ou `files`.
    pub activity_source: &'static str,
    pub git: bool,
    pub lockfile: bool,
    pub source_size: u64,
    pub git_size: u64,
    pub artifacts_size: u64,
    pub artifacts: Vec<ArtifactDto>,
}

/// Artefacts couverts par les règles, par nœud : (indice de la reco, statut).
fn artifact_cover(applied: &Applied) -> HashMap<NodeId, (usize, &'static str)> {
    let mut out = HashMap::new();
    let Some(set) = applied.set() else { return out };
    for (i, reco) in applied.analysis.recos.iter().enumerate() {
        let rule = &set.rules[reco.rule];
        if let Some(pos) = ARTIFACT_RULES.iter().position(|r| *r == rule.id) {
            for &n in &reco.covered {
                out.insert(n, (i, if pos == 0 { "safe" } else { "review" }));
            }
        }
    }
    out
}

fn child_names(scan: &Scan, id: NodeId) -> Vec<String> {
    scan.children(id).map(|c| scan.name(c).to_owned()).collect()
}

/// Racines de projets (la plus haute : un monorepo est un seul projet).
pub fn find_projects(scan: &Scan) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack: Vec<NodeId> = scan.children(scan.root()).filter(|&c| scan.node(c).is_some_and(|n| n.kind == Kind::Dir)).collect();
    while let Some(id) = stack.pop() {
        if skipped(scan.name(id)) {
            continue;
        }
        if scan.children(id).any(|c| is_marker(scan.name(c))) {
            out.push(id);
            continue;
        }
        stack.extend(scan.children(id).filter(|&c| scan.node(c).is_some_and(|n| n.kind == Kind::Dir)));
    }
    out.sort_unstable();
    out
}

fn is_under(scan: &Scan, mut n: NodeId, ancestor: NodeId) -> bool {
    while let Some(node) = scan.node(n) {
        if n == ancestor {
            return true;
        }
        if node.parent == n {
            return false;
        }
        n = node.parent;
    }
    false
}

/// Fichier modifié le plus récemment (FILETIME), hors `.git` et artefacts.
fn newest_file(scan: &Scan, id: NodeId, artifacts: &[NodeId]) -> Option<i64> {
    let mut best: Option<i64> = None;
    let mut stack = vec![id];
    while let Some(n) = stack.pop() {
        for c in scan.children(n) {
            let Some(node) = scan.node(c) else { continue };
            if artifacts.contains(&c) || scan.name(c).eq_ignore_ascii_case(".git") {
                continue;
            }
            match node.kind {
                Kind::Dir => stack.push(c),
                Kind::File => best = best.max(Some(node.modified)),
                _ => {}
            }
        }
    }
    best
}

/// Lecture des fichiers d'un projet (nom, dernier commit), absente dans les tests sans disque.
pub trait ProjectDisk {
    fn manifest(&self, path: &str) -> Option<String>;
    /// Dernier commit (secondes Unix) du dépôt `<projet>\.git`.
    fn last_commit(&self, git: &str) -> Option<i64>;
}

/// Disque réel : dates de dernier accès gelées (`rules::fsread`), git lu sans le lancer.
pub struct RealDisk;

impl ProjectDisk for RealDisk {
    fn manifest(&self, path: &str) -> Option<String> {
        rules::fsread::read_limited(std::path::Path::new(path), MAX_MANIFEST).ok().map(|b| String::from_utf8_lossy(&b).into_owned())
    }

    fn last_commit(&self, git: &str) -> Option<i64> {
        rules::git::last_commit(std::path::Path::new(git)).ok()
    }
}

pub fn projects(applied: &Applied, scan: &Scan, disk: &dyn ProjectDisk) -> Vec<ProjectDto> {
    let cover = artifact_cover(applied);
    // Dernière activité connue du moteur de règles (racines de projets des règles d'artefacts).
    let mut engine: HashMap<NodeId, (i64, ActivitySource)> = HashMap::new();
    for reco in &applied.analysis.recos {
        for p in &reco.projects {
            engine.insert(p.root, (p.last_activity, p.source));
        }
    }
    let mut out = Vec::new();
    for root in find_projects(scan) {
        let names = child_names(scan, root);
        let path = scan.path(root);
        let git_node = scan.children(root).find(|&c| scan.name(c).eq_ignore_ascii_case(".git"));
        let artifacts: Vec<NodeId> = cover.keys().copied().filter(|&a| is_under(scan, a, root)).collect();
        let mut arts: Vec<ArtifactDto> = artifacts
            .iter()
            .map(|&a| ArtifactDto {
                id: a as i64,
                path: scan.path(a).get(path.len()..).unwrap_or_default().trim_start_matches('\\').to_owned(),
                size: bytes_of(scan, a),
                status: cover.get(&a).map_or("review", |c| c.1),
            })
            .collect();
        arts.sort_by(|x, y| y.size.cmp(&x.size).then_with(|| x.path.cmp(&y.path)));
        let total = scan.node(root).map_or(0, |n| n.allocated);
        let artifacts_size: u64 = arts.iter().map(|a| a.size).sum();
        let git_size = git_node.and_then(|g| scan.node(g)).map_or(0, |n| n.allocated);
        let (last, source) = match (engine.get(&root), git_node.and_then(|g| disk.last_commit(&scan.path(g)))) {
            (Some(&(t, s)), _) => (Some(epoch_ms(t)), if s == ActivitySource::Git { "git" } else { "files" }),
            (None, Some(secs)) => (Some(secs * 1000), "git"),
            (None, None) => (newest_file(scan, root, &artifacts).map(epoch_ms), "files"),
        };
        let name = ["package.json", "Cargo.toml", "pyproject.toml"]
            .iter()
            .filter(|f| names.iter().any(|n| n.eq_ignore_ascii_case(f)))
            .find_map(|f| disk.manifest(&format!("{path}\\{f}")).and_then(|t| manifest_name(f, &t)))
            .unwrap_or_else(|| scan.name(root).to_owned());
        out.push(ProjectDto {
            id: root as i64,
            name,
            path: applied.display(&path),
            language: language(&names),
            last_activity: last,
            activity_source: source,
            git: git_node.is_some(),
            lockfile: names.iter().any(|n| LOCKFILES.iter().any(|l| n.eq_ignore_ascii_case(l))),
            source_size: total.saturating_sub(artifacts_size).saturating_sub(git_size),
            git_size,
            artifacts_size,
            artifacts: arts,
        });
    }
    out
}

/// Sélection d'artefacts cochés : uniquement des nœuds couverts par les règles d'artefacts, avec
/// l'autorisation de leur règle ; jamais un autre identifiant (source, `.git`…).
pub fn select_artifacts(applied: &Applied, scan: &Scan, ids: &[i64]) -> Result<Selection, LumenError> {
    let set = applied.set().ok_or_else(|| LumenError::Plan("règles non appliquées".into()))?;
    let cover = artifact_cover(applied);
    let mut sel = Selection { targets: Vec::new(), allow: Allowlist::new(), commands: Vec::new(), rule_names: HashMap::new(), elevated: false, guard: None };
    for &id in ids {
        let n = NodeId::try_from(id).ok().filter(|n| cover.contains_key(n)).ok_or(LumenError::UnknownNode)?;
        let reco = &applied.analysis.recos[cover[&n].0];
        let rule = &set.rules[reco.rule];
        sel.rule_names.insert(rule.id.clone(), (rule.name.fr.clone(), rule.name.en.clone()));
        let grant = grant_for(applied, scan, rule, n).ok_or(LumenError::UnknownNode)?;
        sel.allow.grant(grant);
        let exp = expected(scan, n).ok_or(LumenError::UnknownNode)?;
        let contents = exp.directory.then(|| tree(scan, n));
        sel.targets.push(Target { path: scan.path(n), authority: Authority::Rule(rule.id.clone()), expected: exp, bytes: bytes_of(scan, n), contents, hash: None });
    }
    if sel.targets.is_empty() {
        return Err(LumenError::Plan("aucun artefact choisi".into()));
    }
    Ok(sel)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevDto {
    pub projects: Vec<ProjectDto>,
    /// Caches globaux de paquets (npm, pnpm, Yarn, pip, uv, NuGet, Gradle ; registre Cargo et
    /// .m2 « À vérifier ») : recommandations de l'analyse, nettoyées par leurs commandes officielles.
    pub caches: Vec<crate::analysis::RecoDto>,
}

/// Projets et caches de l'analyse affichée (lecture des manifestes et de `.git` hors du fil de
/// l'interface).
#[tauri::command]
pub async fn dev_overview(app: tauri::AppHandle, scan_id: u32) -> Result<DevDto, LumenError> {
    use tauri::Manager;
    let entry = crate::scanning::entry_of(&app.state::<crate::scanning::ScanState>(), scan_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let applied = entry.applied.as_ref().ok_or_else(|| LumenError::Plan("règles non appliquées".into()))?;
        let caches = applied.recommendations(&entry.scan).into_iter().filter(|r| r.category == "packages").collect();
        Ok(DevDto { projects: projects(applied, &entry.scan, &RealDisk), caches })
    })
    .await
    .map_err(|e| LumenError::Plan(e.to_string()))?
}

mod platform {
    #[cfg(windows)]
    pub fn docker_images() -> Option<safety::exec::CommandRun> {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        safety::win::run_read_only(&safety::commands::OfficialCommand::DockerDanglingImages, &cancel)
    }

    #[cfg(not(windows))]
    pub fn docker_images() -> Option<safety::exec::CommandRun> {
        None
    }
}

/// Images Docker sans étiquette (lecture seule) ; Docker arrêté ou absent : dit tel quel.
pub fn docker_now() -> docker::DockerDto {
    docker::dto(platform::docker_images().as_ref())
}

#[tauri::command]
pub async fn docker_images() -> Result<docker::DockerDto, LumenError> {
    tauri::async_runtime::spawn_blocking(docker_now).await.map_err(|e| LumenError::Plan(e.to_string()))
}

/// `docker image prune` (images sans étiquette), seulement si Docker tourne et qu'il y a des
/// images à retirer ; le gain estimé est la somme des images listées.
pub fn select_docker() -> Result<Selection, LumenError> {
    let d = docker_now();
    if !d.running {
        return Err(LumenError::Plan("Docker ne tourne pas : rien à faire".into()));
    }
    if d.images.is_empty() {
        return Err(LumenError::Plan("aucune image Docker sans étiquette".into()));
    }
    let command = safety::plan::PlannedCommand::action(safety::commands::OfficialCommand::DockerImagePrune, None, d.total);
    Ok(Selection { targets: Vec::new(), allow: Allowlist::new(), commands: vec![command], rule_names: HashMap::new(), elevated: false, guard: None })
}

/// Dernière liste des disques virtuels (le front coche des identifiants, jamais des chemins).
#[derive(Default)]
pub struct DevState(std::sync::Mutex<Vec<std::path::PathBuf>>);

mod wsl_platform {
    use super::wsl::Distro;

    #[cfg(windows)]
    pub fn distros() -> Vec<Distro> {
        use scan::win::registry::{string, subkeys, Hive, View};
        let root = "Software\\Microsoft\\Windows\\CurrentVersion\\Lxss";
        subkeys(Hive::CurrentUser, View::Native, root)
            .into_iter()
            .filter_map(|k| {
                let path = format!("{root}\\{k}");
                Some(Distro {
                    name: string(Hive::CurrentUser, View::Native, &path, "DistributionName")?,
                    base_path: string(Hive::CurrentUser, View::Native, &path, "BasePath")?,
                    vhd: string(Hive::CurrentUser, View::Native, &path, "VhdFileName"),
                })
            })
            .collect()
    }

    #[cfg(not(windows))]
    pub fn distros() -> Vec<Distro> {
        Vec::new()
    }

    pub fn hyperv() -> bool {
        std::env::var_os("SystemRoot").map(std::path::PathBuf::from).is_some_and(|r| r.join("System32").join("WindowsPowerShell").join("v1.0").join("Modules").join("Hyper-V").is_dir())
    }
}

/// Disques virtuels WSL et Docker et leur taille (réglage Expérimental côté interface).
#[tauri::command]
pub fn wsl_disks(state: tauri::State<'_, DevState>) -> wsl::WslDto {
    let local = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
    let size = |p: &std::path::Path| std::fs::metadata(p).ok().filter(|m| m.is_file()).map(|m| m.len());
    let list = wsl::disks(&wsl_platform::distros(), local.as_deref(), &size);
    if let Ok(mut s) = state.0.lock() {
        *s = list.iter().map(|(_, p)| p.clone()).collect();
    }
    wsl::WslDto { disks: list.into_iter().map(|(d, _)| d).collect(), hyperv: wsl_platform::hyperv() }
}

/// Chemins des disques cochés, d'après la dernière liste.
pub fn wsl_paths(state: &DevState, ids: &[u32]) -> Result<Vec<std::path::PathBuf>, LumenError> {
    let list = state.0.lock().map_err(|_| LumenError::Plan("état indisponible".into()))?;
    ids.iter().map(|&i| list.get(i as usize).cloned().ok_or(LumenError::UnknownNode)).collect()
}

#[cfg(test)]
mod tests;
