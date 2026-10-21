//! Fixtures de launchers : Steam avec deux bibliothèques sur deux « disques », manifestes Epic,
//! registre GOG / Ubisoft / Uninstall, et des fichiers corrompus ou tronqués à chaque étape.

use std::fs;
use std::path::{Path, PathBuf};

use super::*;

const NOW: i64 = 1_792_584_000;
const DAY: i64 = 86_400;

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn acf(appid: &str, name: &str, dir: &str, size: &str, last: Option<i64>) -> String {
    let last = last.map(|t| format!("\t\"LastPlayed\"\t\t\"{t}\"\n")).unwrap_or_default();
    format!("\"AppState\"\n{{\n\t\"appid\"\t\t\"{appid}\"\n\t\"name\"\t\t\"{name}\"\n\t\"installdir\"\t\t\"{dir}\"\n\t\"SizeOnDisk\"\t\t\"{size}\"\n{last}\t\"StateFlags\"\t\t\"4\"\n}}\n")
}

/// Chemin tel qu'un launcher l'écrit dans un VDF (barres obliques échappées).
fn vdf_path(p: &Path) -> String {
    p.display().to_string().replace('\\', "\\\\")
}

struct Machine {
    _tmp: tempfile::TempDir,
    steam: PathBuf,
    library_d: PathBuf,
    program_data: PathBuf,
    reg: MemRegistry,
}

fn machine() -> Machine {
    let tmp = tempfile::tempdir().unwrap();
    let steam = tmp.path().join("C").join("Program Files (x86)").join("Steam");
    let library_d = tmp.path().join("D").join("SteamLibrary");
    let program_data = tmp.path().join("C").join("ProgramData");
    // Bibliothèques sur deux disques (format actuel de libraryfolders.vdf).
    write(
        &steam.join("steamapps").join("libraryfolders.vdf"),
        &format!(
            "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t\t\"apps\" {{ \"570\" \"123\" }}\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n}}\n",
            vdf_path(&steam),
            vdf_path(&library_d)
        ),
    );
    let apps_c = steam.join("steamapps");
    let apps_d = library_d.join("steamapps");
    write(&apps_c.join("appmanifest_570.acf"), &acf("570", "Dota 2", "dota 2 beta", "42949672960", Some(1_400_000_000)));
    write(&apps_c.join("appmanifest_228980.acf"), &acf("228980", "Steamworks Common Redistributables", "Steamworks Shared", "1000", None));
    // Manifeste tronqué (écriture interrompue) et manifeste sans dossier d'installation.
    write(&apps_c.join("appmanifest_999.acf"), "\"AppState\"\n{\n\t\"appid\"\t\t\"999\"\n\t\"name\"\t\t\"Tronq");
    write(&apps_d.join("appmanifest_42.acf"), "\"AppState\" { \"appid\" \"42\" \"name\" \"Sans dossier\" }");
    write(&apps_d.join("appmanifest_1091500.acf"), &acf("1091500", "Cyberpunk 2077", "Cyberpunk 2077", "75161927680", None));
    write(&apps_d.join("appmanifest_vide.acf"), "");
    fs::create_dir_all(apps_d.join("shadercache").join("1091500")).unwrap();
    // Dernière partie : deux comptes, dont un fichier corrompu.
    write(
        &steam.join("userdata").join("123").join("config").join("localconfig.vdf"),
        &format!(
            "\"UserLocalConfigStore\" {{ \"Software\" {{ \"Valve\" {{ \"Steam\" {{ \"apps\" {{ \"1091500\" {{ \"LastPlayed\" \"{}\" }} \"570\" {{ \"LastPlayed\" \"1500000000\" }} }} }} }} }} }}",
            NOW - 3 * DAY
        ),
    );
    write(&steam.join("userdata").join("456").join("config").join("localconfig.vdf"), "\"UserLocalConfigStore\" { \"Software\" {");
    write(&steam.join("appcache").join("librarycache").join("1091500").join("library_600x900.jpg"), "jpg");
    write(&steam.join("appcache").join("librarycache").join("570_header.jpg"), "jpg");

    // Epic : un jeu, un DLC, une installation incomplète, un manifeste tronqué, un sans nom.
    let manifests = epic::manifests_dir(&program_data);
    let item = |app: &str, main: &str, name: &str, dir: &Path, extra: &str| {
        format!("{{\"AppName\":\"{app}\",\"MainGameAppName\":\"{main}\",\"DisplayName\":\"{name}\",\"InstallLocation\":\"{}\",\"InstallSize\":32212254720{extra}}}", vdf_path(dir))
    };
    let games_e = tmp.path().join("E").join("Epic Games");
    write(&manifests.join("A1.item"), &item("Fortnite", "Fortnite", "Fortnite", &games_e.join("Fortnite"), ""));
    write(&manifests.join("A2.item"), &item("FortniteDLC", "Fortnite", "Fortnite — pack", &games_e.join("Fortnite"), ""));
    write(&manifests.join("A3.item"), &item("Hades", "Hades", "Hades", &games_e.join("Hades"), ",\"bIsIncompleteInstall\":true"));
    write(&manifests.join("A4.item"), "{\"AppName\":\"Alan\",\"DisplayName\":\"Alan Wake 2\",\"InstallLoc");
    write(&manifests.join("A5.item"), &format!("{{\"AppName\":\"X\",\"InstallLocation\":\"{}\"}}", vdf_path(&games_e.join("X"))));
    write(&manifests.join("notes.txt"), "pas un manifeste");

    let mut reg = MemRegistry::new();
    // Steam écrit son chemin avec des barres obliques.
    reg.set_str(Hive::CurrentUser, "Software\\Valve\\Steam", "SteamPath", &steam.display().to_string().replace('\\', "/"));
    let gog_dir = tmp.path().join("G").join("GOG Games").join("The Witcher 3");
    reg.set_str(Hive::LocalMachine, "SOFTWARE\\GOG.com\\Games\\1207664663", "gameName", "The Witcher 3: Wild Hunt")
        .set_str(Hive::LocalMachine, "SOFTWARE\\GOG.com\\Games\\1207664663", "path", &gog_dir.display().to_string())
        .set_str(Hive::LocalMachine, "SOFTWARE\\GOG.com\\Games\\1", "gameName", "Clé GOG sans dossier");
    let u = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall";
    let entry = |reg: &mut MemRegistry, key: &str, name: &str, publisher: &str, dir: Option<&Path>, kb: Option<u32>| {
        let path = format!("{u}\\{key}");
        reg.set_str(Hive::LocalMachine, &path, "DisplayName", name).set_str(Hive::LocalMachine, &path, "Publisher", publisher);
        if let Some(d) = dir {
            reg.set_str(Hive::LocalMachine, &path, "InstallLocation", &d.display().to_string());
        }
        if let Some(kb) = kb {
            reg.set_dword(Hive::LocalMachine, &path, "EstimatedSize", kb);
        }
    };
    // Doublon du jeu GOG (même dossier), jeu Steam (reste à Steam), launchers eux-mêmes.
    entry(&mut reg, "1207664663_is1", "The Witcher 3: Wild Hunt", "GOG.com", Some(&gog_dir), Some(1));
    entry(&mut reg, "Steam App 570", "Dota 2", "Valve", Some(&apps_c.join("common").join("dota 2 beta")), None);
    entry(&mut reg, "Battle.net", "Battle.net", "Blizzard Entertainment", None, None);
    entry(&mut reg, "Overwatch", "Overwatch", "Blizzard Entertainment, Inc.", Some(&tmp.path().join("C").join("Overwatch")), Some(50 * 1024 * 1024));
    entry(&mut reg, "{EA-APP}", "EA app", "Electronic Arts", None, None);
    entry(&mut reg, "{APEX}", "Apex Legends", "Electronic Arts", Some(&tmp.path().join("C").join("EA Games").join("Apex")), None);
    entry(&mut reg, "Uplay Install 4", "Assassin's Creed Valhalla", "Ubisoft", None, Some(1024));
    entry(&mut reg, "{NOTEPAD}", "Notepad++", "Notepad++ Team", None, None);
    reg.set_str(Hive::LocalMachine, "SOFTWARE\\Ubisoft\\Launcher\\Installs\\4", "InstallDir", &tmp.path().join("C").join("Ubisoft").join("ACV").display().to_string())
        .set_str(Hive::LocalMachine, "SOFTWARE\\Ubisoft\\Launcher\\Installs\\5", "Autre", "sans dossier");
    Machine { steam, library_d, program_data, reg, _tmp: tmp }
}

fn find<'a>(d: &'a Detection, name: &str) -> &'a Game {
    d.games.iter().find(|g| g.name == name).unwrap_or_else(|| panic!("{name} absent : {:?}", d.games.iter().map(|g| &g.name).collect::<Vec<_>>()))
}

#[test]
fn detection_sur_toute_la_machine() {
    let m = machine();
    let d = detect(&Env { program_data: m.program_data.clone() }, &m.reg);
    let names: Vec<&str> = d.games.iter().map(|g| g.name.as_str()).collect();
    assert_eq!(names, ["Apex Legends", "Assassin's Creed Valhalla", "Cyberpunk 2077", "Dota 2", "Fortnite", "Overwatch", "The Witcher 3: Wild Hunt"]);
    assert_eq!(d.found, [Launcher::Steam, Launcher::Epic, Launcher::Gog, Launcher::Ubisoft, Launcher::BattleNet, Launcher::Ea]);

    // Steam : deux bibliothèques sur deux disques, dossier d'installation dans la bonne.
    assert_eq!(d.steam_libraries.len(), 2);
    let cp = find(&d, "Cyberpunk 2077");
    assert_eq!((cp.launcher, cp.key.as_str(), cp.size), (Launcher::Steam, "1091500", Some(75_161_927_680)));
    assert_eq!(cp.install_dir.as_deref(), Some(m.library_d.join("steamapps").join("common").join("Cyberpunk 2077").as_path()));
    assert_eq!(cp.last_played, Some(NOW - 3 * DAY));
    assert!(cp.cover.as_ref().is_some_and(|c| c.ends_with("library_600x900.jpg")));
    assert!(cp.shader_cache.is_some());
    // Dernière partie : la plus récente du manifeste et de localconfig.
    let dota = find(&d, "Dota 2");
    assert_eq!((dota.last_played, dota.launcher), (Some(1_500_000_000), Launcher::Steam));
    assert!(dota.cover.as_ref().is_some_and(|c| c.ends_with("570_header.jpg")));
    assert!(dota.install_dir.as_ref().is_some_and(|p| p.starts_with(&m.steam)));

    // Epic : ni DLC ni installation incomplète ; date de dernière partie inconnue.
    let fortnite = find(&d, "Fortnite");
    assert_eq!((fortnite.launcher, fortnite.size, fortnite.last_played, fortnite.cover.clone()), (Launcher::Epic, Some(32_212_254_720), None, None));
    assert!(!names.contains(&"Hades") && !names.contains(&"Fortnite — pack"));

    // GOG trouvé une fois (clé GOG.com, doublon Uninstall écarté) ; Ubisoft nommé par Uninstall.
    assert_eq!(d.games.iter().filter(|g| g.name.starts_with("The Witcher")).count(), 1);
    assert_eq!(find(&d, "The Witcher 3: Wild Hunt").launcher, Launcher::Gog);
    assert_eq!(find(&d, "Assassin's Creed Valhalla").launcher, Launcher::Ubisoft);
    assert_eq!(find(&d, "Overwatch").size, Some(50 * 1024 * 1024 * 1024));
    assert_eq!(find(&d, "Apex Legends").launcher, Launcher::Ea);
}

#[test]
fn fichiers_corrompus_ignores_et_comptes() {
    let m = machine();
    let d = detect(&Env { program_data: m.program_data.clone() }, &m.reg);
    let reasons = |l: Launcher, r: Reason| d.ignored.iter().filter(|i| i.launcher == l && i.reason == r).count();
    // Steam : manifeste tronqué, manifeste vide (rien d'un jeu), sans dossier, localconfig tronqué.
    assert_eq!(reasons(Launcher::Steam, Reason::Malformed), 2, "{:?}", d.ignored);
    assert_eq!(reasons(Launcher::Steam, Reason::Incomplete), 2, "{:?}", d.ignored);
    // Epic : JSON tronqué, manifeste sans nom.
    assert_eq!(reasons(Launcher::Epic, Reason::Malformed), 1);
    assert_eq!(reasons(Launcher::Epic, Reason::Incomplete), 1);
    // GOG et Ubisoft : clés sans dossier.
    assert_eq!(reasons(Launcher::Gog, Reason::Incomplete), 1);
    assert_eq!(reasons(Launcher::Ubisoft, Reason::Incomplete), 1);
    assert!(d.ignored.iter().any(|i| i.file.ends_with("appmanifest_999.acf")));
}

#[test]
fn bibliotheques_steam_ancien_format_et_fichier_illisible() {
    let m = machine();
    let file = m.steam.join("steamapps").join("libraryfolders.vdf");
    write(&file, &format!("\"LibraryFolders\"\n{{\n\t\"TimeNextStatsReport\"\t\t\"1700000000\"\n\t\"1\"\t\t\"{}\"\n}}\n", vdf_path(&m.library_d)));
    let mut d = Detection::default();
    assert_eq!(steam::libraries(&m.steam, &mut d).len(), 2);
    // Tronqué : seule la bibliothèque de Steam reste, compté comme mal formé.
    write(&file, "\"libraryfolders\" { \"1\" { \"path\" ");
    let mut d = Detection::default();
    assert_eq!(steam::libraries(&m.steam, &mut d).len(), 1);
    assert_eq!(d.ignored[0].reason, Reason::Malformed);
    // Remplacé par un dossier (illisible) : idem, compté comme illisible.
    let other = tempfile::tempdir().unwrap();
    fs::create_dir_all(other.path().join("steamapps").join("libraryfolders.vdf")).unwrap();
    let mut d = Detection::default();
    assert_eq!(steam::libraries(other.path(), &mut d), [other.path().to_path_buf()]);
    assert_eq!(d.ignored[0].reason, Reason::Unreadable);
}

#[test]
fn rien_d_installe() {
    let tmp = tempfile::tempdir().unwrap();
    let d = detect(&Env { program_data: tmp.path().join("ProgramData") }, &MemRegistry::new());
    assert_eq!(d, Detection::default());
    // Steam déclaré dans le registre mais dossier absent : rien, sans erreur.
    let mut reg = MemRegistry::new();
    reg.set_str(Hive::CurrentUser, "Software\\Valve\\Steam", "SteamPath", "Z:/absent/Steam");
    assert!(detect(&Env { program_data: tmp.path().into() }, &reg).games.is_empty());
}

#[test]
fn trop_gros_ignore() {
    let tmp = tempfile::tempdir().unwrap();
    let f = tmp.path().join("gros.acf");
    fs::File::create(&f).unwrap().set_len(MAX_FILE + 1).unwrap();
    assert_eq!(read_text(&f), Err(Reason::TooLarge));
    assert_eq!(read_text(&tmp.path().join("absent.acf")), Err(Reason::Missing));
}

#[test]
fn statut_seulement_d_apres_une_date_connue() {
    assert!(!needs_review(None, NOW));
    assert!(!needs_review(Some(NOW - 30 * DAY), NOW));
    assert!(needs_review(Some(NOW - 200 * DAY), NOW));
}

/// Fichier verrouillé par le launcher (ouvert sans partage) : ignoré et compté.
#[cfg(windows)]
#[test]
fn manifeste_verrouille_ignore() {
    use std::os::windows::fs::OpenOptionsExt;
    let m = machine();
    let file = m.library_d.join("steamapps").join("appmanifest_1091500.acf");
    let _lock = fs::OpenOptions::new().read(true).share_mode(0).open(&file).unwrap();
    let d = detect(&Env { program_data: m.program_data.clone() }, &m.reg);
    assert!(!d.games.iter().any(|g| g.name == "Cyberpunk 2077"));
    assert!(d.ignored.iter().any(|i| i.file.ends_with("appmanifest_1091500.acf") && i.reason == Reason::Unreadable));
}
