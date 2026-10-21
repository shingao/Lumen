//! Bout en bout, sous Windows (CI) : un faux profil de fixtures est analysé (parcours réel),
//! les règles livrées y sont appliquées, les recommandations sans risque sont nettoyées pour de
//! vrai (Corbeille, commande officielle npm), puis on vérifie que seuls les bons éléments sont
//! partis, que les fichiers récents de %TEMP% et tout le reste sont intacts, que le journal
//! est complet et qu'un élément se restaure depuis la Corbeille.
//! Jalon 6 : doublons (lecture réelle du contenu, liens physiques, groupe jamais vidé, contenu
//! modifié juste avant la suppression) et fichiers dormants, sur un autre faux profil.
#![cfg(windows)]

use std::fs::{self, OpenOptions};
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};

use rules::schema::Var;
use safety::check::Checker;
use safety::denylist::DenyList;
use safety::exec::{CommandStatus, ItemOutcome};
use safety::path::WinPath;
use safety::plan::{execute, Confirmation, DeleteMode, Plan};
use safety::probe::FileId;
use safety::win::{in_recycle_bin, restore, system_layout, WinEffects, WinProbe};

use std::collections::HashMap;

use crate::advanced::{dormant, duplicates, ipc};
use crate::analysis;
use crate::cleanup::select;
use crate::journal::{ActionMeta, Journal};

fn write(p: &Path, content: &[u8]) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, content).unwrap();
}

fn age(p: &Path, hours: u64) {
    let f = OpenOptions::new().write(true).open(p).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(hours * 3600)).unwrap();
}

#[test]
fn nettoyage_reel_d_un_faux_profil() {
    // Dossier temporaire supprimé à la fin par `tempfile` (aucune suppression écrite ici).
    let root = tempfile::Builder::new().prefix("lumen-e2e-").tempdir().unwrap();
    let p = root.path().join("Arthur");
    let at = |rel: &str| p.join(rel);

    // Cache npm (commande officielle, sur ce cache-ci grâce à npm_config_cache).
    write(&at("AppData\\Local\\npm-cache\\_cacache\\index-v5\\aa\\bb\\entree"), b"{}");
    write(&at("AppData\\Local\\npm-cache\\_cacache\\content-v2\\sha512\\cc\\dd\\paquet"), &[7u8; 8192]);
    // Projet actif avec fichier de verrouillage : node_modules part.
    write(&at("Projets\\app\\package.json"), b"{\"name\":\"app\"}");
    write(&at("Projets\\app\\package-lock.json"), b"{}");
    write(&at("Projets\\app\\src\\index.js"), b"console.log(1)");
    write(&at("Projets\\app\\node_modules\\left-pad\\index.js"), &[1u8; 4096]);
    write(&at("Projets\\app\\node_modules\\left-pad\\package.json"), b"{}");
    // Projet sans fichier de verrouillage : node_modules reste.
    write(&at("Projets\\sans-verrou\\package.json"), b"{\"name\":\"x\"}");
    write(&at("Projets\\sans-verrou\\node_modules\\x\\index.js"), &[2u8; 4096]);
    // %TEMP% : anciens (plus de 48 h) et récents.
    write(&at("AppData\\Local\\Temp\\vieux.tmp"), &[3u8; 4096]);
    age(&at("AppData\\Local\\Temp\\vieux.tmp"), 72);
    write(&at("AppData\\Local\\Temp\\recent.tmp"), &[4u8; 4096]);
    write(&at("AppData\\Local\\Temp\\installeur\\ancien.log"), &[5u8; 4096]);
    age(&at("AppData\\Local\\Temp\\installeur\\ancien.log"), 96);
    write(&at("AppData\\Local\\Temp\\installeur\\en-cours.log"), &[6u8; 4096]);
    // Jamais touchés.
    write(&at("Documents\\important.txt"), b"ne pas toucher");
    write(&at(".ssh\\id_rsa"), b"secret");
    write(&at("Downloads\\setup.exe"), &[8u8; 4096]);

    // Analyse réelle du faux profil.
    let real = scan::win::root_path(p.to_str().unwrap());
    let scan = scan::walk(&scan::win::WinSource::new(&real), &real, &AtomicBool::new(false), &|_: &scan::Progress| {}).unwrap();
    let wp = |rel: &str| WinPath::parse(&format!("{real}\\{rel}")).unwrap();
    let mut env = rules::Env::new();
    env.set(Var::UserProfile, WinPath::parse(&real).unwrap())
        .set(Var::LocalAppData, wp("AppData\\Local"))
        .set(Var::AppData, wp("AppData\\Roaming"))
        .set(Var::Temp, wp("AppData\\Local\\Temp"))
        .set(Var::Documents, wp("Documents"))
        .set(Var::Downloads, wp("Downloads"))
        .set(Var::SystemDrive, WinPath::parse("C:\\").unwrap());
    let deny = DenyList::new(&system_layout().unwrap());
    let applied = analysis::apply_with(analysis::rules().unwrap(), &scan, env, &deny, Some(real.clone()), analysis::now());
    let recos = applied.recommendations(&scan);
    let safe: Vec<String> = recos.iter().filter(|r| r.status == "safe").map(|r| r.id.clone()).collect();
    for id in ["cache-npm", "artefacts-projets-actifs", "fichiers-temporaires"] {
        assert!(safe.iter().any(|s| s == id), "{id} absent : {safe:?}");
    }

    // Plan des recommandations sans risque, confirmé, puis exécuté pour de vrai.
    let sel = select(&applied, &scan, &safe).unwrap();
    let checker = Checker { probe: &WinProbe, deny: &deny, allow: &sel.allow, now: analysis::now() };
    let plan = Plan::build(&checker, &sel.targets, DeleteMode::Trash).with_commands(sel.commands.clone());
    assert!(plan.refused().is_empty(), "{:?}", plan.refused());
    std::env::set_var("npm_config_cache", at("AppData\\Local\\npm-cache"));
    let confirmation = Confirmation { fingerprint: plan.fingerprint(), permanent_acknowledged: false, without_restore_point: false };
    let report = execute(&plan, &confirmation, &checker, &WinEffects::new(), &AtomicBool::new(false), |_| {}).unwrap();
    eprintln!("{report:#?}");

    // Partis : node_modules du projet verrouillé, vieux fichiers de %TEMP%, contenu du cache npm.
    assert!(!at("Projets\\app\\node_modules").exists());
    assert!(!at("AppData\\Local\\Temp\\vieux.tmp").exists());
    if std::env::var_os("CI").is_some() {
        let npm = report.commands.iter().find(|c| c.command == safety::commands::OfficialCommand::NpmCacheClean).unwrap();
        assert_eq!(npm.status, CommandStatus::Succeeded, "{npm:#?}");
        assert!(npm.program.as_deref().unwrap().to_lowercase().ends_with("node.exe"));
        assert!(!at("AppData\\Local\\npm-cache\\_cacache\\content-v2\\sha512\\cc\\dd\\paquet").exists());
    }
    // Intacts : sources, projet sans verrou, fichiers récents de %TEMP%, documents, secrets, téléchargements.
    for keep in [
        "Projets\\app\\package.json",
        "Projets\\app\\package-lock.json",
        "Projets\\app\\src\\index.js",
        "Projets\\sans-verrou\\node_modules\\x\\index.js",
        "AppData\\Local\\Temp\\recent.tmp",
        "AppData\\Local\\Temp\\installeur\\en-cours.log",
        "Documents\\important.txt",
        ".ssh\\id_rsa",
        "Downloads\\setup.exe",
    ] {
        assert!(at(keep).exists(), "{keep} ne devait pas bouger");
    }
    // Tout ce qui est parti est dans la Corbeille, pas détruit.
    let units: Vec<_> = report.items.iter().flat_map(|i| &i.units).filter(|u| u.result.is_ok()).collect();
    assert!(!units.is_empty());
    for u in &units {
        let bin = u.result.as_ref().unwrap().bin_path.clone().expect("emplacement dans la Corbeille");
        assert!(in_recycle_bin(&bin, &u.path, u.file_id), "{} : {bin}", u.path);
    }
    assert!(report.items.iter().all(|i| matches!(i.outcome, ItemOutcome::Done | ItemOutcome::Partial)), "{report:#?}");
    assert!(report.measured_gain().is_some());

    // Journal complet, puis restauration de node_modules depuis la Corbeille.
    let mut journal = Journal::open(&root.path().join("lumen.db")).unwrap();
    let meta = ActionMeta { kind: "recos", without_restore_point: false, started_at: 1, finished_at: 2, title: "E2E", planned_bytes: plan.total_bytes(), script: "# script" };
    let action = journal.record(&meta, &report).unwrap();
    let items = journal.items(action, 1000).unwrap();
    let nm = items.iter().find(|i| i.path.ends_with("\\Projets\\app\\node_modules")).expect("node_modules au journal");
    assert_eq!((nm.result.as_str(), nm.method.as_str()), ("done", "trash"));
    let original = WinPath::parse(&nm.path).unwrap();
    let id = FileId { volume_serial: nm.volume_serial.unwrap(), id: nm.file_id.unwrap() };
    restore(nm.recycle_item.as_deref().unwrap(), &original, id).unwrap();
    assert!(at("Projets\\app\\node_modules\\left-pad\\index.js").exists());
    assert_eq!(journal.commands(action).unwrap().len(), plan.commands().len());
}

#[test]
fn doublons_et_dormants_d_un_faux_profil() {
    let root = tempfile::Builder::new().prefix("lumen-e2e-").tempdir().unwrap();
    let p = root.path().join("Arthur");
    let at = |rel: &str| p.join(rel);
    let film: Vec<u8> = (0..3 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();

    // Trois copies d'une vidéo, un lien physique vers l'une d'elles, des copies dans un projet git
    // et dans AppData (exclues), un vieux document (dormant), un vieux fichier dans le projet git.
    for rel in ["Videos\\film.mp4", "Desktop\\film - copie.mp4", "Downloads\\film (1).mp4", "Projets\\site\\film.mp4", "AppData\\Local\\app\\film.mp4"] {
        write(&at(rel), &film);
    }
    age(&at("Videos\\film.mp4"), 24 * 30);
    fs::hard_link(at("Videos\\film.mp4"), at("Downloads\\lien vers film.mp4")).unwrap();
    write(&at("Projets\\site\\.git\\HEAD"), b"ref: refs/heads/main");
    write(&at("Documents\\vieux-rapport.pdf"), &[9u8; 2 * 1024 * 1024]);
    age(&at("Documents\\vieux-rapport.pdf"), 24 * 400);
    write(&at("Projets\\site\\vieux.psd"), &[7u8; 2 * 1024 * 1024]);
    age(&at("Projets\\site\\vieux.psd"), 24 * 400);
    write(&at("Documents\\recent.docx"), &[5u8; 2 * 1024 * 1024]);

    let real = scan::win::root_path(p.to_str().unwrap());
    let scan = scan::walk(&scan::win::WinSource::new(&real), &real, &AtomicBool::new(false), &|_: &scan::Progress| {}).unwrap();
    let deny = DenyList::new(&system_layout().unwrap());
    let name_of = |id: scan::NodeId| scan.path(id)[real.len() + 1..].to_owned();

    // Dormants (date de modification : on ne dépend pas du réglage du dernier accès de la CI).
    let d = dormant::find(&dormant::Inputs {
        scan: &scan,
        deny: Some(&deny),
        now: analysis::now(),
        months: 6,
        last_access_updated: false,
        last_access_raw: None,
        covered: &|_| false,
        display: &|p| p.to_owned(),
    });
    assert_eq!(d.dto.basis, dormant::Basis::LastWrite);
    let names: Vec<String> = d.eligible.iter().map(|&id| name_of(id)).collect();
    assert_eq!(names, ["Documents\\vieux-rapport.pdf"], "ni projet git, ni AppData, ni fichier récent");

    // Doublons : lecture réelle, sans changer la date de dernier accès.
    let accessed = fs::metadata(at("Desktop\\film - copie.mp4")).unwrap().accessed().unwrap();
    let cancel = AtomicBool::new(false);
    let found = duplicates::find(
        &duplicates::Inputs { scan: &scan, deny: Some(&deny), min_size: 1 << 20, reader: &ipc::platform::Reader, cache: &HashMap::new(), cancel: &cancel },
        |_| {},
    )
    .unwrap();
    assert_eq!(fs::metadata(at("Desktop\\film - copie.mp4")).unwrap().accessed().unwrap(), accessed);
    assert_eq!(found.groups.len(), 1, "{:?}", found.groups);
    let group = &found.groups[0];
    let members: Vec<String> = group.members.iter().map(|&id| name_of(id)).collect();
    // Lien physique : un seul fichier, représenté par l'un de ses deux noms (au choix du parcours),
    // le plus ancien, donc gardé.
    let names_of_film = ["Videos\\film.mp4", "Downloads\\lien vers film.mp4"];
    assert!(names_of_film.contains(&members[0].as_str()), "le plus ancien est gardé : {members:?}");
    assert_eq!(members.iter().filter(|m| names_of_film.contains(&m.as_str())).count(), 1, "{members:?}");
    assert_eq!(members.len(), 3, "{members:?}");
    assert!(members.iter().all(|m| !m.starts_with("Projets") && !m.starts_with("AppData")), "{members:?}");
    assert_eq!(found.stats.hard_links, 1);
    // Le cache évite de relire au scan suivant.
    let cache: HashMap<_, _> = found.cache.iter().cloned().collect();
    let again = duplicates::find(
        &duplicates::Inputs { scan: &scan, deny: Some(&deny), min_size: 1 << 20, reader: &ipc::platform::Reader, cache: &cache, cancel: &cancel },
        |_| {},
    )
    .unwrap();
    assert_eq!((again.stats.read, again.groups.len()), (0, 1));

    // Tous les exemplaires : refusé.
    let all: Vec<i64> = group.members.iter().map(|&id| i64::from(id)).collect();
    assert!(ipc::select_duplicates(&scan, &found, &all).is_err());

    // Les deux copies : l'une est modifiée (même taille, même date) juste avant la suppression.
    let sel = ipc::select_duplicates(&scan, &found, &all[1..]).unwrap();
    let checker = Checker { probe: &WinProbe, deny: &deny, allow: &sel.allow, now: analysis::now() };
    let plan = Plan::build(&checker, &sel.targets, DeleteMode::Trash);
    assert!(plan.refused().is_empty(), "{:?}", plan.refused());
    let changed = at(&members[2]);
    let mtime = fs::metadata(&changed).unwrap().modified().unwrap();
    let mut other = film.clone();
    other[10] ^= 0xFF;
    fs::write(&changed, &other).unwrap();
    OpenOptions::new().write(true).open(&changed).unwrap().set_modified(mtime).unwrap();
    let confirmation = Confirmation { fingerprint: plan.fingerprint(), permanent_acknowledged: false, without_restore_point: false };
    let report = execute(&plan, &confirmation, &checker, &WinEffects::new(), &AtomicBool::new(false), |_| {}).unwrap();
    eprintln!("{report:#?}");
    assert!(!at(&members[1]).exists(), "copie supprimée");
    assert!(changed.exists(), "copie modifiée sautée");
    assert!(report.items.iter().any(|i| matches!(&i.outcome, ItemOutcome::Skipped(r) if r.contains("contenu modifié"))), "{report:#?}");
    for keep in ["Videos\\film.mp4", "Downloads\\lien vers film.mp4", "Projets\\site\\film.mp4", "AppData\\Local\\app\\film.mp4"] {
        assert!(at(keep).exists(), "{keep} ne devait pas bouger");
    }
    let done: Vec<_> = report.items.iter().flat_map(|i| &i.units).filter(|u| u.result.is_ok()).collect();
    assert_eq!(done.len(), 1);
    let bin = done[0].result.as_ref().unwrap().bin_path.clone().expect("emplacement dans la Corbeille");
    assert!(in_recycle_bin(&bin, &done[0].path, done[0].file_id));

    // Dormant coché : Corbeille, le reste intact.
    let sel = ipc::select_files(&scan, &d.eligible, ("Fichiers dormants", "Dormant files")).unwrap();
    let checker = Checker { probe: &WinProbe, deny: &deny, allow: &sel.allow, now: analysis::now() };
    let plan = Plan::build(&checker, &sel.targets, DeleteMode::Trash);
    let confirmation = Confirmation { fingerprint: plan.fingerprint(), permanent_acknowledged: false, without_restore_point: false };
    let report = execute(&plan, &confirmation, &checker, &WinEffects::new(), &AtomicBool::new(false), |_| {}).unwrap();
    assert!(report.items.iter().all(|i| i.outcome == ItemOutcome::Done), "{report:#?}");
    assert!(!at("Documents\\vieux-rapport.pdf").exists());
    assert!(at("Documents\\recent.docx").exists() && at("Projets\\site\\vieux.psd").exists());
}
