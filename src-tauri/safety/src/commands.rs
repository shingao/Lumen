//! Commandes officielles des outils : liste fermée de programmes, arguments construits ici en
//! tableau (jamais par concaténation), arguments variables validés.
//!
//! Un binaire n'est jamais lancé par son seul nom : [`Program::candidates`] liste ses chemins
//! complets possibles (emplacements d'installation connus, puis entrées absolues du PATH), et
//! `safety::win` lance le premier qui existe vraiment (fichier ordinaire, pas un lien), par son
//! chemin réel. Les outils système ne viennent que de `%SystemRoot%\System32`.

use crate::path::WinPath;

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
    /// Windows PowerShell de `System32`, pour une seule commande figée : le point de restauration.
    PowerShell,
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
            Program::PowerShell => "powershell.exe",
        }
    }

    pub fn is_system_tool(self) -> bool {
        self.system_path().is_some()
    }

    /// Outil système : chemin sous `%SystemRoot%`, le seul accepté (jamais le PATH).
    pub fn system_path(self) -> Option<&'static [&'static str]> {
        Some(match self {
            Program::Wsl => &["System32", "wsl.exe"],
            Program::Dism => &["System32", "Dism.exe"],
            Program::Powercfg => &["System32", "powercfg.exe"],
            Program::Vssadmin => &["System32", "vssadmin.exe"],
            Program::Cleanmgr => &["System32", "cleanmgr.exe"],
            Program::PowerShell => &["System32", "WindowsPowerShell", "v1.0", "powershell.exe"],
            _ => return None,
        })
    }

    /// Dossiers d'installation connus, avant le PATH.
    fn known_dirs(self, d: &ToolDirs) -> Vec<WinPath> {
        let under = |base: &Option<WinPath>, rel: &[&str]| base.as_ref().and_then(|b| join_all(b, rel));
        let mut out: Vec<Option<WinPath>> = Vec::new();
        match self {
            Program::Npm => {
                out.extend(d.program_files.iter().map(|pf| join_all(pf, &["nodejs"])));
                out.push(under(&d.local_app_data, &["Programs", "nodejs"]));
            }
            Program::Pnpm => {
                out.push(under(&d.local_app_data, &["pnpm"]));
                out.push(under(&d.app_data, &["npm"]));
            }
            Program::Yarn => {
                out.extend(d.program_files.iter().map(|pf| join_all(pf, &["Yarn", "bin"])));
                out.push(under(&d.app_data, &["npm"]));
            }
            Program::Uv => {
                out.push(under(&d.user_profile, &[".local", "bin"]));
                out.push(under(&d.user_profile, &[".cargo", "bin"]));
            }
            Program::Dotnet => out.extend(d.program_files.iter().map(|pf| join_all(pf, &["dotnet"]))),
            Program::Docker => out.extend(d.program_files.iter().map(|pf| join_all(pf, &["Docker", "Docker", "resources", "bin"]))),
            Program::Ollama => out.push(under(&d.local_app_data, &["Programs", "Ollama"])),
            Program::Pip | Program::Gradle => {}
            Program::Wsl | Program::Dism | Program::Powercfg | Program::Vssadmin | Program::Cleanmgr | Program::PowerShell => {}
        }
        out.into_iter().flatten().collect()
    }

    /// Chemins complets possibles, dans l'ordre de préférence. Outils système : `System32` seul.
    pub fn candidates(self, d: &ToolDirs) -> Vec<Candidate> {
        if let Some(rel) = self.system_path() {
            return d
                .system_root
                .as_ref()
                .and_then(|r| join_all(r, rel))
                .map(|exe| Candidate { exe, script: None })
                .into_iter()
                .collect();
        }
        let mut dirs = self.known_dirs(d);
        for p in &d.path {
            if !dirs.contains(p) {
                dirs.push(p.clone());
            }
        }
        let mut out = Vec::new();
        for dir in dirs {
            if self == Program::Npm {
                // npm.cmd ne fait que lancer node sur npm-cli.js : on lance node directement.
                if let (Some(exe), Some(script)) =
                    (join_all(&dir, &["node.exe"]), join_all(&dir, &["node_modules", "npm", "bin", "npm-cli.js"]))
                {
                    out.push(Candidate { exe, script: Some(script) });
                }
                continue;
            }
            for ext in ["exe", "cmd", "bat"] {
                if let Some(exe) = join_all(&dir, &[&format!("{}.{ext}", self.binary())]) {
                    out.push(Candidate { exe, script: None });
                }
            }
        }
        out
    }
}

fn join_all(base: &WinPath, rel: &[&str]) -> Option<WinPath> {
    rel.iter().try_fold(base.clone(), |p, c| p.join(c).ok())
}

/// Où chercher les binaires : dossiers connus de Windows et entrées absolues du PATH.
#[derive(Debug, Clone, Default)]
pub struct ToolDirs {
    pub system_root: Option<WinPath>,
    pub program_files: Vec<WinPath>,
    pub local_app_data: Option<WinPath>,
    pub app_data: Option<WinPath>,
    pub user_profile: Option<WinPath>,
    /// Entrées du PATH déjà filtrées par [`path_entries`].
    pub path: Vec<WinPath>,
}

/// Entrées du PATH utilisables : chemins absolus sur un lecteur local (ni relatifs, ni réseau,
/// ni `.`), hors des dossiers exclus (dossier temporaire), sans doublon.
pub fn path_entries(raw: &str, exclude: &[WinPath]) -> Vec<WinPath> {
    let mut out: Vec<WinPath> = Vec::new();
    for entry in raw.split(';') {
        let entry = entry.trim().trim_matches('"');
        let drive_absolute = entry.len() >= 3 && entry.as_bytes()[1] == b':' && matches!(entry.as_bytes()[2], b'\\' | b'/');
        if !drive_absolute || entry.contains('%') {
            continue;
        }
        let Ok(p) = WinPath::parse(entry) else { continue };
        if exclude.iter().any(|x| p.starts_with(x)) || out.contains(&p) {
            continue;
        }
        out.push(p);
    }
    out
}

/// Un emplacement possible du binaire : l'exécutable, et pour npm le script lancé par node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub exe: WinPath,
    pub script: Option<WinPath>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ArgError {
    #[error("nom de modèle Ollama invalide : « {0} »")]
    ModelName(String),
    #[error("lettre de lecteur invalide : « {0} »")]
    Drive(String),
    #[error("identifiant de cliché instantané invalide : « {0} »")]
    ShadowId(String),
    #[error("fichier de sortie invalide : « {0} »")]
    OutputFile(String),
    #[error("disque virtuel invalide : « {0} »")]
    Vhdx(String),
}

/// Disque virtuel WSL ou Docker (`ext4.vhdx`, `docker_data.vhdx`) : chemin absolu sur une
/// lettre de lecteur, extension `.vhdx`, sans guillemet ni caractère de contrôle. Il n'est
/// jamais supprimé : seulement compacté, par un script figé qui revérifie tout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Vhdx(WinPath);

impl Vhdx {
    pub fn new(path: &WinPath) -> Result<Vhdx, ArgError> {
        let text = path.display_string();
        let bad = || ArgError::Vhdx(text.clone());
        let name = path.file_name().unwrap_or_default();
        if !matches!(path.prefix(), crate::path::Prefix::Drive(_)) || !name.to_ascii_lowercase().ends_with(".vhdx") || name.len() <= 5 {
            return Err(bad());
        }
        if text.chars().any(|c| c == '"' || c == '`' || c.is_control()) {
            return Err(bad());
        }
        Ok(Vhdx(path.clone()))
    }

    pub fn path(&self) -> &WinPath {
        &self.0
    }
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

impl Drive {
    pub fn letter(self) -> char {
        self.0
    }
}

/// Identifiant d'un cliché instantané (point de restauration) : `{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowId(String);

impl ShadowId {
    pub fn parse(raw: &str) -> Result<ShadowId, ArgError> {
        let inner = raw.strip_prefix('{').and_then(|r| r.strip_suffix('}')).unwrap_or("");
        let groups: Vec<&str> = inner.split('-').collect();
        let lens = [8, 4, 4, 4, 12];
        let ok = groups.len() == 5 && groups.iter().zip(lens).all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()));
        if ok {
            Ok(ShadowId(raw.to_ascii_uppercase()))
        } else {
            Err(ArgError::ShadowId(raw.to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Fichier écrit par une commande élevée (journal de DISM, liste des points de restauration) :
/// chemin absolu sur un lecteur local, nom de fichier simple, aucun guillemet ni caractère de
/// contrôle. Il est passé entre guillemets à la commande, ou entre apostrophes doublées dans un
/// script figé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputFile(WinPath);

impl OutputFile {
    pub fn new(dir: &WinPath, name: &str) -> Result<OutputFile, ArgError> {
        let bad = |s: &str| ArgError::OutputFile(s.to_owned());
        let name_ok = !name.is_empty() && name.len() <= 80 && !name.starts_with('.') && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !name_ok || !matches!(dir.prefix(), crate::path::Prefix::Drive(_)) {
            return Err(bad(name));
        }
        let path = dir.join(name).map_err(|_| bad(name))?;
        let text = path.display_string();
        if text.chars().any(|c| c == '"' || c.is_control()) {
            return Err(bad(&text));
        }
        Ok(OutputFile(path))
    }

    pub fn path(&self) -> &WinPath {
        &self.0
    }

    pub fn display_string(&self) -> String {
        self.0.display_string()
    }
}

/// Un cliché instantané d'un volume, tel que l'a lu la commande élevée de lecture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowCopy {
    pub id: ShadowId,
    /// Création, en secondes depuis 1970.
    pub created: i64,
}

/// Âge minimal du point de restauration le plus ancien pour qu'il puisse être supprimé.
pub const OLDEST_MIN_AGE_SECS: i64 = 7 * 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OldestRefusal {
    #[error("il faut au moins 2 points de restauration : le dernier restant n'est jamais supprimé")]
    TooFew,
    #[error("le plus ancien point de restauration date de moins de 7 jours ou de cette session : il est gardé")]
    TooRecent,
}

/// Date limite (secondes depuis 1970) : le point supprimé doit être plus ancien que 7 jours ET
/// que le début de la session de Lumen (un point créé par Lumen n'est jamais celui-là).
pub fn oldest_not_after(now: i64, session_start: i64) -> i64 {
    (now - OLDEST_MIN_AGE_SECS).min(session_start)
}

/// Le plus ancien point de restauration peut-il être supprimé ? Jamais s'il en reste moins de
/// 2, jamais s'il est récent. Mêmes règles que le script, qui les revérifie au moment de l'action.
pub fn oldest_deletable(shadows: &[ShadowCopy], not_after: i64) -> Result<&ShadowCopy, OldestRefusal> {
    if shadows.len() < 2 {
        return Err(OldestRefusal::TooFew);
    }
    let oldest = shadows.iter().min_by_key(|s| s.created).ok_or(OldestRefusal::TooFew)?;
    if oldest.created >= not_after {
        return Err(OldestRefusal::TooRecent);
    }
    Ok(oldest)
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
    /// Images Docker sans étiquette (`dangling`), celles que liste `DockerDanglingImages`
    /// (confirmation spécifique dans l'interface).
    DockerImagePrune,
    /// Compactage d'un disque virtuel WSL ou Docker (élevé, après `wsl --shutdown`) :
    /// `Optimize-VHD` si le module Hyper-V est là, sinon `diskpart` « compact vdisk » sur le
    /// disque attaché en lecture seule. Jamais de suppression du fichier.
    VhdCompact(Vhdx),
    /// Lecture seule : images Docker sans étiquette (identifiant, dépôt, étiquette, taille, âge),
    /// une par ligne. Échoue si Docker ne tourne pas.
    DockerDanglingImages,
    OllamaRm(ModelName),
    WslShutdown,
    /// Magasin de composants (WinSxS), sans `/ResetBase` ; sortie gardée par `/LogPath`.
    DismComponentCleanup(Option<OutputFile>),
    /// Désactive l'hibernation : plus de démarrage rapide ni de mise en veille prolongée.
    HibernateOff,
    /// Hibernation réduite : garde le démarrage rapide, plus de mise en veille prolongée.
    HibernateReduced,
    /// Supprime uniquement le plus ancien point de restauration du lecteur, jamais tous, et
    /// seulement s'il est celui qui a été affiché (`shadow`), s'il en reste au moins un autre et
    /// s'il est antérieur à `not_after` (7 jours et début de la session). Script figé qui
    /// revérifie tout au moment de l'action, puis `vssadmin delete shadows /shadow=…`.
    ShadowDeleteOldest { drive: Drive, shadow: ShadowId, not_after: i64 },
    /// Lecture seule : clichés instantanés et points de restauration du lecteur, écrits en JSON
    /// dans `out` (aucune suppression).
    RestorePointsList { drive: Drive, out: OutputFile },
    /// Nettoyage de disque de Windows (Windows.old, fichiers système).
    DiskCleanup(Drive),
    /// Point de restauration du système avant les actions système (script PowerShell figé).
    RestorePointCreate,
}

/// Script du point de restauration, figé dans le binaire et passé en `-EncodedCommand` :
/// aucun texte variable. Codes de sortie : 0 créé, 3 un point existe déjà depuis moins que la
/// fréquence autorisée par Windows (24 h par défaut), 4 Windows n'a pas créé de point,
/// 5 protection du système indisponible.
pub const RESTORE_POINT_SCRIPT: &str = r#"$ErrorActionPreference = 'Stop'
$clients = @()
try { $clients = @((Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SPP\Clients').'{09F7EDC5-294E-4180-AF6A-FB0E6A0E9513}') } catch {}
$lecteur = '(' + $env:SystemDrive.TrimEnd(':') + '%3A)'
if (-not ($clients | Where-Object { $_ -like "*$lecteur*" })) { exit 5 }
try { $avant = @(Get-ComputerRestorePoint) } catch { exit 5 }
$frequence = 1440
try {
  $v = (Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore' -Name SystemRestorePointCreationFrequency).SystemRestorePointCreationFrequency
  if ($null -ne $v) { $frequence = [int]$v }
} catch {}
if ($avant.Count -gt 0) {
  $dernier = $avant | Sort-Object SequenceNumber | Select-Object -Last 1
  $date = [Management.ManagementDateTimeConverter]::ToDateTime($dernier.CreationTime)
  if (((Get-Date) - $date).TotalMinutes -lt $frequence) { exit 3 }
}
try { Checkpoint-Computer -Description 'Lumen : avant nettoyage du système' -RestorePointType MODIFY_SETTINGS } catch { exit 4 }
if (@(Get-ComputerRestorePoint).Count -le $avant.Count) { exit 4 }
exit 0
"#;

/// Suppression du plus ancien point de restauration : modèle figé, seuls la lettre du lecteur,
/// l'identifiant attendu et la date limite (valeurs validées) y sont insérés. Codes de sortie :
/// 0 supprimé, 6 moins de 2 points (rien n'est fait), 7 le plus ancien est trop récent,
/// 8 le plus ancien n'est plus celui qui a été affiché, 9 lecture ou suppression impossible.
const SHADOW_DELETE_TEMPLATE: &str = r#"$ErrorActionPreference = 'Stop'
$attendu = '@ID@'
try {
  $limite = [DateTimeOffset]::FromUnixTimeSeconds(@LIMITE@).UtcDateTime
  $volume = (Get-CimInstance -ClassName Win32_Volume | Where-Object { $_.DriveLetter -eq '@D@:' }).DeviceID
  $copies = @(Get-CimInstance -ClassName Win32_ShadowCopy | Where-Object { $_.VolumeName -eq $volume })
} catch { exit 9 }
if ($copies.Count -lt 2) { exit 6 }
$plusAncien = $copies | Sort-Object InstallDate | Select-Object -First 1
if ($plusAncien.ID.ToUpperInvariant() -ne $attendu) { exit 8 }
if ($plusAncien.InstallDate.ToUniversalTime() -ge $limite) { exit 7 }
try { & (Join-Path $env:SystemRoot 'System32\vssadmin.exe') delete shadows "/shadow=$attendu" /quiet } catch { exit 9 }
if ($LASTEXITCODE -ne 0) { exit 9 }
exit 0
"#;

/// Lecture des points de restauration : modèle figé, lettre du lecteur et fichier de sortie
/// validés. Lecture seule.
const RESTORE_POINTS_LIST_TEMPLATE: &str = r#"$ErrorActionPreference = 'Stop'
$sortie = '@SORTIE@'
$secondes = { param($d) [DateTimeOffset]::new($d.ToUniversalTime()).ToUnixTimeSeconds() }
try {
  $volume = (Get-CimInstance -ClassName Win32_Volume | Where-Object { $_.DriveLetter -eq '@D@:' }).DeviceID
  $copies = @(Get-CimInstance -ClassName Win32_ShadowCopy | Where-Object { $_.VolumeName -eq $volume } | ForEach-Object { [ordered]@{ id = $_.ID; created = (& $secondes $_.InstallDate) } })
} catch { exit 9 }
$points = @()
try {
  $points = @(Get-ComputerRestorePoint | ForEach-Object { [ordered]@{ description = $_.Description; created = (& $secondes ([Management.ManagementDateTimeConverter]::ToDateTime($_.CreationTime))) } })
} catch {}
$json = ConvertTo-Json -Depth 4 -Compress ([ordered]@{ shadows = $copies; points = $points })
[IO.File]::WriteAllText($sortie, $json, (New-Object Text.UTF8Encoding $false))
exit 0
"#;

/// Chaîne entre apostrophes de PowerShell : les apostrophes sont doublées.
fn ps_literal(s: &str) -> String {
    s.replace('\'', "''")
}

/// Compactage d'un disque virtuel, chemin validé inséré : revérifie que c'est un fichier
/// `.vhdx` ordinaire (ni lien ni parent lien), puis `Optimize-VHD` monté en lecture seule, ou
/// `diskpart` (attach readonly, compact, detach). Seul le script diskpart temporaire est retiré.
/// Codes : 0 compacté, 10 absent, 11 pas un vhdx ordinaire, 12 Optimize-VHD, 13 diskpart.
pub fn vhd_compact_script(vhdx: &Vhdx) -> String {
    format!(
        r#"$ErrorActionPreference = 'Stop'
$vhd = '{path}'
if (-not (Test-Path -LiteralPath $vhd -PathType Leaf)) {{ exit 10 }}
$item = Get-Item -LiteralPath $vhd -Force
if (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -or ($item.Extension -ne '.vhdx')) {{ exit 11 }}
$dir = $item.Directory
while ($dir) {{ if ($dir.Attributes -band [IO.FileAttributes]::ReparsePoint) {{ exit 11 }}; $dir = $dir.Parent }}
if (Get-Command Optimize-VHD -ErrorAction SilentlyContinue) {{
  try {{ Mount-VHD -Path $vhd -ReadOnly; Optimize-VHD -Path $vhd -Mode Full }} catch {{ exit 12 }} finally {{ try {{ Dismount-VHD -Path $vhd }} catch {{}} }}
  exit 0
}}
$etapes = Join-Path $env:TEMP ('lumen-compact-' + [guid]::NewGuid().ToString() + '.txt')
@('select vdisk file="' + $vhd + '"', 'attach vdisk readonly', 'compact vdisk', 'detach vdisk') | Set-Content -Encoding ascii -LiteralPath $etapes
$code = 1
try {{ & (Join-Path $env:SystemRoot 'System32\diskpart.exe') /s $etapes | Out-Null; $code = $LASTEXITCODE }} catch {{ $code = 1 }} finally {{ Remove-Item -LiteralPath $etapes -Force -ErrorAction SilentlyContinue }}
if ($code -ne 0) {{ exit 13 }}
exit 0
"#,
        path = ps_literal(&vhdx.path().display_string())
    )
}

/// Plus grande date que `DateTimeOffset` accepte (31/12/9999) : au-delà, la conversion échouerait.
const MAX_UNIX_SECS: i64 = 253_402_300_799;

/// Script de suppression du plus ancien point de restauration, valeurs validées insérées.
pub fn shadow_delete_script(drive: Drive, shadow: &ShadowId, not_after: i64) -> String {
    let limit = not_after.clamp(0, MAX_UNIX_SECS);
    SHADOW_DELETE_TEMPLATE.replace("@D@", &drive.0.to_string()).replace("@ID@", shadow.as_str()).replace("@LIMITE@", &limit.to_string())
}

/// Script de lecture des points de restauration, valeurs validées insérées.
pub fn restore_points_list_script(drive: Drive, out: &OutputFile) -> String {
    RESTORE_POINTS_LIST_TEMPLATE.replace("@D@", &drive.0.to_string()).replace("@SORTIE@", &ps_literal(&out.display_string()))
}

fn powershell_args(script: &str) -> Vec<String> {
    let mut a: Vec<String> = ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand"].iter().map(|s| (*s).to_owned()).collect();
    a.push(encoded_command(script));
    a
}

/// Base64 standard (sans dépendance) du script en UTF-16LE, comme l'attend `-EncodedCommand`.
fn encoded_command(script: &str) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(T[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
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
            DockerImagePrune | DockerDanglingImages => Program::Docker,
            OllamaRm(_) => Program::Ollama,
            WslShutdown => Program::Wsl,
            DismComponentCleanup(_) => Program::Dism,
            HibernateOff | HibernateReduced => Program::Powercfg,
            DiskCleanup(_) => Program::Cleanmgr,
            RestorePointCreate | ShadowDeleteOldest { .. } | RestorePointsList { .. } | VhdCompact(_) => Program::PowerShell,
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
            DockerImagePrune => v(&["image", "prune", "--force"]),
            DockerDanglingImages => v(&["image", "ls", "--filter", "dangling=true", "--format", "{{.ID}}\t{{.Repository}}\t{{.Tag}}\t{{.Size}}\t{{.CreatedSince}}"]),
            OllamaRm(model) => vec!["rm".into(), model.as_str().to_owned()],
            WslShutdown => v(&["--shutdown"]),
            DismComponentCleanup(log) => {
                let mut a = v(&["/Online", "/Cleanup-Image", "/StartComponentCleanup"]);
                if let Some(log) = log {
                    a.push(format!("/LogPath:{}", log.display_string()));
                }
                a
            }
            HibernateOff => v(&["/h", "off"]),
            HibernateReduced => v(&["/h", "/type", "reduced"]),
            ShadowDeleteOldest { drive, shadow, not_after } => powershell_args(&shadow_delete_script(*drive, shadow, *not_after)),
            RestorePointsList { drive, out } => powershell_args(&restore_points_list_script(*drive, out)),
            DiskCleanup(Drive(d)) => vec!["/d".into(), format!("{d}:")],
            RestorePointCreate => powershell_args(RESTORE_POINT_SCRIPT),
            VhdCompact(vhdx) => powershell_args(&vhd_compact_script(vhdx)),
        }
    }

    /// Résultat lisible d'un code de sortie particulier (sinon le code brut est affiché).
    pub fn exit_meaning(&self, code: i32) -> Option<&'static str> {
        match (self, code) {
            (OfficialCommand::RestorePointCreate, 0) => Some("point de restauration créé"),
            (OfficialCommand::RestorePointCreate, 3) => Some(
                "un point de restauration existe déjà depuis moins de 24 h : Windows n'en crée pas d'autre avant ce délai (réglage SystemRestorePointCreationFrequency)",
            ),
            (OfficialCommand::RestorePointCreate, 4) => Some("Windows n'a pas créé de point de restauration"),
            (OfficialCommand::RestorePointCreate, 5) => {
                Some("Windows ne peut pas créer de point de restauration sur ce disque : la protection du système y est désactivée")
            }
            (OfficialCommand::ShadowDeleteOldest { .. }, 0) => Some("plus ancien point de restauration supprimé"),
            (OfficialCommand::ShadowDeleteOldest { .. }, 6) => Some("moins de 2 points de restauration : rien n'a été supprimé"),
            (OfficialCommand::ShadowDeleteOldest { .. }, 7) => {
                Some("le plus ancien point de restauration date de moins de 7 jours ou de cette session : il est gardé")
            }
            (OfficialCommand::ShadowDeleteOldest { .. }, 8) => {
                Some("le plus ancien point de restauration n'est plus celui qui était affiché : rien n'a été supprimé")
            }
            (OfficialCommand::ShadowDeleteOldest { .. } | OfficialCommand::RestorePointsList { .. }, 9) => {
                Some("lecture ou suppression des points de restauration impossible")
            }
            (OfficialCommand::DismComponentCleanup(Some(_)), _) => Some("sortie complète dans le journal de DISM (/LogPath)"),
            (OfficialCommand::VhdCompact(_), 0) => Some("disque virtuel compacté"),
            (OfficialCommand::VhdCompact(_), 10) => Some("disque virtuel introuvable : rien n'a été fait"),
            (OfficialCommand::VhdCompact(_), 11) => Some("ce n'est plus un disque virtuel ordinaire (lien ou autre fichier) : rien n'a été fait"),
            (OfficialCommand::VhdCompact(_), 12) => Some("Optimize-VHD a échoué (disque encore utilisé ?) : le disque est intact"),
            (OfficialCommand::VhdCompact(_), 13) => Some("diskpart n'a pas pu compacter le disque : il est intact"),
            _ => None,
        }
    }

    /// Un prérequis est-il rempli avec ce code ? Point de restauration : créé, ou déjà récent.
    pub fn satisfied_by(&self, code: i32) -> bool {
        match self {
            OfficialCommand::RestorePointCreate => code == 0 || code == 3,
            _ => code == 0,
        }
    }

    /// Fenêtre visible : seul le Nettoyage de disque est une appli interactive.
    pub fn shows_window(&self) -> bool {
        matches!(self, OfficialCommand::DiskCleanup(_))
    }

    /// Délai maximal : au-delà, la commande et ses processus enfants sont arrêtés.
    pub fn timeout_secs(&self) -> u64 {
        use OfficialCommand::*;
        match self {
            GradleStop | WslShutdown | DockerDanglingImages => 120,
            NpmCacheClean | PnpmStorePrune | YarnCacheClean | PipCachePurge | UvCacheClean | NugetLocalsClear => 1800,
            NpmCacheVerify | DockerImagePrune | OllamaRm(_) => 900,
            DismComponentCleanup(_) | DiskCleanup(_) => 3600,
            HibernateOff | HibernateReduced | ShadowDeleteOldest { .. } | RestorePointsList { .. } => 300,
            RestorePointCreate => 900,
            VhdCompact(_) => 7200,
        }
    }

    /// Action système qui appelle un point de restauration avant elle (gouffres système) ; le
    /// compactage d'un disque virtuel WSL ou Docker ne touche pas au système.
    pub fn wants_restore_point(&self) -> bool {
        self.needs_elevation() && !matches!(self, OfficialCommand::VhdCompact(_) | OfficialCommand::RestorePointsList { .. })
    }

    /// Commande de lecture seule, lançable hors d'un plan (rien n'est modifié).
    pub fn read_only(&self) -> bool {
        matches!(self, OfficialCommand::DockerDanglingImages | OfficialCommand::RestorePointsList { .. })
    }

    pub fn needs_elevation(&self) -> bool {
        use OfficialCommand::*;
        matches!(
            self,
            DismComponentCleanup(_)
                | HibernateOff
                | HibernateReduced
                | ShadowDeleteOldest { .. }
                | DiskCleanup(_)
                | RestorePointCreate
                | RestorePointsList { .. }
                | VhdCompact(_)
        )
    }

    /// Fichier écrit par la commande, rattaché à l'entrée du journal (journal de DISM).
    pub fn log_file(&self) -> Option<&OutputFile> {
        match self {
            OfficialCommand::DismComponentCleanup(log) => log.as_ref(),
            _ => None,
        }
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
            DockerImagePrune => "Supprime les images Docker sans étiquette (dangling), qu'aucun conteneur n'utilise",
            DockerDanglingImages => "Liste les images Docker sans étiquette (aucune modification)",
            VhdCompact(_) => "Compacte le disque virtuel de WSL ou de Docker (jamais supprimé)",
            OllamaRm(_) => "Supprime le modèle d'Ollama (retéléchargeable avec ollama pull)",
            WslShutdown => "Arrête WSL et Docker avant le compactage de leur disque virtuel",
            DismComponentCleanup(_) => "Nettoie le magasin de composants de Windows (WinSxS) avec l'outil officiel",
            HibernateOff => "Désactive l'hibernation : plus de démarrage rapide ni de mise en veille prolongée",
            HibernateReduced => "Hibernation réduite : garde le démarrage rapide, plus de veille prolongée",
            ShadowDeleteOldest { .. } => {
                "Supprime uniquement le plus ancien point de restauration, s'il en reste un autre et s'il date de plus de 7 jours"
            }
            RestorePointsList { .. } => "Lit la liste des points de restauration (aucune modification)",
            DiskCleanup(_) => "Ouvre le Nettoyage de disque de Windows (Windows.old : « Installation(s) précédente(s) de Windows »)",
            RestorePointCreate => "Crée un point de restauration du système avant les actions système",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disque_virtuel_compacte_jamais_supprime() {
        let p = |s: &str| WinPath::parse(s).unwrap();
        let ok = Vhdx::new(&p("C:\\Users\\Arthur\\AppData\\Local\\Packages\\Ubuntu\\LocalState\\ext4.vhdx")).unwrap();
        let script = vhd_compact_script(&ok);
        assert!(script.contains("$vhd = 'C:\\Users\\Arthur\\AppData\\Local\\Packages\\Ubuntu\\LocalState\\ext4.vhdx'"));
        assert!(script.contains("Mount-VHD -Path $vhd -ReadOnly") && script.contains("attach vdisk readonly") && script.contains("compact vdisk"));
        // Jamais de suppression du disque : le seul Remove-Item vise le script diskpart temporaire.
        assert_eq!(script.matches("Remove-Item").count(), 1);
        assert!(script.contains("Remove-Item -LiteralPath $etapes"));
        assert!(!script.contains("Remove-Item -LiteralPath $vhd") && !script.to_lowercase().contains("delete"));
        let cmd = OfficialCommand::VhdCompact(ok);
        assert!(cmd.needs_elevation() && !cmd.read_only());
        assert_eq!(cmd.program(), Program::PowerShell);
        // Apostrophe doublée dans le littéral PowerShell ; guillemet, autre extension ou chemin
        // réseau refusés.
        let quote = Vhdx::new(&p("D:\\WSL\\l'arch\\ext4.vhdx")).unwrap();
        assert!(vhd_compact_script(&quote).contains("'D:\\WSL\\l''arch\\ext4.vhdx'"));
        assert!(Vhdx::new(&p("D:\\WSL\\ext4.vhd")).is_err());
        assert!(Vhdx::new(&p("D:\\WSL\\.vhdx")).is_err());
        assert!(Vhdx::new(&p("\\\\serveur\\partage\\ext4.vhdx")).is_err());
    }

    #[test]
    fn docker_seulement_les_images_listees() {
        // La liste affichée et ce qui part sont les mêmes images : sans étiquette, jamais --all.
        assert_eq!(OfficialCommand::DockerImagePrune.args(), ["image", "prune", "--force"]);
        assert!(OfficialCommand::DockerDanglingImages.args().contains(&"dangling=true".to_owned()));
        assert!(OfficialCommand::DockerDanglingImages.read_only() && !OfficialCommand::DockerImagePrune.read_only());
        assert!(!OfficialCommand::NpmCacheClean.read_only());
    }

    #[test]
    fn arguments_en_tableau() {
        assert_eq!(OfficialCommand::NpmCacheClean.args(), ["cache", "clean", "--force"]);
        let rm = OfficialCommand::OllamaRm(ModelName::parse("llama3:8b").unwrap());
        assert_eq!(rm.program(), Program::Ollama);
        assert_eq!(rm.args(), ["rm", "llama3:8b"]);
    }

    const ID: &str = "{0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9}";
    const DAY: i64 = 86_400;

    fn shadow(id: u8, created: i64) -> ShadowCopy {
        ShadowCopy { id: ShadowId::parse(&format!("{{0000000{id}-0000-0000-0000-000000000000}}")).unwrap(), created }
    }

    #[test]
    fn points_de_restauration_jamais_tous() {
        let d = Drive::parse("C").unwrap();
        let script = shadow_delete_script(d, &ShadowId::parse(ID).unwrap(), 1_700_000_000);
        // Le cliché affiché, et lui seul ; jamais /all ni /oldest à l'aveugle.
        assert!(script.contains("\"/shadow=$attendu\""));
        assert!(script.contains(&format!("$attendu = '{}'", ID.to_ascii_uppercase())));
        assert!(!script.to_ascii_lowercase().contains("/all") && !script.contains("/oldest"));
        // Mêmes règles que `oldest_deletable` : au moins 2 points, assez ancien, celui affiché.
        assert!(script.contains("$copies.Count -lt 2) { exit 6 }"));
        assert!(script.contains("FromUnixTimeSeconds(1700000000)"));
        // Date hors de ce que PowerShell accepte : bornée, jamais d'exception non rattrapée.
        assert!(shadow_delete_script(d, &ShadowId::parse(ID).unwrap(), i64::MAX).contains("FromUnixTimeSeconds(253402300799)"));
        assert!(shadow_delete_script(d, &ShadowId::parse(ID).unwrap(), -5).contains("FromUnixTimeSeconds(0)"));
        assert!(script.contains("-ge $limite) { exit 7 }"));
        assert!(script.contains("-ne $attendu) { exit 8 }"));
        let cmd = OfficialCommand::ShadowDeleteOldest { drive: d, shadow: ShadowId::parse(ID).unwrap(), not_after: 1_700_000_000 };
        assert_eq!(cmd.program(), Program::PowerShell);
        assert!(cmd.needs_elevation());
        assert_eq!(cmd.args()[4], "-EncodedCommand");
    }

    #[test]
    fn un_seul_point_qui_vient_d_etre_cree_jamais_supprime() {
        let now = 1_800_000_000;
        let session = now - 3600;
        let limit = oldest_not_after(now, session);
        // Lumen vient de créer le seul point de restauration : il n'est jamais supprimé.
        assert_eq!(oldest_deletable(&[shadow(1, now - 60)], limit), Err(OldestRefusal::TooFew));
        assert_eq!(oldest_deletable(&[], limit), Err(OldestRefusal::TooFew));
        // Deux points, dont le plus ancien date de 3 jours : gardé.
        assert_eq!(oldest_deletable(&[shadow(1, now - 60), shadow(2, now - 3 * DAY)], limit), Err(OldestRefusal::TooRecent));
        // Session ouverte depuis 10 jours : un point créé pendant la session est gardé même s'il a 8 jours.
        let long = oldest_not_after(now, now - 10 * DAY);
        assert_eq!(long, now - 10 * DAY);
        assert_eq!(oldest_deletable(&[shadow(1, now - 8 * DAY), shadow(2, now)], long), Err(OldestRefusal::TooRecent));
        // Le plus ancien a 30 jours, un autre reste : seul celui-là peut partir.
        let points = [shadow(1, now - 60), shadow(2, now - 30 * DAY), shadow(3, now - 10 * DAY)];
        assert_eq!(oldest_deletable(&points, limit).unwrap().id, shadow(2, 0).id);
    }

    #[test]
    fn identifiants_et_fichiers_de_sortie_valides() {
        assert!(ShadowId::parse(ID).is_ok());
        for bad in ["", "0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F9", "{0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8F}", "{x}", "{0A1B2C3D-4E5F-6071-8293-A4B5C6D7E8FZ}", "{'; rm}"] {
            assert!(ShadowId::parse(bad).is_err(), "{bad}");
        }
        let logs = WinPath::parse("C:\\Users\\Jean O'Brien\\AppData\\Local\\Lumen\\logs").unwrap();
        let f = OutputFile::new(&logs, "dism-20261021-120000.log").unwrap();
        let dism = OfficialCommand::DismComponentCleanup(Some(f.clone()));
        assert_eq!(dism.args().last().unwrap(), "/LogPath:C:\\Users\\Jean O'Brien\\AppData\\Local\\Lumen\\logs\\dism-20261021-120000.log");
        assert_eq!(dism.log_file(), Some(&f));
        assert!(!dism.args().iter().any(|a| a.eq_ignore_ascii_case("/ResetBase")));
        for bad in ["", "..", "a b.log", "x\\y.log", "x\"y.log", ".cache"] {
            assert!(OutputFile::new(&logs, bad).is_err(), "{bad}");
        }
        assert!(OutputFile::new(&WinPath::parse("\\\\serveur\\partage").unwrap(), "x.log").is_err());
        // Apostrophes doublées dans le script de lecture.
        let list = restore_points_list_script(Drive::parse("C").unwrap(), &OutputFile::new(&logs, "restore-points.json").unwrap());
        assert!(list.contains("$sortie = 'C:\\Users\\Jean O''Brien\\"));
        assert!(!list.contains("delete"));
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

    fn tool_dirs(path: &str) -> ToolDirs {
        let p = |s: &str| WinPath::parse(s).unwrap();
        ToolDirs {
            system_root: Some(p("C:\\Windows")),
            program_files: vec![p("C:\\Program Files"), p("C:\\Program Files (x86)")],
            local_app_data: Some(p("C:\\Users\\Arthur\\AppData\\Local")),
            app_data: Some(p("C:\\Users\\Arthur\\AppData\\Roaming")),
            user_profile: Some(p("C:\\Users\\Arthur")),
            path: path_entries(path, &[p("C:\\Users\\Arthur\\AppData\\Local\\Temp")]),
        }
    }

    #[test]
    fn entrees_du_path_absolues_seulement() {
        let raw = "C:\\Windows\\system32;;.;bin;\\\\serveur\\outils;\"C:\\Program Files\\nodejs\\\";%USERPROFILE%\\bin;C:\\Users\\Arthur\\AppData\\Local\\Temp\\x;c:/tools;C:\\WINDOWS\\System32";
        let entries: Vec<String> = tool_dirs(raw).path.iter().map(|p| p.display_string()).collect();
        assert_eq!(entries, ["C:\\Windows\\system32", "C:\\Program Files\\nodejs", "C:\\tools"]);
    }

    #[test]
    fn binaires_par_chemin_complet() {
        let d = tool_dirs("D:\\outils;C:\\Program Files\\nodejs");
        // Outils système : System32 uniquement, jamais le PATH.
        let dism = Program::Dism.candidates(&d);
        assert_eq!(dism, [Candidate { exe: WinPath::parse("C:\\Windows\\System32\\dism.exe").unwrap(), script: None }]);
        // npm : node.exe + npm-cli.js, emplacement connu d'abord, PATH ensuite, sans doublon.
        let npm = Program::Npm.candidates(&d);
        assert_eq!(npm[0].exe.display_string(), "C:\\Program Files\\nodejs\\node.exe");
        assert_eq!(npm[0].script.as_ref().unwrap().display_string(), "C:\\Program Files\\nodejs\\node_modules\\npm\\bin\\npm-cli.js");
        assert_eq!(npm.iter().filter(|c| c.exe.display_string() == "C:\\Program Files\\nodejs\\node.exe").count(), 1);
        assert!(npm.iter().any(|c| c.exe.display_string() == "D:\\outils\\node.exe"));
        // Autres : .exe avant .cmd / .bat, emplacements connus avant le PATH.
        let pnpm: Vec<String> = Program::Pnpm.candidates(&d).iter().map(|c| c.exe.display_string()).collect();
        assert_eq!(pnpm[0], "C:\\Users\\Arthur\\AppData\\Local\\pnpm\\pnpm.exe");
        assert_eq!(pnpm[1], "C:\\Users\\Arthur\\AppData\\Local\\pnpm\\pnpm.cmd");
        assert!(pnpm.contains(&"D:\\outils\\pnpm.exe".to_owned()));
        assert!(Program::Gradle.candidates(&d).iter().all(|c| c.exe.components().len() >= 2));
        assert!(Program::Gradle.candidates(&tool_dirs("")).is_empty());
    }

    #[test]
    fn delais_maximaux() {
        assert_eq!(OfficialCommand::GradleStop.timeout_secs(), 120);
        assert!(OfficialCommand::NpmCacheClean.timeout_secs() <= 1800);
    }

    #[test]
    fn point_de_restauration_script_fige() {
        let args = OfficialCommand::RestorePointCreate.args();
        assert_eq!(&args[..5], ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand"]);
        // Base64 de l'UTF-16LE : « $E » → 24 00 45 00.
        assert!(args[5].starts_with("JABFAHIAcgBvAHIA"), "{}", &args[5][..20]);
        assert!(args[5].chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=')));
        assert_eq!(encoded_command("ab"), "YQBiAA==");
        assert_eq!(encoded_command("abc"), "YQBiAGMA");
        let c = OfficialCommand::RestorePointCreate;
        assert!(c.needs_elevation() && c.satisfied_by(0) && c.satisfied_by(3) && !c.satisfied_by(4));
        assert!(c.exit_meaning(3).unwrap().contains("24 h"));
        assert!(!RESTORE_POINT_SCRIPT.contains("-All") && !RESTORE_POINT_SCRIPT.contains("Remove"));
    }

    #[test]
    fn outils_systeme_par_chemin_complet() {
        let d = tool_dirs("C:\\Windows\\System32");
        let ps = Program::PowerShell.candidates(&d);
        assert_eq!(ps[0].exe.display_string(), "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe");
        assert_eq!(ps.len(), 1);
        for p in [Program::Dism, Program::Powercfg, Program::Vssadmin, Program::Cleanmgr] {
            let c = p.candidates(&d);
            assert_eq!(c.len(), 1, "{p:?}");
            assert!(c[0].exe.display_string().starts_with("C:\\Windows\\System32\\"));
        }
        // Jamais tous les points de restauration, jamais /ResetBase.
        let dism = OfficialCommand::DismComponentCleanup(None).args();
        assert!(!dism.iter().any(|a| a.eq_ignore_ascii_case("/ResetBase")));
    }

    #[test]
    fn outils_systeme_et_elevation() {
        assert!(Program::Dism.is_system_tool());
        assert!(!Program::Npm.is_system_tool());
        assert!(OfficialCommand::DismComponentCleanup(None).needs_elevation());
        assert!(!OfficialCommand::PipCachePurge.needs_elevation());
    }
}
