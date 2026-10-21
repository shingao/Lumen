//! Application des règles à une analyse : ce que chaque règle couvre, statut et catégorie de
//! chaque nœud, gains cumulés, recommandations.
//!
//! Garanties :
//! - une règle de suppression ne couvre **jamais** un élément de la denylist (`safety`) : chaque
//!   élément couvert, et tout ce qu'il contient, passe `DenyList::check` et les vérifications de
//!   faits (attribut SYSTEM, dossier contenant `.git`) ; sinon seuls ses enfants sûrs sont pris ;
//! - un élément n'est couvert que par une seule règle (la première du fichier) ;
//! - les règles « commande » (Windows.old par le Nettoyage de disque…) mesurent seulement : rien
//!   n'est supprimé directement ;
//! - le disque n'est lu que pour `.git` (date du dernier commit) et `.gitignore`, dates de dernier
//!   accès gelées (`fsread`).

use std::collections::HashMap;
use std::path::PathBuf;

use safety::denylist::{DenyList, DenyReason};
use safety::path::{fold, WinPath};
use scan::model::F_HARDLINK_DUP;
use scan::{Kind, NodeId, Scan};

use crate::pattern::{glob, path_components, Env, Pattern, States};
use crate::schema::{Action, Activity, Category, Rule, RuleSet, Scope, Screen, Status};
use crate::{fsread, git};

const ATTR_SYSTEM: u32 = 0x4;
/// Secondes entre 1601 et 1970, pour convertir les dates git en FILETIME.
const EPOCH_1601: i64 = 11_644_473_600;
const NONE: u16 = u16::MAX;

pub struct Context<'a> {
    pub env: &'a Env,
    pub deny: &'a DenyList,
    /// Maintenant, en FILETIME.
    pub now: i64,
    /// Lire `.git` et `.gitignore` sur le disque (faux dans les tests sans disque).
    pub read_disk: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivitySource {
    /// Date du dernier commit (`.git`).
    Git,
    /// Fichier source modifié le plus récemment (projet sans git).
    Files,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub root: NodeId,
    /// Dernière activité, en FILETIME.
    pub last_activity: i64,
    pub source: ActivitySource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reco {
    /// Indice de la règle dans `RuleSet::rules`.
    pub rule: usize,
    /// Éléments couverts (les plus hauts), du plus gros au plus petit.
    pub covered: Vec<NodeId>,
    pub allocated: u64,
    pub logical: u64,
    pub files: u64,
    /// Dossiers parmi les éléments couverts.
    pub dirs: u64,
    /// Dernière utilisation la plus récente, en FILETIME.
    pub last_used: i64,
    pub projects: Vec<Project>,
}

/// Cible neutralisée parce qu'elle tombe dans la denylist sur cette machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disabled {
    pub rule: String,
    pub target: String,
    pub reason: DenyReason,
}

#[derive(Debug, Clone)]
pub struct Analysis {
    /// Par nœud : 0 à garder, 1 sans risque, 2 à vérifier.
    pub status: Vec<u8>,
    /// Par nœud : indice dans `Category::ALL`.
    pub category: Vec<u8>,
    /// Par nœud : octets récupérables dessous (sans risque / à vérifier).
    pub gain_safe: Vec<u64>,
    pub gain_review: Vec<u64>,
    /// Par nœud : recommandation qui le couvre (indice dans `recos`), `u16::MAX` sinon.
    pub reco_of: Vec<u16>,
    pub recos: Vec<Reco>,
    pub disabled: Vec<Disabled>,
}

pub const STATUS_KEEP: u8 = 0;
pub const STATUS_SAFE: u8 = 1;
pub const STATUS_REVIEW: u8 = 2;

fn category_index(c: Category) -> u8 {
    Category::ALL.iter().position(|x| *x == c).unwrap_or(3) as u8
}

/// Ce qu'une correspondance déclenche.
#[derive(Debug, Clone, Copy)]
enum Job {
    Rule { rule: usize },
    Artifact { rule: usize, artifact: usize },
    Area { category: Category },
}

struct Matches {
    /// (job, nœud), dans l'ordre du parcours.
    hits: Vec<(usize, NodeId)>,
}

/// Parcours unique de l'arène avec un automate par motif ; un motif s'arrête à sa première
/// correspondance sur une branche (élément le plus haut).
fn find_matches(scan: &Scan, patterns: &[Pattern]) -> Matches {
    let mut hits = Vec::new();
    let root_comps = WinPath::parse(&scan.root_path).map(|p| path_components(&p)).unwrap_or_default();
    let mut alive: Vec<(usize, States)> = Vec::new();
    for (j, p) in patterns.iter().enumerate() {
        let mut s = p.start();
        for c in &root_comps {
            s = p.step(&s, c);
            if s.is_empty() {
                break;
            }
        }
        if s.is_empty() {
            continue;
        }
        if p.complete(&s) {
            hits.push((j, scan.root()));
        } else {
            alive.push((j, s));
        }
    }
    let mut stack = vec![(scan.root(), alive)];
    while let Some((node, alive)) = stack.pop() {
        for child in scan.children(node) {
            let name = fold(scan.name(child));
            let mut next = Vec::new();
            for (j, s) in &alive {
                let s2 = patterns[*j].step(s, &name);
                if s2.is_empty() {
                    continue;
                }
                if patterns[*j].complete(&s2) {
                    hits.push((*j, child));
                } else {
                    next.push((*j, s2));
                }
            }
            let is_dir = scan.node(child).is_some_and(|n| n.kind == Kind::Dir);
            if is_dir && !next.is_empty() {
                stack.push((child, next));
            }
        }
    }
    Matches { hits }
}

struct Engine<'a> {
    scan: &'a Scan,
    ctx: &'a Context<'a>,
    /// Plus récente date de modification du sous-arbre de chaque nœud.
    newest: Vec<i64>,
    /// Recommandation qui couvre ce nœud (le plus haut couvert seulement).
    top: Vec<u16>,
    /// Un descendant est déjà couvert.
    covered_below: Vec<bool>,
    git_cache: HashMap<PathBuf, Option<i64>>,
}

impl<'a> Engine<'a> {
    fn new(scan: &'a Scan, ctx: &'a Context<'a>) -> Engine<'a> {
        let n = scan.len();
        let mut newest: Vec<i64> = (0..n as u32).map(|i| scan.node(i).map_or(i64::MIN, |x| x.modified)).collect();
        for i in (1..n).rev() {
            if let Some(p) = scan.node(i as u32).map(|x| x.parent as usize) {
                newest[p] = newest[p].max(newest[i]);
            }
        }
        Engine { scan, ctx, newest, top: vec![NONE; n], covered_below: vec![false; n], git_cache: HashMap::new() }
    }

    fn is_dir(&self, n: NodeId) -> bool {
        self.scan.node(n).is_some_and(|x| x.kind == Kind::Dir)
    }

    fn child_named(&self, dir: NodeId, name: &str) -> Option<NodeId> {
        let f = fold(name);
        self.scan.children(dir).find(|&c| fold(self.scan.name(c)) == f)
    }

    fn has_marker(&self, dir: NodeId, markers: &[String]) -> bool {
        self.scan.children(dir).any(|c| {
            let name = fold(self.scan.name(c));
            markers.iter().any(|m| glob(fold(m).as_bytes(), name.as_bytes()))
        })
    }

    fn under_covered(&self, n: NodeId) -> bool {
        let mut cur = n;
        loop {
            if self.top[cur as usize] != NONE {
                return true;
            }
            if cur == self.scan.root() {
                return false;
            }
            cur = self.scan.node(cur).map_or(self.scan.root(), |x| x.parent);
        }
    }

    /// Refus de la denylist pour ce nœud (chemin, attribut SYSTEM, dossier de projet git).
    fn denied(&self, n: NodeId, path: &WinPath) -> bool {
        let Some(node) = self.scan.node(n) else { return true };
        let is_dir = node.kind == Kind::Dir;
        let contains_git = is_dir && self.child_named(n, ".git").is_some();
        self.ctx.deny.check(path).is_some() || self.ctx.deny.check_facts(node.attributes & ATTR_SYSTEM != 0, is_dir, contains_git).is_some()
    }

    /// Tout le sous-arbre passe-t-il la denylist (et rien n'y est déjà couvert) ?
    fn clean_subtree(&self, n: NodeId, path: &WinPath) -> bool {
        let mut stack = vec![(n, path.clone())];
        while let Some((cur, p)) = stack.pop() {
            for c in self.scan.children(cur) {
                let Ok(cp) = p.join(self.scan.name(c)) else { return false };
                if self.top[c as usize] != NONE || self.denied(c, &cp) {
                    return false;
                }
                if self.is_dir(c) {
                    stack.push((c, cp));
                }
            }
        }
        true
    }

    /// Éléments que la règle peut réellement couvrir à partir du candidat `n`.
    fn cover_safely(&self, n: NodeId, out: &mut Vec<NodeId>) {
        if self.under_covered(n) {
            return;
        }
        let Ok(path) = WinPath::parse(&self.scan.path(n)) else { return };
        if self.denied(n, &path) {
            return;
        }
        if !self.covered_below[n as usize] && self.clean_subtree(n, &path) {
            out.push(n);
        } else if self.is_dir(n) {
            for c in self.scan.children(n) {
                self.cover_safely(c, out);
            }
        }
    }

    fn mark(&mut self, n: NodeId, reco: u16) {
        self.top[n as usize] = reco;
        let mut cur = n;
        while cur != self.scan.root() {
            cur = self.scan.node(cur).map_or(self.scan.root(), |x| x.parent);
            self.covered_below[cur as usize] = true;
        }
    }

    fn git_time(&mut self, git_entry: NodeId) -> Option<i64> {
        if !self.ctx.read_disk {
            return None;
        }
        let path = PathBuf::from(self.scan.path(git_entry));
        if let Some(t) = self.git_cache.get(&path) {
            return *t;
        }
        let t = git::last_commit(&path).ok().map(|secs| (secs + EPOCH_1601) * 10_000_000);
        self.git_cache.insert(path, t);
        t
    }

    /// Dernière activité d'un projet : commit le plus récent (`.git` dans le projet ou un parent),
    /// sinon le fichier source modifié le plus récemment, hors artefacts et `.git`.
    fn activity(&mut self, project: NodeId, artifact_names: &[String]) -> (i64, ActivitySource) {
        let mut cur = project;
        loop {
            if let Some(g) = self.child_named(cur, ".git") {
                if let Some(t) = self.git_time(g) {
                    return (t, ActivitySource::Git);
                }
                break;
            }
            if cur == self.scan.root() {
                break;
            }
            cur = self.scan.node(cur).map_or(self.scan.root(), |x| x.parent);
        }
        let skip: Vec<String> = artifact_names.iter().map(|a| fold(a)).chain([".GIT".to_owned()]).collect();
        let mut newest = i64::MIN;
        let mut stack = vec![project];
        while let Some(d) = stack.pop() {
            for c in self.scan.children(d) {
                let Some(node) = self.scan.node(c) else { continue };
                match node.kind {
                    Kind::Dir if !skip.contains(&fold(self.scan.name(c))) => stack.push(c),
                    Kind::File => newest = newest.max(node.modified),
                    _ => {}
                }
            }
        }
        (newest, ActivitySource::Files)
    }

    fn gitignored(&self, project: NodeId, dir: &str) -> bool {
        if !self.ctx.read_disk {
            return false;
        }
        let Some(gi) = self.child_named(project, ".gitignore") else { return false };
        let Ok(text) = fsread::read_limited(&PathBuf::from(self.scan.path(gi)), 1 << 20) else { return false };
        let want = fold(dir);
        String::from_utf8_lossy(&text).lines().any(|l| {
            let l = l.trim();
            !l.starts_with('#') && !l.starts_with('!') && fold(l.trim_start_matches('/').trim_end_matches('/')) == want
        })
    }
}

fn last_used(scan: &Scan, n: NodeId) -> i64 {
    match scan.node(n) {
        Some(x) if x.kind == Kind::Dir => x.accessed,
        Some(x) if scan.last_access_reliable => x.accessed.max(x.modified),
        Some(x) => x.modified,
        None => i64::MIN,
    }
}

fn counted_allocated(scan: &Scan, n: NodeId) -> (u64, u64) {
    match scan.node(n) {
        Some(x) if x.kind == Kind::File && x.flags & F_HARDLINK_DUP != 0 => (0, 0),
        Some(x) if matches!(x.kind, Kind::File | Kind::Dir) => (x.allocated, x.logical),
        _ => (0, 0),
    }
}

/// Applique les règles à l'analyse.
pub fn analyze(set: &RuleSet, scan: &Scan, ctx: &Context) -> Analysis {
    // 1. Motifs développés pour cette machine.
    let mut patterns: Vec<Pattern> = Vec::new();
    let mut jobs: Vec<Job> = Vec::new();
    let mut disabled = Vec::new();
    for (ri, rule) in set.rules.iter().enumerate() {
        for t in &rule.targets {
            let Some(p) = ctx.env.expand(t) else { continue };
            // Cible littérale d'une règle de suppression : vérifiée telle quelle contre la denylist
            // (ex. %TEMP% qui pointerait dans C:\Windows) et neutralisée si elle y tombe.
            if rule.action == Action::Delete && p.fixed_prefix().len() == p.comps.len() {
                if let Some(reason) = literal_path(&p).and_then(|wp| ctx.deny.check(&wp)) {
                    disabled.push(Disabled { rule: rule.id.clone(), target: t.raw.clone(), reason });
                    continue;
                }
            }
            patterns.push(p);
            jobs.push(Job::Rule { rule: ri });
        }
        if let Some(spec) = &rule.project {
            for root in &spec.roots {
                for (ai, a) in spec.artifacts.iter().enumerate() {
                    let mut t = root.clone();
                    t.rest.push("**".into());
                    t.rest.push(a.dir.clone());
                    if let Some(p) = ctx.env.expand(&t) {
                        patterns.push(p);
                        jobs.push(Job::Artifact { rule: ri, artifact: ai });
                    }
                }
            }
        }
    }
    for area in &set.areas {
        for t in &area.targets {
            if let Some(p) = ctx.env.expand(t) {
                patterns.push(p);
                jobs.push(Job::Area { category: area.category });
            }
        }
    }
    let matches = find_matches(scan, &patterns);
    let exclusions: Vec<Vec<Pattern>> = set
        .rules
        .iter()
        .map(|r| r.project.as_ref().map_or_else(Vec::new, |p| p.exclude.iter().filter_map(|t| ctx.env.expand(t)).collect()))
        .collect();

    // 2. Catégories : zones connues, projets (marqueurs), puis héritage.
    let n = scan.len();
    let mut assigned: Vec<u8> = vec![NONE as u8; n];
    for &(j, node) in &matches.hits {
        if let Job::Area { category } = jobs[j] {
            assigned[node as usize] = category_index(category);
        }
    }
    let markers: Vec<String> = set.areas.iter().flat_map(|a| a.project_markers.iter().map(|m| fold(m))).collect();
    if !markers.is_empty() {
        let code = category_index(Category::Code);
        for i in 0..n as u32 {
            if assigned[i as usize] == NONE as u8
                && scan.node(i).is_some_and(|x| x.kind == Kind::Dir)
                && scan.children(i).any(|c| {
                    let name = fold(scan.name(c));
                    markers.iter().any(|m| glob(m.as_bytes(), name.as_bytes()))
                })
            {
                assigned[i as usize] = code;
            }
        }
    }

    // 3. Règles, dans l'ordre du fichier. Chaque élément couvert est marqué aussitôt : une
    // correspondance plus profonde (un `build` dans un `node_modules` déjà couvert) est ignorée.
    let mut engine = Engine::new(scan, ctx);
    let mut recos: Vec<Reco> = Vec::new();
    for (ri, rule) in set.rules.iter().enumerate() {
        let idx = recos.len() as u16;
        let mut covered: Vec<NodeId> = Vec::new();
        let mut projects: Vec<Project> = Vec::new();
        let take = |engine: &mut Engine, c: NodeId, covered: &mut Vec<NodeId>| {
            let before = covered.len();
            match rule.action {
                Action::Delete => engine.cover_safely(c, covered),
                // Commande officielle : mesure seulement, sans compter deux fois la même chose.
                Action::Command => {
                    if !engine.under_covered(c) && !engine.covered_below[c as usize] {
                        covered.push(c);
                    }
                }
            }
            for &x in &covered[before..] {
                engine.mark(x, idx);
            }
        };
        for &(j, node) in &matches.hits {
            match jobs[j] {
                Job::Rule { rule: r } if r == ri => match rule.scope {
                    Scope::Whole => take(&mut engine, node, &mut covered),
                    Scope::Contents => {
                        for c in scan.children(node) {
                            take(&mut engine, c, &mut covered);
                        }
                    }
                    Scope::OlderThan { hours } => {
                        let cutoff = ctx.now - i64::from(hours) * 3600 * 10_000_000;
                        for c in scan.children(node) {
                            if engine.newest[c as usize] < cutoff {
                                take(&mut engine, c, &mut covered);
                            }
                        }
                    }
                },
                Job::Artifact { rule: r, artifact } if r == ri => {
                    let Some(spec) = &rule.project else { continue };
                    let a = &spec.artifacts[artifact];
                    if !engine.is_dir(node) || engine.under_covered(node) {
                        continue;
                    }
                    let Some(project) = scan.node(node).map(|x| x.parent) else { continue };
                    if !engine.has_marker(project, &a.markers)
                        || (!a.requires.is_empty() && !engine.has_marker(project, &a.requires))
                        || (a.gitignored && !engine.gitignored(project, &a.dir))
                        || excluded(scan, project, &exclusions[ri])
                    {
                        continue;
                    }
                    let names: Vec<String> = spec.artifacts.iter().map(|x| x.dir.clone()).collect();
                    let (last, source) = engine.activity(project, &names);
                    let age_days = (ctx.now - last) / (86_400 * 10_000_000);
                    let inactive = last == i64::MIN || age_days >= i64::from(spec.inactive_days);
                    if inactive != (spec.activity == Activity::Inactive) {
                        continue;
                    }
                    let before = covered.len();
                    take(&mut engine, node, &mut covered);
                    if covered.len() > before && !projects.iter().any(|p| p.root == project) {
                        projects.push(Project { root: project, last_activity: last, source });
                    }
                }
                _ => {}
            }
        }
        if covered.is_empty() {
            continue;
        }
        let mut reco = Reco { rule: ri, covered: Vec::new(), allocated: 0, logical: 0, files: 0, dirs: 0, last_used: i64::MIN, projects };
        for &c in &covered {
            let (a, l) = counted_allocated(scan, c);
            reco.allocated += a;
            reco.logical += l;
            reco.files += scan.node(c).map_or(0, |x| u64::from(x.files));
            reco.dirs += u64::from(engine.is_dir(c));
            reco.last_used = reco.last_used.max(last_used(scan, c));
        }
        covered.sort_by_key(|&c| std::cmp::Reverse(counted_allocated(scan, c).0));
        reco.covered = covered;
        recos.push(reco);
    }

    // 4. Statuts, recommandations et catégories par nœud (parents avant enfants), puis gains.
    let mut status = vec![STATUS_KEEP; n];
    let mut reco_of = vec![NONE; n];
    let mut category = vec![category_index(Category::Tools); n];
    let status_of = |r: &Rule| if r.status == Status::Safe { STATUS_SAFE } else { STATUS_REVIEW };
    for i in 0..n {
        let parent = if i == 0 { None } else { scan.node(i as u32).map(|x| x.parent as usize) };
        let top = engine.top[i];
        if top != NONE {
            let rule = &set.rules[recos[top as usize].rule];
            reco_of[i] = top;
            status[i] = status_of(rule);
            category[i] = category_index(rule.category);
        } else if let Some(p) = parent.filter(|&p| reco_of[p] != NONE) {
            reco_of[i] = reco_of[p];
            status[i] = status[p];
            category[i] = category[p];
        } else if assigned[i] != NONE as u8 {
            category[i] = assigned[i];
        } else if let Some(p) = parent {
            category[i] = category[p];
        }
    }
    let mut gain_safe = vec![0u64; n];
    let mut gain_review = vec![0u64; n];
    for i in (0..n).rev() {
        let top = engine.top[i];
        // Règle d'un autre onglet (Jeux) : statut et couleur seulement, aucun gain sur l'écran Analyse.
        if top != NONE && set.rules[recos[top as usize].rule].screen == Screen::Analyse {
            let own = counted_allocated(scan, i as u32).0;
            if set.rules[recos[top as usize].rule].status == Status::Safe {
                gain_safe[i] = own;
            } else {
                gain_review[i] = own;
            }
        }
        if i > 0 {
            if let Some(p) = scan.node(i as u32).map(|x| x.parent as usize) {
                gain_safe[p] += gain_safe[i];
                gain_review[p] += gain_review[i];
            }
        }
    }
    Analysis { status, category, gain_safe, gain_review, reco_of, recos, disabled }
}

/// Le dossier (ou l'un de ses parents) est-il dans une zone exclue ?
fn excluded(scan: &Scan, node: NodeId, exclude: &[Pattern]) -> bool {
    if exclude.is_empty() {
        return false;
    }
    let Ok(path) = WinPath::parse(&scan.path(node)) else { return true };
    let comps = path_components(&path);
    (1..=comps.len()).any(|k| exclude.iter().any(|p| p.matches(&comps[..k])))
}

/// Chemin d'un motif entièrement littéral.
fn literal_path(p: &Pattern) -> Option<WinPath> {
    let comps = p.fixed_prefix();
    let (head, rest) = comps.split_first()?;
    let mut s = if head.ends_with(':') { format!("{head}\\") } else { head.clone() };
    s.push_str(&rest.join("\\"));
    WinPath::parse(&s).ok()
}
