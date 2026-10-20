//! Tests du moteur sur des arborescences construites en mémoire (aucun disque) et sur les règles
//! livrées : schéma, liste des règles « Sans risque » à relire, denylist jamais couverte.

use std::collections::BTreeMap;

use safety::denylist::{DenyList, DenyReason, SystemLayout};
use safety::path::WinPath;
use scan::model::Builder;
use scan::{Kind, Method, NodeId, RawEntry, Scan, Stats};

use crate::engine::{analyze, Analysis, Context, STATUS_KEEP, STATUS_REVIEW, STATUS_SAFE};
use crate::pattern::Env;
use crate::schema::{Action, Category, RuleSet, Scope, Status, Var};
use crate::{builtin, BUILTIN};

/// Maintenant : 20/10/2026 à midi, en FILETIME.
const NOW: i64 = (1_792_497_600 + 11_644_473_600) * 10_000_000;
const DAY: i64 = 86_400 * 10_000_000;
const ATTR_SYSTEM: u32 = 0x4;

fn wp(s: &str) -> WinPath {
    WinPath::parse(s).unwrap()
}

/// Arborescence de test : chemin relatif à la racine → (type, taille, âge en jours, attributs).
type Spec = (Kind, u64, i64, u32);

#[derive(Default)]
struct Tree {
    entries: BTreeMap<String, Spec>,
}

impl Tree {
    fn file(&mut self, path: &str, size: u64, age_days: i64) -> &mut Tree {
        self.entry(path, Kind::File, size, age_days, 0)
    }

    fn entry(&mut self, path: &str, kind: Kind, size: u64, age_days: i64, attrs: u32) -> &mut Tree {
        // Dossiers parents implicites.
        let parts: Vec<&str> = path.split('\\').collect();
        for k in 1..parts.len() {
            self.entries.entry(parts[..k].join("\\")).or_insert((Kind::Dir, 0, age_days, 0x10));
        }
        self.entries.insert(path.to_owned(), (kind, size, age_days, attrs));
        self
    }

    fn dir(&mut self, path: &str, age_days: i64) -> &mut Tree {
        self.entry(path, Kind::Dir, 0, age_days, 0x10)
    }

    fn build(&self, root: &str) -> Scan {
        let mut next_id = 100u128;
        let mut raw = |name: &str, kind: Kind, size: u64, age: i64, attrs: u32| {
            next_id += 1;
            RawEntry { name: name.into(), kind, attributes: attrs, file_id: next_id, logical: size, allocated: size, modified: NOW - age * DAY, accessed: NOW - age * DAY }
        };
        let root_name = root.trim_end_matches('\\').rsplit('\\').next().unwrap_or(root).to_owned();
        let mut b = Builder::new(&raw(&root_name, Kind::Dir, 0, 0, 0x10));
        let mut queue = vec![(String::new(), 0 as NodeId)];
        while let Some((prefix, id)) = queue.pop() {
            let children: Vec<(&String, &Spec)> = self
                .entries
                .iter()
                .filter(|(p, _)| match p.rsplit_once('\\') {
                    Some((parent, _)) => parent == prefix,
                    None => prefix.is_empty(),
                })
                .collect();
            let entries: Vec<RawEntry> = children.iter().map(|(p, (k, s, a, at))| raw(p.rsplit('\\').next().unwrap(), *k, *s, *a, *at)).collect();
            let range = b.add_children(id, &entries);
            for (i, (p, (k, ..))) in children.iter().enumerate() {
                if *k == Kind::Dir {
                    queue.push(((*p).clone(), range.start + i as NodeId));
                }
            }
        }
        b.finish(root.to_owned(), 7, Method::Walk, true, Stats::default())
    }
}

fn layout(profile: &str) -> SystemLayout {
    SystemLayout {
        windows: wp("C:\\Windows"),
        program_files: vec![wp("C:\\Program Files"), wp("C:\\Program Files (x86)")],
        program_data: wp("C:\\ProgramData"),
        users: wp("C:\\Users"),
        profile: wp(profile),
        public: wp("C:\\Users\\Public"),
        other_profiles: vec![wp("C:\\Users\\Bob")],
    }
}

fn env(profile: &str, temp: &str) -> Env {
    let mut e = Env::new();
    let local = format!("{profile}\\AppData\\Local");
    e.set(Var::UserProfile, wp(profile))
        .set(Var::LocalAppData, wp(&local))
        .set(Var::AppData, wp(&format!("{profile}\\AppData\\Roaming")))
        .set(Var::Temp, wp(temp))
        .set(Var::SystemDrive, wp("C:\\"))
        .set(Var::Documents, wp(&format!("{profile}\\Documents")))
        .set(Var::Downloads, wp(&format!("{profile}\\Downloads")))
        .set(Var::WinDir, wp("C:\\Windows"))
        .set(Var::ProgramFiles, wp("C:\\Program Files"))
        .set(Var::ProgramFilesX86, wp("C:\\Program Files (x86)"))
        .set(Var::ProgramData, wp("C:\\ProgramData"));
    e
}

/// Volume C: complet, avec tout ce que la denylist protège et tout ce que les règles visent,
/// y compris des pièges : `.git`, ruche, fichier SYSTEM et projet git dans %TEMP%, appli
/// Electron dans AppData, profil d'un autre utilisateur, Windows.old.
fn machine() -> Tree {
    let mut t = Tree::default();
    let p = "Users\\Arthur";
    t.file("pagefile.sys", 8 << 30, 1)
        .file("Windows\\System32\\config\\SYSTEM", 50 << 20, 1)
        .file("Windows\\Temp\\vieux.tmp", 3 << 20, 30)
        .file("Program Files\\Appli\\appli.exe", 90 << 20, 100)
        .file("ProgramData\\Cache\\x.bin", 1 << 20, 100)
        .file("Windows.old\\Windows\\explorer.exe", 7 << 30, 300)
        .file("$Recycle.Bin\\S-1-5-21\\$R1.txt", 1 << 20, 10)
        .file("System Volume Information\\tracking.log", 1 << 10, 10)
        .file("Users\\Public\\Documents\\partage.docx", 1 << 20, 10)
        .file("Users\\Bob\\Projets\\site\\package.json", 1 << 10, 1)
        .file("Users\\Bob\\Projets\\site\\package-lock.json", 1 << 10, 1)
        .file("Users\\Bob\\Projets\\site\\node_modules\\react\\index.js", 9 << 20, 1)
        .file(&format!("{p}\\NTUSER.DAT"), 8 << 20, 1)
        .file(&format!("{p}\\AppData\\Local\\Microsoft\\Windows\\UsrClass.dat"), 4 << 20, 1)
        .file(&format!("{p}\\AppData\\Local\\npm-cache\\_cacache\\content-v2\\a"), 13 << 20, 2)
        .file(&format!("{p}\\AppData\\Local\\D3DSCache\\abc\\shader.bin"), 800 << 10, 5)
        .file(&format!("{p}\\AppData\\Local\\Temp\\installeur-ancien\\setup.msi"), 2 << 20, 10)
        .file(&format!("{p}\\AppData\\Local\\Temp\\recent.tmp"), 1 << 20, 0)
        .file(&format!("{p}\\AppData\\Local\\Temp\\vieux.log"), 1 << 20, 5)
        .file(&format!("{p}\\AppData\\Local\\Temp\\NTUSER.DAT.bak"), 1 << 20, 5)
        .entry(&format!("{p}\\AppData\\Local\\Temp\\systeme.bin"), Kind::File, 1 << 20, 5, ATTR_SYSTEM)
        .file(&format!("{p}\\AppData\\Local\\Temp\\clone\\.git\\HEAD"), 1 << 10, 5)
        .file(&format!("{p}\\AppData\\Local\\Temp\\clone\\src\\main.rs"), 1 << 10, 5)
        .file(&format!("{p}\\AppData\\Local\\Temp\\mixte\\a.tmp"), 1 << 20, 5)
        .file(&format!("{p}\\AppData\\Local\\Temp\\mixte\\.git"), 1 << 10, 5)
        .file(&format!("{p}\\AppData\\Local\\Programs\\Editeur\\resources\\app\\package.json"), 1 << 10, 0)
        .file(&format!("{p}\\AppData\\Local\\Programs\\Editeur\\resources\\app\\package-lock.json"), 1 << 10, 0)
        .file(&format!("{p}\\AppData\\Local\\Programs\\Editeur\\resources\\app\\node_modules\\x\\i.js"), 30 << 20, 0)
        .file(&format!("{p}\\.vscode\\extensions\\ext\\package.json"), 1 << 10, 0)
        .file(&format!("{p}\\.vscode\\extensions\\ext\\package-lock.json"), 1 << 10, 0)
        .file(&format!("{p}\\.vscode\\extensions\\ext\\node_modules\\y\\i.js"), 20 << 20, 0)
        .file(&format!("{p}\\.gradle\\caches\\modules-2\\a.jar"), 6 << 20, 3)
        .file(&format!("{p}\\.cargo\\registry\\cache\\x.crate"), 2 << 20, 3)
        .file(&format!("{p}\\.ollama\\models\\blobs\\sha256-1"), 4 << 30, 20)
        .file(&format!("{p}\\Documents\\rapport.docx"), 2 << 20, 3)
        // Projet actif : verrouillage présent, source modifiée il y a 2 jours.
        .file(&format!("{p}\\Projets\\actif\\package.json"), 1 << 10, 2)
        .file(&format!("{p}\\Projets\\actif\\package-lock.json"), 1 << 10, 2)
        .file(&format!("{p}\\Projets\\actif\\src\\index.ts"), 1 << 10, 2)
        .file(&format!("{p}\\Projets\\actif\\node_modules\\react\\index.js"), 300 << 20, 2)
        .file(&format!("{p}\\Projets\\actif\\node_modules\\pkg\\build\\out.js"), 1 << 20, 2)
        .file(&format!("{p}\\Projets\\actif\\node_modules\\pkg\\build.gradle"), 1 << 10, 2)
        .file(&format!("{p}\\Projets\\actif\\.next\\cache\\x"), 90 << 20, 2)
        .file(&format!("{p}\\Projets\\actif\\dist\\bundle.js"), 5 << 20, 2)
        // Projet actif sans fichier de verrouillage : node_modules non régénérable à l'identique.
        .file(&format!("{p}\\Projets\\sans-verrou\\package.json"), 1 << 10, 2)
        .file(&format!("{p}\\Projets\\sans-verrou\\node_modules\\a\\i.js"), 40 << 20, 2)
        // Projet inactif : rien de modifié depuis 300 jours.
        .file(&format!("{p}\\Projets\\vieux\\Cargo.toml"), 1 << 10, 300)
        .file(&format!("{p}\\Projets\\vieux\\src\\main.rs"), 1 << 10, 300)
        .file(&format!("{p}\\Projets\\vieux\\target\\debug\\app.exe"), 700 << 20, 300);
    t
}

fn run(set: &RuleSet, scan: &Scan, profile: &str, temp: &str) -> (Analysis, DenyList) {
    let deny = DenyList::new(&layout(profile));
    let e = env(profile, temp);
    let ctx = Context { env: &e, deny: &deny, now: NOW, read_disk: false };
    (analyze(set, scan, &ctx), deny)
}

fn covered_paths(a: &Analysis, scan: &Scan, set: &RuleSet, id: &str) -> Vec<String> {
    let mut v: Vec<String> = a
        .recos
        .iter()
        .filter(|r| set.rules[r.rule].id == id)
        .flat_map(|r| r.covered.iter().map(|&c| scan.path(c)))
        .collect();
    v.sort();
    v
}

/// Chaque élément couvert par une règle de suppression, et tout ce qu'il contient, passe la
/// denylist : jamais une zone protégée, jamais un fichier SYSTEM, jamais un projet git.
fn assert_never_denied(a: &Analysis, scan: &Scan, set: &RuleSet, deny: &DenyList) {
    for reco in &a.recos {
        if set.rules[reco.rule].action == Action::Command {
            continue;
        }
        let mut stack: Vec<NodeId> = reco.covered.clone();
        while let Some(n) = stack.pop() {
            let path = scan.path(n);
            let node = scan.node(n).unwrap();
            let is_dir = node.kind == Kind::Dir;
            let has_git = scan.children(n).any(|c| scan.name(c).eq_ignore_ascii_case(".git"));
            assert_eq!(deny.check(&wp(&path)), None, "{} couvre {path}", set.rules[reco.rule].id);
            assert_eq!(deny.check_facts(node.attributes & ATTR_SYSTEM != 0, is_dir, has_git), None, "{} couvre {path}", set.rules[reco.rule].id);
            stack.extend(scan.children(n));
        }
    }
}

#[test]
fn regles_livrees_valides() {
    let set = builtin().unwrap();
    assert!(set.rules.len() >= 15, "{}", set.rules.len());
    // Aucun « À vérifier » ne peut être une règle qui se coche seule : le statut est explicite.
    assert!(set.rules.iter().any(|r| r.status == Status::Review));
    assert!(BUILTIN.contains("version = 1"));
}

/// Liste des règles « Sans risque », relue par un humain : toute règle ajoutée ou modifiée avec
/// ce statut fait échouer ce test tant que `sans-risque.txt` n'est pas régénéré (et relu dans la
/// PR). Régénérer : `LUMEN_UPDATE_RULE_LISTS=1 cargo test -p rules`.
#[test]
fn liste_des_regles_sans_risque_a_relire() {
    let set = builtin().unwrap();
    let mut out = String::from(
        "# Règles « Sans risque » de Lumen : uniquement des données régénérables.\n# Fichier généré par le test liste_des_regles_sans_risque_a_relire (rules/src/tests.rs) : à relire à chaque changement.\n",
    );
    for r in set.rules.iter().filter(|r| r.status == Status::Safe) {
        out.push_str(&format!("\n[{}] {}\n", r.id, r.name.fr));
        let action = match r.action {
            Action::Delete => "suppression (Corbeille par défaut, via safety)",
            Action::Command => "commande officielle",
        };
        out.push_str(&format!("  action : {action}\n"));
        if !r.commands.is_empty() {
            let cmds: Vec<String> = r
                .commands
                .iter()
                .filter_map(|c| c.official(Some('C')))
                .map(|c| format!("{} {}", c.program().binary(), c.args().join(" ")))
                .collect();
            out.push_str(&format!("  commandes : {}\n", cmds.join(" ; ")));
        }
        let scope = match r.scope {
            Scope::Whole => "la cible entière".to_owned(),
            Scope::Contents => "son contenu".to_owned(),
            Scope::OlderThan { hours } => format!("son contenu inchangé depuis {hours} h"),
        };
        for t in &r.targets {
            out.push_str(&format!("  cible : {} ({scope})\n", t.raw));
        }
        if let Some(p) = &r.project {
            let roots: Vec<&str> = p.roots.iter().map(|t| t.raw.as_str()).collect();
            let excl: Vec<&str> = p.exclude.iter().map(|t| t.raw.as_str()).collect();
            out.push_str(&format!("  projets sous : {} ; jamais sous : {}\n", roots.join(", "), excl.join(", ")));
            out.push_str(&format!("  projets {} (activité depuis moins de {} jours)\n", if p.activity == crate::schema::Activity::Active { "actifs" } else { "inactifs" }, p.inactive_days));
            for a in &p.artifacts {
                let mut line = format!("  artefact : {} si {}", a.dir, a.markers.join(" | "));
                if !a.requires.is_empty() {
                    line.push_str(&format!(" et {}", a.requires.join(" | ")));
                }
                if a.gitignored {
                    line.push_str(" et exclu par .gitignore");
                }
                out.push_str(&line);
                out.push('\n');
            }
        }
        if let Some(t) = &r.regenerated_by {
            out.push_str(&format!("  régénéré par : {}\n", t.fr));
        }
    }
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("sans-risque.txt");
    if std::env::var("LUMEN_UPDATE_RULE_LISTS").is_ok() {
        std::fs::write(&file, &out).unwrap();
    }
    let committed = std::fs::read_to_string(&file).unwrap_or_default().replace("\r\n", "\n");
    assert_eq!(committed, out, "sans-risque.txt n'est plus à jour : régénère-le et relis-le (LUMEN_UPDATE_RULE_LISTS=1)");
}

/// Les règles elles-mêmes ne visent aucune zone de la denylist, sur des dispositions variées.
#[test]
fn aucune_cible_de_regle_dans_la_denylist() {
    let set = builtin().unwrap();
    for profile in ["C:\\Users\\Arthur", "D:\\Comptes\\claire.DOMAINE", "C:\\Users\\Élodie Martin"] {
        let deny = DenyList::new(&layout(profile));
        let e = env(profile, &format!("{profile}\\AppData\\Local\\Temp"));
        for r in set.rules.iter().filter(|r| r.action == Action::Delete) {
            let targets = r.targets.iter().chain(r.project.iter().flat_map(|p| p.roots.iter()));
            for t in targets {
                let p = e.expand(t).unwrap();
                let prefix = p.fixed_prefix();
                // Un élément sous la cible (motif : sous son préfixe fixe) n'est jamais protégé.
                let probe = format!("{}\\{}\\lumen-sonde", prefix[0], prefix[1..].join("\\"));
                assert_eq!(deny.check(&wp(&probe)), None, "{} : {} → {probe}", r.id, t.raw);
                if prefix.len() == p.comps.len() && r.project.is_none() {
                    let lit = format!("{}\\{}", prefix[0], prefix[1..].join("\\"));
                    assert_eq!(deny.check(&wp(&lit)), None, "{} : {}", r.id, t.raw);
                }
            }
        }
    }
}

#[test]
fn aucune_regle_ne_couvre_la_denylist() {
    let set = builtin().unwrap();
    let scan = machine().build("C:\\");
    let profile = "C:\\Users\\Arthur";
    let (a, deny) = run(&set, &scan, profile, &format!("{profile}\\AppData\\Local\\Temp"));
    assert_never_denied(&a, &scan, &set, &deny);
    assert!(a.disabled.is_empty(), "{:?}", a.disabled);

    // Fichiers temporaires : les vieux seulement, jamais la ruche, le fichier SYSTEM ni un dépôt
    // git (dossier contenant .git, même sous forme de fichier : ni entier, ni en partie).
    let temp = covered_paths(&a, &scan, &set, "fichiers-temporaires");
    let t = "C:\\Users\\Arthur\\AppData\\Local\\Temp";
    assert_eq!(temp, [format!("{t}\\installeur-ancien"), format!("{t}\\vieux.log")]);

    // Commande officielle : Windows.old est mesuré, jamais couvert pour suppression.
    let old = covered_paths(&a, &scan, &set, "windows-old");
    assert_eq!(old, ["C:\\Windows.old"]);
    assert_eq!(deny.check(&wp("C:\\Windows.old")), Some(DenyReason::WindowsOld));
}

#[test]
fn temp_dans_windows_neutralise() {
    // Compte de service : %TEMP% = C:\Windows\Temp. La règle est neutralisée, rien n'est couvert.
    let set = builtin().unwrap();
    let scan = machine().build("C:\\");
    let (a, deny) = run(&set, &scan, "C:\\Users\\Arthur", "C:\\Windows\\Temp");
    assert!(a.disabled.iter().any(|d| d.rule == "fichiers-temporaires" && d.reason == DenyReason::Windows), "{:?}", a.disabled);
    assert!(covered_paths(&a, &scan, &set, "fichiers-temporaires").is_empty());
    assert_never_denied(&a, &scan, &set, &deny);
}

#[test]
fn profil_systeme_rien_couvert() {
    // Lumen lancé sous SYSTEM : le profil est dans C:\Windows ; aucune suppression possible.
    let set = builtin().unwrap();
    let mut t = machine();
    let sys = "Windows\\System32\\config\\systemprofile";
    t.file(&format!("{sys}\\AppData\\Local\\npm-cache\\x"), 5 << 20, 2).file(&format!("{sys}\\AppData\\Local\\Temp\\x.tmp"), 5 << 20, 30);
    let scan = t.build("C:\\");
    let profile = format!("C:\\{sys}");
    let (a, deny) = run(&set, &scan, &profile, &format!("{profile}\\AppData\\Local\\Temp"));
    assert_never_denied(&a, &scan, &set, &deny);
    assert!(a.recos.iter().all(|r| set.rules[r.rule].action == Action::Command), "{:?}", a.recos.iter().map(|r| &set.rules[r.rule].id).collect::<Vec<_>>());
}

#[test]
fn projets_actifs_inactifs_et_garde_fous() {
    let set = builtin().unwrap();
    let scan = machine().build("C:\\");
    let (a, _) = run(&set, &scan, "C:\\Users\\Arthur", "C:\\Users\\Arthur\\AppData\\Local\\Temp");
    let p = "C:\\Users\\Arthur\\Projets";
    // Actif : node_modules (verrou présent) et .next ; jamais le build imbriqué dans node_modules ;
    // dist exige .gitignore (non lu ici) ; node_modules sans verrou non compté ; AppData et
    // extensions d'éditeur jamais pris pour des projets.
    assert_eq!(covered_paths(&a, &scan, &set, "artefacts-projets-actifs"), [format!("{p}\\actif\\.next"), format!("{p}\\actif\\node_modules")]);
    assert_eq!(covered_paths(&a, &scan, &set, "artefacts-projets-inactifs"), [format!("{p}\\vieux\\target")]);
    let reco = a.recos.iter().find(|r| set.rules[r.rule].id == "artefacts-projets-inactifs").unwrap();
    assert_eq!(reco.projects.len(), 1);
    assert_eq!(reco.projects[0].source, crate::engine::ActivitySource::Files);
    assert_eq!((NOW - reco.projects[0].last_activity) / DAY, 300);
}

#[test]
fn statuts_categories_et_gains() {
    let set = builtin().unwrap();
    let scan = machine().build("C:\\");
    let (a, _) = run(&set, &scan, "C:\\Users\\Arthur", "C:\\Users\\Arthur\\AppData\\Local\\Temp");
    let id = |rel: &str| scan.find(rel).unwrap() as usize;
    let cat = |rel: &str| Category::ALL[a.category[id(rel)] as usize];
    let npm = id("Users\\Arthur\\AppData\\Local\\npm-cache");
    assert_eq!(a.status[npm], STATUS_SAFE);
    assert_eq!(a.status[id("Users\\Arthur\\AppData\\Local\\npm-cache\\_cacache")], STATUS_SAFE, "hérité");
    assert_eq!(a.status[id("Users\\Arthur\\.ollama\\models")], STATUS_REVIEW);
    assert_eq!(a.status[id("Users\\Arthur\\Documents")], STATUS_KEEP);
    assert_eq!(cat("Users\\Arthur\\Documents\\rapport.docx"), Category::Media);
    assert_eq!(cat("Users\\Arthur\\Projets\\actif\\src"), Category::Code);
    assert_eq!(cat("Users\\Arthur\\Projets\\actif\\node_modules"), Category::Code);
    assert_eq!(cat("Users\\Arthur\\.ollama"), Category::Ai);
    assert_eq!(cat("Users\\Arthur\\AppData\\Local\\npm-cache"), Category::Packages);
    assert_eq!(cat("Windows\\Temp"), Category::System);
    assert_eq!(cat("Users\\Arthur\\AppData\\Local\\Microsoft"), Category::Tools, "par défaut");
    // Gains : remontent jusqu'à la racine, séparés par statut, égaux aux recommandations.
    let safe: u64 = a.recos.iter().filter(|r| set.rules[r.rule].status == Status::Safe).map(|r| r.allocated).sum();
    let review: u64 = a.recos.iter().filter(|r| set.rules[r.rule].status == Status::Review).map(|r| r.allocated).sum();
    assert_eq!(a.gain_safe[0], safe);
    assert_eq!(a.gain_review[0], review);
    assert_eq!(a.gain_safe[id("Users\\Arthur\\Projets")], (300 << 20) + (1 << 20) + (1 << 10) + (90 << 20));
    // Un élément n'est compté qu'une fois : la somme des recommandations = gain de la racine.
    let all: Vec<NodeId> = a.recos.iter().flat_map(|r| r.covered.iter().copied()).collect();
    for (i, x) in all.iter().enumerate() {
        for y in &all[i + 1..] {
            assert!(!scan.path(*y).starts_with(&format!("{}\\", scan.path(*x))) && !scan.path(*x).starts_with(&format!("{}\\", scan.path(*y))));
        }
    }
}

#[test]
fn analyse_du_seul_profil() {
    // Racine de l'analyse = le profil : les motifs se raccordent au chemin de la racine.
    let set = builtin().unwrap();
    let mut t = Tree::default();
    t.file("AppData\\Local\\npm-cache\\x", 10 << 20, 1).file("NTUSER.DAT", 1 << 20, 1).dir("Vide", 1);
    let scan = t.build("C:\\Users\\Arthur");
    let (a, deny) = run(&set, &scan, "C:\\Users\\Arthur", "C:\\Users\\Arthur\\AppData\\Local\\Temp");
    assert_eq!(covered_paths(&a, &scan, &set, "cache-npm"), ["C:\\Users\\Arthur\\AppData\\Local\\npm-cache"]);
    assert_never_denied(&a, &scan, &set, &deny);
}

/// Secrets et configurations d'outils (liste minimale relue) : jamais couverts par une règle de
/// suppression, ni à leur place, ni glissés dans un dossier de cache que vise une règle.
#[test]
fn aucune_regle_ne_couvre_un_secret() {
    let set = builtin().unwrap();
    let p = "Users\\Arthur";
    let real = [
        ".gradle\\gradle.properties",
        ".gradle\\init.d\\entreprise.gradle",
        ".m2\\settings.xml",
        ".m2\\settings-security.xml",
        ".npmrc",
        ".yarnrc",
        ".yarnrc.yml",
        ".cargo\\credentials.toml",
        ".cargo\\credentials",
        ".cargo\\config.toml",
        ".docker\\config.json",
        ".kube\\config",
        ".ssh\\id_ed25519",
        ".aws\\credentials",
        ".azure\\msal_token_cache.json",
        ".gnupg\\pubring.kbx",
        "AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cookies",
        "AppData\\Local\\Google\\Chrome\\User Data\\Default\\Login Data",
        "AppData\\Local\\Google\\Chrome\\User Data\\Default\\Web Data",
        "AppData\\Local\\Google\\Chrome\\User Data\\Default\\Local Storage\\leveldb\\000003.log",
        "AppData\\Local\\Google\\Chrome\\User Data\\Default\\IndexedDB\\x.leveldb\\1.log",
        "AppData\\Local\\Google\\Chrome\\User Data\\Default\\Sessions\\Session_1",
    ];
    let mut t = machine();
    for rel in real {
        t.file(&format!("{p}\\{rel}"), 4 << 10, 400);
    }
    // Les mêmes noms glissés dans chaque dossier que visent les règles de suppression (vieux
    // fichiers : la règle des 48 h les retiendrait si rien ne les protégeait).
    let traps = [".npmrc", ".yarnrc.yml", "Cookies", "Login Data", "Web Data", "Local Storage\\x.log", "IndexedDB\\x", "Sessions\\s", "id_rsa", "release.jks"];
    let local = format!("{p}\\AppData\\Local");
    let cache_dirs = [
        format!("{local}\\Temp\\appli-electron"),
        format!("{local}\\D3DSCache\\abc"),
        format!("{p}\\.gradle\\caches\\modules-2"),
        format!("{p}\\.cargo\\registry\\cache"),
        format!("{p}\\Projets\\actif\\node_modules\\pkg"),
        format!("{p}\\Projets\\vieux\\target\\debug"),
    ];
    for dir in &cache_dirs {
        for trap in traps {
            t.file(&format!("{dir}\\{trap}"), 4 << 10, 400);
        }
    }
    let scan = t.build("C:\\");
    let profile = "C:\\Users\\Arthur";
    let (a, deny) = run(&set, &scan, profile, &format!("{profile}\\AppData\\Local\\Temp"));
    assert_never_denied(&a, &scan, &set, &deny);

    // Contrôle indépendant de la denylist : aucun élément couvert (ni rien dessous) n'a un nom ou
    // un chemin de la liste.
    let secret_names = [".NPMRC", ".YARNRC", ".YARNRC.YML", "COOKIES", "LOGIN DATA", "WEB DATA", "LOCAL STORAGE", "INDEXEDDB", "SESSIONS", "ID_RSA", "RELEASE.JKS"];
    let real_paths: Vec<String> = real.iter().map(|r| format!("C:\\{p}\\{r}").to_uppercase()).collect();
    for reco in a.recos.iter().filter(|r| set.rules[r.rule].action == Action::Delete) {
        let mut stack = reco.covered.clone();
        while let Some(n) = stack.pop() {
            let path = scan.path(n).to_uppercase();
            let name = scan.name(n).to_uppercase();
            assert!(!secret_names.contains(&name.as_str()), "{} couvre {path}", set.rules[reco.rule].id);
            assert!(!real_paths.iter().any(|s| s == &path || s.starts_with(&format!("{path}\\"))), "{} couvre {path}", set.rules[reco.rule].id);
            stack.extend(scan.children(n));
        }
    }
    // Les caches restent nettoyés autour des pièges : seuls les secrets restent en place.
    let gradle = covered_paths(&a, &scan, &set, "caches-gradle");
    assert!(gradle.iter().any(|c| c.ends_with("\\.gradle\\caches\\modules-2\\a.jar")), "{gradle:?}");
    assert!(!gradle.iter().any(|c| c.ends_with("\\.gradle\\caches") || c.ends_with("\\modules-2")), "dossier contenant un secret jamais pris entier : {gradle:?}");
    let temp = covered_paths(&a, &scan, &set, "fichiers-temporaires");
    assert!(temp.iter().any(|c| c.ends_with("\\Temp\\vieux.log")) && !temp.iter().any(|c| c.ends_with("appli-electron")), "{temp:?}");
}

/// Les règles visent des sous-dossiers de cache explicites, jamais la racine d'un dossier d'outil.
#[test]
fn cibles_jamais_a_la_racine_d_un_dossier_d_outil() {
    let set = builtin().unwrap();
    for r in &set.rules {
        for t in &r.targets {
            match t.rest.first() {
                // `%USERPROFILE%\.outil` doit être suivi d'au moins un sous-dossier.
                Some(first) => assert!(!(t.var == Var::UserProfile && first.starts_with('.') && t.rest.len() < 2), "{} : {}", r.id, t.raw),
                // Une variable seule : uniquement %TEMP%, contenu inchangé depuis 48 h au moins.
                None => assert!(t.var == Var::Temp && matches!(r.scope, Scope::OlderThan { hours } if hours >= 48), "{} : {}", r.id, t.raw),
            }
        }
    }
}

#[test]
fn rien_dans_les_telechargements() {
    let set = builtin().unwrap();
    let mut t = machine();
    let d = "Users\\Arthur\\Downloads";
    t.file(&format!("{d}\\installeur.exe"), 90 << 20, 400)
        .file(&format!("{d}\\archive\\package.json"), 1 << 10, 400)
        .file(&format!("{d}\\archive\\package-lock.json"), 1 << 10, 400)
        .file(&format!("{d}\\archive\\node_modules\\x\\i.js"), 50 << 20, 400)
        .file(&format!("{d}\\recent\\package.json"), 1 << 10, 1)
        .file(&format!("{d}\\recent\\package-lock.json"), 1 << 10, 1)
        .file(&format!("{d}\\recent\\node_modules\\x\\i.js"), 50 << 20, 1)
        .file(&format!("{d}\\cache.tmp"), 1 << 20, 400);
    let scan = t.build("C:\\");
    let (a, _) = run(&set, &scan, "C:\\Users\\Arthur", "C:\\Users\\Arthur\\AppData\\Local\\Temp");
    let downloads = scan.find(d).unwrap() as usize;
    assert_eq!((a.gain_safe[downloads], a.gain_review[downloads]), (0, 0));
    for reco in &a.recos {
        for &c in &reco.covered {
            assert!(!scan.path(c).contains("\\Downloads"), "{} couvre {}", set.rules[reco.rule].id, scan.path(c));
        }
    }
}
