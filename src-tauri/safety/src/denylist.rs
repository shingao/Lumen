//! Denylist absolue, codée en dur et non modifiable (logique pure).
//!
//! Les dossiers système sont protégés deux fois : à l'emplacement réel détecté sur la machine
//! (`SystemLayout`) **et** à leur emplacement littéral sur `C:`. Un chemin est refusé s'il est
//! dans une zone protégée, ou s'il en est un parent (supprimer `C:\Users` emporterait les profils).

use crate::path::{fold, Prefix, WinPath};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DenyReason {
    #[error("racine d'un volume")]
    VolumeRoot,
    #[error("dossier de Windows")]
    Windows,
    #[error("dossier des programmes installés")]
    ProgramFiles,
    #[error("données des programmes (ProgramData)")]
    ProgramData,
    #[error("installation précédente de Windows : uniquement via l'outil Nettoyage de disque")]
    WindowsOld,
    #[error("fichiers de démarrage")]
    Boot,
    #[error("métadonnées NTFS")]
    NtfsMetadata,
    #[error("System Volume Information")]
    SystemVolumeInformation,
    #[error("Corbeille")]
    RecycleBin,
    #[error("fichier d'échange ou d'hibernation : uniquement via powercfg pour hiberfil.sys")]
    SwapFile,
    #[error("ruche du registre")]
    RegistryHive,
    #[error("profil Windows par défaut ou public")]
    SharedProfile,
    #[error("profil d'un autre utilisateur")]
    OtherProfile,
    #[error("dossier des profils ou profil entier")]
    ProfileRoot,
    #[error("fichier système (attribut SYSTEM)")]
    SystemAttribute,
    #[error("dossier de projet git : seuls ses artefacts régénérables peuvent être purgés")]
    GitProject,
    #[error("contenu interne d'un dépôt git (.git)")]
    GitInternals,
    #[error("identifiants, clés, cookies ou configuration d'un outil : jamais supprimés")]
    Secret,
    #[error("sauvegarde de jeu : jamais supprimée")]
    GameSave,
    #[error("jeux et applis du Microsoft Store (WindowsApps, XboxGames) : uniquement par l'appli Xbox ou le Store")]
    StoreApps,
}

/// Emplacements réels de la machine (dossiers connus de Windows), fournis par `safety::win`.
#[derive(Debug, Clone)]
pub struct SystemLayout {
    pub windows: WinPath,
    pub program_files: Vec<WinPath>,
    pub program_data: WinPath,
    /// Dossier des profils (`C:\Users`).
    pub users: WinPath,
    /// Profil de l'utilisateur courant (chemin long résolu).
    pub profile: WinPath,
    pub public: WinPath,
    /// Profils des autres comptes, lus dans le registre (ProfileList) et résolus en chemins
    /// longs : protégés où qu'ils soient, quel que soit leur nom.
    pub other_profiles: Vec<WinPath>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Zone {
    /// Le dossier et tout ce qu'il contient.
    Subtree,
    /// Le dossier lui-même seulement (ce qu'il contient peut être autorisé ailleurs).
    Exact,
}

#[derive(Debug, Clone)]
pub struct DenyList {
    zones: Vec<(WinPath, Zone, DenyReason)>,
    users: Vec<WinPath>,
    profile: WinPath,
}

/// Noms à la racine d'un volume.
const BOOT_NAMES: &[&str] = &["BOOT", "EFI", "BOOTMGR", "BOOTNXT"];
const SWAP_FILES: &[&str] = &["PAGEFILE.SYS", "SWAPFILE.SYS", "HIBERFIL.SYS"];
const HIVE_PREFIXES: &[&str] = &["NTUSER.DAT", "USRCLASS.DAT"];

/// Secrets et configurations, où qu'ils soient (y compris dans un dossier de cache) : jetons,
/// mots de passe, clés privées, cookies et stockage des navigateurs et des applis Electron.
const SECRET_NAMES: &[&str] = &[
    ".NPMRC", ".YARNRC", ".YARNRC.YML", ".GIT-CREDENTIALS", ".NETRC", "_NETRC", ".PGPASS",
    "COOKIES", "COOKIES-JOURNAL", "LOGIN DATA", "LOGIN DATA-JOURNAL", "LOGIN DATA FOR ACCOUNT", "WEB DATA",
    "WEB DATA-JOURNAL", "LOCAL STORAGE", "SESSION STORAGE", "INDEXEDDB", "SESSIONS",
    "COOKIES.SQLITE", "LOGINS.JSON", "KEY3.DB", "KEY4.DB", "CERT9.DB",
];
const SECRET_PREFIXES: &[&str] = &["ID_RSA", "ID_ED25519", "ID_ECDSA", "ID_DSA"];

/// Sauvegardes de jeux, où qu'elles soient (AppData, Documents\My Games, Saved Games, dossier
/// d'une instance Minecraft…) : dossiers par leur nom, fichiers par leur extension. Jamais
/// proposées, jamais supprimables, pas même par Doublons, Dormants ou Gros fichiers.
const SAVE_NAMES: &[&str] = &[
    "SAVES", "SAVE", "SAVEGAME", "SAVEGAMES", "SAVE GAMES", "SAVEDGAMES", "SAVED GAMES", "SAVEDATA", "PLAYERPROFILES",
];
const SAVE_SUFFIXES: &[&str] = &[".SAV", ".SAVE", ".LSV"];

/// Jeux et applis du Microsoft Store : leurs dossiers ne se gèrent que par l'appli Xbox ou le
/// Store (n'importe où dans le chemin), et le dossier des jeux Gaming Services à la racine d'un
/// volume (`XboxGames`, `.GamingRoot`).
const STORE_NAMES: &[&str] = &["WINDOWSAPPS", "WPSYSTEM"];
const STORE_ROOT_NAMES: &[&str] = &["XBOXGAMES", ".GAMINGROOT"];

fn is_save_name(name: &str) -> bool {
    SAVE_NAMES.contains(&name) || SAVE_SUFFIXES.iter().any(|s| name.ends_with(s))
}
const SECRET_SUFFIXES: &[&str] = &[".JKS", ".KEYSTORE", ".PFX", ".P12", ".KDBX", ".PPK"];

/// Configurations d'outils dans le profil (chemins relatifs) : `true` = dossier entier.
const PROFILE_SECRETS: &[(&[&str], bool)] = &[
    (&[".ssh"], true),
    (&[".aws"], true),
    (&[".azure"], true),
    (&[".kube"], true),
    (&[".gnupg"], true),
    (&[".gradle", "init.d"], true),
    (&[".gradle", "gradle.properties"], false),
    (&[".m2", "settings.xml"], false),
    (&[".m2", "settings-security.xml"], false),
    (&[".docker", "config.json"], false),
    (&[".android", "adbkey"], false),
    (&[".android", "adbkey.pub"], false),
    (&[".android", "debug.keystore"], false),
];
/// Fichiers d'un dossier d'outil du profil dont le nom commence par… (`~\.cargo\credentials*`).
const PROFILE_SECRET_PREFIXES: &[(&str, &str)] = &[(".cargo", "CREDENTIALS"), (".cargo", "CONFIG"), ("", ".YARNRC")];

fn is_secret_name(name: &str) -> bool {
    SECRET_NAMES.contains(&name) || SECRET_PREFIXES.iter().any(|p| name.starts_with(p)) || SECRET_SUFFIXES.iter().any(|s| name.ends_with(s))
}

fn literal(raw: &str) -> Option<WinPath> {
    WinPath::parse(raw).ok()
}

impl DenyList {
    pub fn new(layout: &SystemLayout) -> DenyList {
        use DenyReason::*;
        use Zone::*;
        let mut zones: Vec<(WinPath, Zone, DenyReason)> = Vec::new();
        let mut add = |p: Option<WinPath>, zone: Zone, reason: DenyReason| {
            if let Some(p) = p {
                zones.push((p, zone, reason));
            }
        };

        add(Some(layout.windows.clone()), Subtree, Windows);
        add(literal("C:\\Windows"), Subtree, Windows);
        for pf in &layout.program_files {
            add(Some(pf.clone()), Subtree, ProgramFiles);
        }
        add(literal("C:\\Program Files"), Subtree, ProgramFiles);
        add(literal("C:\\Program Files (x86)"), Subtree, ProgramFiles);
        add(Some(layout.program_data.clone()), Subtree, ProgramData);
        add(literal("C:\\ProgramData"), Subtree, ProgramData);
        // Windows.old ne part que par cleanmgr (commande officielle), jamais en suppression directe.
        add(layout.windows.parent().and_then(|root| root.join("Windows.old").ok()), Subtree, WindowsOld);
        add(literal("C:\\Windows.old"), Subtree, WindowsOld);

        let users = vec![layout.users.clone(), WinPath::parse("C:\\Users").unwrap_or_else(|_| layout.users.clone())];
        for u in &users {
            add(Some(u.clone()), Exact, ProfileRoot);
            for shared in ["Default", "Default User", "Public", "All Users"] {
                add(u.join(shared).ok(), Subtree, SharedProfile);
            }
        }
        add(Some(layout.public.clone()), Subtree, SharedProfile);
        for other in &layout.other_profiles {
            // Un profil listé dans le registre qui contiendrait le profil courant (configuration
            // anormale) ne doit pas bloquer celui-ci : on ne garde que les profils distincts.
            if !layout.profile.starts_with(other) {
                add(Some(other.clone()), Subtree, OtherProfile);
            }
        }
        add(Some(layout.profile.clone()), Exact, ProfileRoot);
        for (rel, whole) in PROFILE_SECRETS {
            let p = rel.iter().try_fold(layout.profile.clone(), |acc, c| acc.join(c).ok());
            add(p, if *whole { Subtree } else { Exact }, Secret);
        }
        for (dir, _) in PROFILE_SECRET_PREFIXES {
            // Le dossier lui-même (supprimé, il emporterait ses secrets) ; son contenu est vérifié par nom.
            if !dir.is_empty() {
                add(layout.profile.join(dir).ok(), Exact, Secret);
            }
        }

        DenyList { zones, users, profile: layout.profile.clone() }
    }

    /// Motif de refus d'un chemin canonique, ou `None` s'il n'est pas dans la denylist.
    pub fn check(&self, path: &WinPath) -> Option<DenyReason> {
        use DenyReason::*;
        if path.is_volume_root() {
            return Some(VolumeRoot);
        }
        let names: Vec<String> = path.components().iter().map(|c| fold(c)).collect();

        if let Some(first) = names.first() {
            if BOOT_NAMES.contains(&first.as_str()) {
                return Some(Boot);
            }
            if first.starts_with('$') {
                return Some(NtfsMetadata);
            }
            if STORE_ROOT_NAMES.contains(&first.as_str()) {
                return Some(StoreApps);
            }
        }
        for (i, n) in names.iter().enumerate() {
            match n.as_str() {
                "SYSTEM VOLUME INFORMATION" => return Some(SystemVolumeInformation),
                "$RECYCLE.BIN" | "RECYCLER" => return Some(RecycleBin),
                "$EXTEND" => return Some(NtfsMetadata),
                ".GIT" => return Some(GitInternals),
                n if is_secret_name(n) => return Some(Secret),
                n if is_save_name(n) => return Some(GameSave),
                n if STORE_NAMES.contains(&n) => return Some(StoreApps),
                "CONFIG" if i > 0 && names[i - 1] == "SYSTEM32" => return Some(RegistryHive),
                _ => {}
            }
        }
        if let Some(last) = names.last() {
            if SWAP_FILES.contains(&last.as_str()) {
                return Some(SwapFile);
            }
            if HIVE_PREFIXES.iter().any(|h| last.starts_with(h)) {
                return Some(RegistryHive);
            }
        }

        // `~\.cargo\credentials*`, `~\.yarnrc*`… : par préfixe, dans le dossier de l'outil.
        if let (Some(last), Some(parent)) = (names.last(), path.parent()) {
            for (dir, prefix) in PROFILE_SECRET_PREFIXES {
                let base = if dir.is_empty() { Some(self.profile.clone()) } else { self.profile.join(dir).ok() };
                if base.as_ref() == Some(&parent) && last.starts_with(prefix) {
                    return Some(Secret);
                }
            }
        }

        for (zone, kind, reason) in &self.zones {
            let inside = match kind {
                Zone::Subtree => path.starts_with(zone),
                Zone::Exact => path == zone,
            };
            // Parent d'une zone protégée : le supprimer emporterait la zone.
            if inside || zone.is_strictly_under(path) {
                return Some(*reason);
            }
        }

        // Sous le dossier des profils : seul le profil courant est accessible.
        for users in &self.users {
            if path.is_strictly_under(users) && !path.starts_with(&self.profile) {
                return Some(OtherProfile);
            }
        }

        // Partage réseau : la racine du partage compte comme une racine de volume.
        if matches!(path.prefix(), Prefix::Unc { .. }) && path.depth() == 0 {
            return Some(VolumeRoot);
        }
        None
    }

    /// Motifs qui dépendent de l'état réel du fichier (lus par la sonde).
    pub fn check_facts(&self, system_attribute: bool, is_dir: bool, contains_git: bool) -> Option<DenyReason> {
        if system_attribute {
            return Some(DenyReason::SystemAttribute);
        }
        if is_dir && contains_git {
            return Some(DenyReason::GitProject);
        }
        None
    }
}

#[cfg(test)]
pub(crate) fn test_layout() -> SystemLayout {
    let p = |s: &str| WinPath::parse(s).unwrap();
    SystemLayout {
        windows: p("C:\\Windows"),
        program_files: vec![p("C:\\Program Files"), p("C:\\Program Files (x86)")],
        program_data: p("C:\\ProgramData"),
        users: p("C:\\Users"),
        profile: p("C:\\Users\\Arthur"),
        public: p("C:\\Users\\Public"),
        other_profiles: vec![p("C:\\Users\\Bob"), p("D:\\Comptes\\claire.DOMAINE")],
    }
}

#[cfg(test)]
mod tests {
    use super::DenyReason::*;
    use super::*;

    fn deny(raw: &str) -> Option<DenyReason> {
        DenyList::new(&test_layout()).check(&WinPath::parse(raw).unwrap())
    }

    #[test]
    fn chaque_entree_de_la_denylist() {
        let cases: &[(&str, DenyReason)] = &[
            ("C:\\", VolumeRoot),
            ("D:\\", VolumeRoot),
            ("C:\\Windows", Windows),
            ("C:\\Windows\\Temp\\x.tmp", Windows),
            ("C:\\Windows\\System32\\config\\SAM", RegistryHive),
            ("C:\\Program Files", ProgramFiles),
            ("C:\\Program Files\\App\\app.exe", ProgramFiles),
            ("C:\\Program Files (x86)\\Steam", ProgramFiles),
            ("C:\\ProgramData", ProgramData),
            ("C:\\ProgramData\\Microsoft\\x", ProgramData),
            ("C:\\Windows.old", WindowsOld),
            ("C:\\Windows.old\\Users", WindowsOld),
            ("C:\\Boot", Boot),
            ("D:\\EFI\\Microsoft", Boot),
            ("C:\\bootmgr", Boot),
            ("C:\\BOOTNXT", Boot),
            ("C:\\System Volume Information", SystemVolumeInformation),
            ("D:\\System Volume Information\\x", SystemVolumeInformation),
            ("C:\\$Recycle.Bin", NtfsMetadata),
            ("D:\\$RECYCLE.BIN\\S-1-5-21\\$R123.txt", NtfsMetadata),
            ("C:\\$Extend\\$UsnJrnl", NtfsMetadata),
            ("C:\\$MFT", NtfsMetadata),
            ("C:\\$LogFile", NtfsMetadata),
            ("C:\\pagefile.sys", SwapFile),
            ("C:\\swapfile.sys", SwapFile),
            ("C:\\hiberfil.sys", SwapFile),
            ("D:\\pagefile.sys", SwapFile),
            ("C:\\Users\\Arthur\\NTUSER.DAT", RegistryHive),
            ("C:\\Users\\Arthur\\ntuser.dat.LOG1", RegistryHive),
            ("C:\\Users\\Arthur\\NTUSER.DAT{a1b2}.TM.blf", RegistryHive),
            ("C:\\Users\\Arthur\\AppData\\Local\\Microsoft\\Windows\\UsrClass.dat", RegistryHive),
            ("C:\\Users\\Arthur\\AppData\\Local\\Microsoft\\Windows\\UsrClass.dat.LOG2", RegistryHive),
            ("E:\\sauvegarde\\Windows\\System32\\config\\SYSTEM", RegistryHive),
            ("C:\\Users\\Default", SharedProfile),
            ("C:\\Users\\Default\\AppData", SharedProfile),
            ("C:\\Users\\Public\\Documents", SharedProfile),
            ("C:\\Users\\Default User", SharedProfile),
            ("C:\\Users\\All Users", SharedProfile),
            ("C:\\Users\\Bob", OtherProfile),
            ("C:\\Users\\Bob\\AppData\\Local\\Temp\\x", OtherProfile),
            ("C:\\Users", ProfileRoot),
            ("C:\\Users\\Arthur", ProfileRoot),
            ("C:\\Users\\Arthur\\Projets\\app\\.git", GitInternals),
            ("C:\\Users\\Arthur\\Projets\\app\\.git\\objects\\ab", GitInternals),
        ];
        for (raw, reason) in cases {
            assert_eq!(deny(raw), Some(*reason), "{raw}");
        }
    }

    #[test]
    fn secrets_et_configurations_jamais_supprimes() {
        let p = "C:\\Users\\Arthur";
        for rel in [
            ".gradle\\gradle.properties",
            ".gradle\\init.d",
            ".gradle\\init.d\\entreprise.gradle",
            ".m2\\settings.xml",
            ".m2\\settings-security.xml",
            ".npmrc",
            ".yarnrc",
            ".yarnrc.yml",
            ".cargo\\credentials",
            ".cargo\\credentials.toml",
            ".cargo\\config",
            ".cargo\\config.toml",
            ".docker\\config.json",
            ".kube",
            ".kube\\config",
            ".ssh",
            ".ssh\\id_ed25519",
            ".aws\\credentials",
            ".azure\\msal_token_cache.json",
            ".gnupg\\private-keys-v1.d\\x.key",
            ".android\\adbkey",
            ".android\\debug.keystore",
            // Parents qui emporteraient ces fichiers : refusés aussi.
            ".gradle",
            ".m2",
            ".cargo",
            ".docker",
            ".android",
        ] {
            assert_eq!(deny(&format!("{p}\\{rel}")), Some(Secret), "{rel}");
        }
        // Navigateurs et applis Electron : stockage, cookies, mots de passe, sessions — même dans un cache.
        let chrome = format!("{p}\\AppData\\Local\\Google\\Chrome\\User Data\\Default");
        for rel in ["Cookies", "Network\\Cookies", "Login Data", "Local Storage\\leveldb\\000003.log", "IndexedDB", "Sessions\\Session_1", "Web Data", "Session Storage"] {
            assert_eq!(deny(&format!("{chrome}\\{rel}")), Some(Secret), "{rel}");
        }
        assert_eq!(deny(&format!("{p}\\AppData\\Roaming\\Mozilla\\Firefox\\Profiles\\x.default\\logins.json")), Some(Secret));
        assert_eq!(deny(&format!("{p}\\AppData\\Roaming\\discord\\Local Storage")), Some(Secret));
        assert_eq!(deny(&format!("{p}\\AppData\\Local\\Temp\\paquet\\.npmrc")), Some(Secret));
        assert_eq!(deny(&format!("{p}\\AppData\\Local\\npm-cache\\.npmrc")), Some(Secret));
        assert_eq!(deny(&format!("{p}\\Projets\\app\\release.jks")), Some(Secret));
        assert_eq!(deny(&format!("{p}\\Documents\\coffre.kdbx")), Some(Secret));
        // Les sous-dossiers de cache, eux, restent autorisés.
        for rel in [
            ".gradle\\caches\\modules-2\\x.jar",
            ".gradle\\wrapper\\dists\\gradle-8",
            ".m2\\repository\\org\\x.jar",
            ".cargo\\registry\\cache\\x.crate",
            ".android\\avd\\Pixel.avd",
            "AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache\\Cache_Data\\f_000001",
            "AppData\\Local\\Google\\Chrome\\User Data\\Default\\Code Cache\\js\\x",
            "AppData\\Local\\npm-cache\\_cacache\\content-v2",
        ] {
            assert_eq!(deny(&format!("{p}\\{rel}")), None, "{rel}");
        }
    }

    #[test]
    fn sauvegardes_de_jeux_jamais_supprimees() {
        let p = "C:\\Users\\Arthur";
        for rel in [
            "AppData\\Local\\Larian Studios\\Baldur's Gate 3\\PlayerProfiles",
            "AppData\\Local\\Larian Studios\\Baldur's Gate 3\\PlayerProfiles\\Public\\Savegames\\Story\\x.lsv",
            "AppData\\LocalLow\\Team Cherry\\Hollow Knight\\Saves\\user1.dat",
            "Documents\\My Games\\Skyrim Special Edition\\Saves\\Save 12.ess",
            "Saved Games",
            "Saved Games\\CD Projekt Red\\Cyberpunk 2077\\ManualSave-3\\sav.dat",
            "AppData\\Roaming\\EldenRing\\76561198000000000\\ER0000.sl2.save",
            "AppData\\Local\\Pal\\Saved\\SaveGames\\0\\Level.sav",
            "curseforge\\minecraft\\Instances\\All the Mods 9\\saves\\Monde",
            "Documents\\copie de partie.SAV",
            "AppData\\Local\\Temp\\x\\SaveData",
        ] {
            assert_eq!(deny(&format!("{p}\\{rel}")), Some(GameSave), "{rel}");
        }
        // Les autres données des jeux et les fichiers au nom proche restent possibles ailleurs.
        for rel in ["AppData\\Local\\Larian Studios\\Baldur's Gate 3\\LevelCache", "Documents\\My Games\\Skyrim Special Edition\\Logs", "Documents\\saves.txt", "Documents\\savoir.pdf"] {
            assert_eq!(deny(&format!("{p}\\{rel}")), None, "{rel}");
        }
    }

    #[test]
    fn windowsapps_et_jeux_du_store_intouchables() {
        for raw in [
            "C:\\Program Files\\WindowsApps\\Microsoft.GamingApp_x",
            "D:\\WindowsApps",
            "D:\\WindowsApps\\Microsoft.624F8B84B80_1.0.0.0_x64__8wekyb3d8bbwe\\Content",
            "E:\\WpSystem\\S-1-5-21\\AppData",
            "D:\\XboxGames",
            "D:\\XboxGames\\Forza Horizon 5\\Content\\x.pak",
            "E:\\.GamingRoot",
        ] {
            assert_eq!(deny(raw), Some(StoreApps), "{raw}");
        }
        // Un dossier nommé XboxGames ailleurs qu'à la racine n'est pas celui de Gaming Services.
        assert_eq!(deny("C:\\Users\\Arthur\\Documents\\XboxGames"), None);
    }

    #[test]
    fn variantes_de_casse() {
        for raw in [
            "c:\\windows",
            "C:\\WINDOWS\\SYSTEM32",
            "c:\\program files\\x",
            "C:\\PROGRAM FILES (X86)",
            "c:\\programdata",
            "C:\\PAGEFILE.SYS",
            "c:\\users\\arthur\\ntuser.dat",
            "C:\\USERS\\DEFAULT",
            "c:\\system volume information",
            "C:\\$recycle.bin",
            "c:\\efi",
        ] {
            assert!(deny(raw).is_some(), "{raw}");
        }
    }

    #[test]
    fn prefixe_verbatim_et_point_point() {
        assert_eq!(deny("\\\\?\\C:\\Windows\\System32"), Some(Windows));
        assert_eq!(deny("\\\\?\\c:\\users\\arthur\\..\\..\\windows"), Some(Windows));
        assert_eq!(deny("C:\\Users\\Arthur\\AppData\\..\\..\\Bob"), Some(OtherProfile));
        assert_eq!(deny("C:\\Users\\Arthur\\Projets\\..\\..\\..\\Program Files"), Some(ProgramFiles));
    }

    #[test]
    fn parents_des_zones_protegees() {
        // Supprimer un parent emporterait la zone : refusé.
        assert_eq!(deny("C:\\Users\\Arthur"), Some(ProfileRoot));
        let custom = SystemLayout { windows: WinPath::parse("D:\\OS\\Windows").unwrap(), ..test_layout() };
        let list = DenyList::new(&custom);
        assert_eq!(list.check(&WinPath::parse("D:\\OS").unwrap()), Some(Windows));
        assert_eq!(list.check(&WinPath::parse("D:\\OS\\Windows\\x").unwrap()), Some(Windows));
        assert_eq!(list.check(&WinPath::parse("D:\\OS\\Windows.old").unwrap()), Some(WindowsOld));
        // Le C:\Windows littéral reste protégé même si Windows est ailleurs.
        assert_eq!(list.check(&WinPath::parse("C:\\Windows").unwrap()), Some(Windows));
    }

    #[test]
    fn zones_utilisateur_autorisees() {
        for raw in [
            "C:\\Users\\Arthur\\AppData\\Local\\npm-cache",
            "C:\\Users\\Arthur\\AppData\\Local\\Temp\\setup.tmp",
            "C:\\Users\\Arthur\\Projets\\app\\node_modules",
            "C:\\Users\\Arthur\\.gradle\\caches",
            "C:\\Users\\Arthur\\Documents\\gitignore.txt",
            "C:\\Users\\Arthur\\Documents\\$facture.pdf",
            "D:\\SteamLibrary\\steamapps\\shadercache",
            "C:\\Users\\Arthur\\Downloads\\Program Files.zip",
            "C:\\Users\\Arthur\\Windows",
        ] {
            assert_eq!(deny(raw), None, "{raw}");
        }
    }

    #[test]
    fn autres_profils_du_registre_quel_que_soit_leur_nom() {
        // Profil d'un autre compte hors de C:\Users : protégé parce qu'il est dans ProfileList,
        // pas à cause de son nom.
        assert_eq!(deny("D:\\Comptes\\claire.DOMAINE\\Documents\\x.docx"), Some(OtherProfile));
        assert_eq!(deny("d:\\comptes\\CLAIRE.domaine"), Some(OtherProfile));
        assert_eq!(deny("D:\\Comptes"), Some(OtherProfile));
        assert_eq!(deny("D:\\Comptes\\claire.DOMAINE2\\x"), None);
        // Le profil courant n'est jamais traité comme un autre profil, même au nom proche.
        let layout = SystemLayout {
            profile: WinPath::parse("D:\\Comptes\\claire.DOMAINE\\arthur").unwrap(),
            ..test_layout()
        };
        let list = DenyList::new(&layout);
        assert_eq!(list.check(&WinPath::parse("D:\\Comptes\\claire.DOMAINE\\arthur\\AppData\\x").unwrap()), None);
    }

    #[test]
    fn faits_du_fichier() {
        let list = DenyList::new(&test_layout());
        assert_eq!(list.check_facts(true, false, false), Some(SystemAttribute));
        assert_eq!(list.check_facts(false, true, true), Some(GitProject));
        assert_eq!(list.check_facts(false, false, true), None);
        assert_eq!(list.check_facts(false, true, false), None);
    }
}
