//! Fixtures de projets : avec et sans fichier de verrouillage, avec et sans git, actifs et inactifs,
//! manifestes tronqués.

use std::collections::HashMap;

use rules::schema::Var;
use safety::denylist::{DenyList, SystemLayout};
use safety::path::WinPath;
use scan::model::Builder;
use scan::{Kind, Method, RawEntry, Stats};

use super::*;
use crate::analysis::{now, Applied};

const DAY: i64 = 864_000_000_000; // en FILETIME (100 ns)

fn wp(s: &str) -> WinPath {
    WinPath::parse(s).unwrap()
}

/// Disque factice : manifestes et dernier commit par chemin.
#[derive(Default)]
struct FakeDisk {
    files: HashMap<String, String>,
    commits: HashMap<String, i64>,
}

impl ProjectDisk for FakeDisk {
    fn manifest(&self, path: &str) -> Option<String> {
        self.files.get(path).cloned()
    }

    fn last_commit(&self, git: &str) -> Option<i64> {
        self.commits.get(git).copied()
    }
}

/// Profil avec cinq projets :
/// - `app` : package.json + lockfile + .git, actif → node_modules et .next sans risque ;
/// - `vieux` : Cargo.toml, inactif depuis 400 jours → target à vérifier ;
/// - `nolock` : package.json sans lockfile, actif → node_modules jamais couvert ;
/// - `service` : go.mod, sans artefact ;
/// - `AppData\Local\outil` : jamais un projet (dossier d'appli).
fn profile() -> (Applied, Scan) {
    let now = now();
    let mut id = 0u128;
    let mut e = |name: &str, kind: Kind, size: u64, age_days: i64| {
        id += 1;
        RawEntry { name: name.into(), kind, attributes: 0, file_id: id, logical: size, allocated: size, modified: now - age_days * DAY, accessed: now - age_days * DAY }
    };
    let mut b = Builder::new(&e("Arthur", Kind::Dir, 0, 0));
    let top = b.add_children(0, &[e("Projets", Kind::Dir, 0, 0), e("AppData", Kind::Dir, 0, 0)]);
    let (projets, appdata) = (top.start, top.start + 1);
    let local = b.add_children(appdata, &[e("Local", Kind::Dir, 0, 0)]).start;
    let outil = b.add_children(local, &[e("outil", Kind::Dir, 0, 0)]).start;
    b.add_children(outil, &[e("package.json", Kind::File, 10, 1)]);
    let p = b.add_children(projets, &[e("app", Kind::Dir, 0, 0), e("vieux", Kind::Dir, 0, 0), e("nolock", Kind::Dir, 0, 0), e("service", Kind::Dir, 0, 0)]);
    let (app, vieux, nolock, service) = (p.start, p.start + 1, p.start + 2, p.start + 3);
    let a = b.add_children(
        app,
        &[
            e("package.json", Kind::File, 1_000, 2),
            e("package-lock.json", Kind::File, 50_000, 2),
            e("tsconfig.json", Kind::File, 500, 2),
            e(".git", Kind::Dir, 0, 2),
            e("node_modules", Kind::Dir, 0, 2),
            e(".next", Kind::Dir, 0, 2),
            e("src", Kind::Dir, 0, 2),
        ],
    );
    b.add_children(a.start + 3, &[e("HEAD", Kind::File, 30_000, 2)]);
    b.add_children(a.start + 4, &[e("react.js", Kind::File, 300_000_000, 2)]);
    b.add_children(a.start + 5, &[e("cache.bin", Kind::File, 80_000_000, 2)]);
    b.add_children(a.start + 6, &[e("index.tsx", Kind::File, 20_000, 2)]);
    let v = b.add_children(vieux, &[e("Cargo.toml", Kind::File, 300, 400), e("target", Kind::Dir, 0, 400), e("src", Kind::Dir, 0, 400)]);
    b.add_children(v.start + 1, &[e("app.exe", Kind::File, 2_000_000_000, 400)]);
    b.add_children(v.start + 2, &[e("main.rs", Kind::File, 4_000, 400)]);
    let n = b.add_children(nolock, &[e("package.json", Kind::File, 800, 3), e("node_modules", Kind::Dir, 0, 3)]);
    b.add_children(n.start + 1, &[e("lodash.js", Kind::File, 9_000_000, 3)]);
    b.add_children(service, &[e("go.mod", Kind::File, 100, 30), e("main.go", Kind::File, 5_000, 30)]);
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
    env.set(Var::UserProfile, wp("C:\\Users\\Arthur"))
        .set(Var::LocalAppData, wp("C:\\Users\\Arthur\\AppData\\Local"))
        .set(Var::AppData, wp("C:\\Users\\Arthur\\AppData\\Roaming"))
        .set(Var::Downloads, wp("C:\\Users\\Arthur\\Downloads"));
    let deny = DenyList::new(&layout);
    let ctx = rules::Context { env: &env, deny: &deny, now, read_disk: false };
    let set = crate::analysis::builtin().as_ref().unwrap();
    let analysis = rules::analyze(set, &scan, &ctx);
    (Applied { analysis, env, profile: Some("C:\\Users\\Arthur".into()) }, scan)
}

fn by_path<'a>(list: &'a [ProjectDto], name: &str) -> &'a ProjectDto {
    list.iter().find(|p| p.path.ends_with(name)).unwrap_or_else(|| panic!("{name} : {:?}", list.iter().map(|p| &p.path).collect::<Vec<_>>()))
}

#[test]
fn projets_langages_tailles_et_artefacts() {
    let (a, scan) = profile();
    let mut disk = FakeDisk::default();
    disk.files.insert("C:\\Users\\Arthur\\Projets\\app\\package.json".into(), "{\"name\": \"hinageshi-app\", \"version\": \"1.0.0\"}".into());
    // Manifeste tronqué : le nom du dossier reste.
    disk.files.insert("C:\\Users\\Arthur\\Projets\\vieux\\Cargo.toml".into(), "[package]\nname = \"kai".into());
    let list = projects(&a, &scan, &disk);
    assert_eq!(list.len(), 4, "AppData n'est jamais cherché");

    let app = by_path(&list, "\\app");
    assert_eq!((app.name.as_str(), app.language, app.git, app.lockfile), ("hinageshi-app", Some("TypeScript"), true, true));
    let arts: Vec<(&str, &str)> = app.artifacts.iter().map(|x| (x.path.as_str(), x.status)).collect();
    assert_eq!(arts, [("node_modules", "safe"), (".next", "safe")]);
    assert_eq!(app.artifacts_size, 380_000_000);
    assert_eq!(app.git_size, 30_000);
    assert_eq!(app.source_size, 1_000 + 50_000 + 500 + 20_000);

    let vieux = by_path(&list, "\\vieux");
    assert_eq!((vieux.name.as_str(), vieux.language, vieux.git), ("vieux", Some("Rust"), false));
    assert_eq!(vieux.artifacts.iter().map(|x| (x.path.as_str(), x.status)).collect::<Vec<_>>(), [("target", "review")]);
    assert_eq!(vieux.activity_source, "files");
    let age_days = (crate::cleanup::unix_ms() - vieux.last_activity.unwrap_or(0)) / 86_400_000;
    assert!((399..=401).contains(&age_days), "{age_days}");

    // Sans fichier de verrouillage : node_modules n'est pas un artefact régénérable à l'identique.
    let nolock = by_path(&list, "\\nolock");
    assert!(!nolock.lockfile && nolock.artifacts.is_empty());
    let service = by_path(&list, "\\service");
    assert_eq!((service.language, service.artifacts.len(), service.source_size), (Some("Go"), 0, 5_100));
}

#[test]
fn dernier_commit_lu_dans_git_sinon_fichiers() {
    let (a, scan) = profile();
    let mut disk = FakeDisk::default();
    disk.commits.insert("C:\\Users\\Arthur\\Projets\\app\\.git".into(), 1_790_000_000);
    let list = projects(&a, &scan, &disk);
    let app = by_path(&list, "\\app");
    // Le moteur de règles connaît déjà ce projet (artefacts) : sa date sert ; sinon, celle de git.
    assert!(app.last_activity.is_some());
    let service = by_path(&list, "\\service");
    assert_eq!(service.activity_source, "files");
}

#[test]
fn purge_seulement_des_artefacts_couverts() {
    let (a, scan) = profile();
    let list = projects(&a, &scan, &FakeDisk::default());
    let app = by_path(&list, "\\app");
    let ids: Vec<i64> = app.artifacts.iter().map(|x| x.id).collect();
    let sel = select_artifacts(&a, &scan, &ids).unwrap();
    let paths: Vec<&str> = sel.targets.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(paths, ["C:\\Users\\Arthur\\Projets\\app\\node_modules", "C:\\Users\\Arthur\\Projets\\app\\.next"]);
    assert!(sel.targets.iter().all(|t| t.authority == Authority::Rule("artefacts-projets-actifs".into())));
    // Le source, .git, le projet entier ou le node_modules sans lockfile : refusés.
    for p in ["Projets\\app\\src", "Projets\\app\\.git", "Projets\\app", "Projets\\nolock\\node_modules", "Projets\\app\\package.json"] {
        let id = scan.find(p).unwrap() as i64;
        assert!(matches!(select_artifacts(&a, &scan, &[id]), Err(LumenError::UnknownNode)), "{p}");
    }
    assert!(select_artifacts(&a, &scan, &[]).is_err());
    assert!(select_artifacts(&a, &scan, &[-1]).is_err());
}

#[test]
fn manifestes_tolerants() {
    assert_eq!(manifest_name("package.json", "{\"name\":\"@lumen/web\"}").as_deref(), Some("@lumen/web"));
    assert_eq!(manifest_name("package.json", "{\"name\":\"tronq"), None);
    assert_eq!(manifest_name("package.json", "[1, 2]"), None);
    assert_eq!(manifest_name("Cargo.toml", "[workspace]\nname = \"non\"\n[package]\nversion = \"1\"\nname = \"lumen\"\n").as_deref(), Some("lumen"));
    assert_eq!(manifest_name("pyproject.toml", "[project]\nname='outil'\n").as_deref(), Some("outil"));
    assert_eq!(manifest_name("pyproject.toml", "[tool.poetry]\nname = \"poesie\"").as_deref(), Some("poesie"));
    assert_eq!(manifest_name("Cargo.toml", "[package]\nname = \"inachevé"), None);
    assert_eq!(manifest_name("Cargo.toml", "\u{fffd}\u{fffd}"), None);
    assert_eq!(language(&["go.mod".into()]), Some("Go"));
    assert_eq!(language(&["App.csproj".into()]), Some("C#"));
    assert_eq!(language(&["build.gradle.kts".into()]), Some("Kotlin"));
    assert_eq!(language(&[".git".into()]), None);
}
