//! Script PowerShell autonome, généré uniquement depuis un plan validé. Il s'exécute HORS de
//! `safety`, donc il embarque ses propres garde-fous :
//!
//! - date de génération dans l'en-tête, refus de s'exécuter au-delà de 24 h ;
//! - pour chaque chemin : `Test-Path`, refus si l'élément ou l'un de ses parents est un lien
//!   (attribut ReparsePoint) ou un fichier cloud, refus si la taille ou la date diffère de la
//!   génération (pour un dossier : nombre de fichiers et de dossiers, taille totale et date la
//!   plus récente, mesurés sans jamais entrer dans un lien ; un dossier qui contient un lien est
//!   refusé), refus hors d'un disque local fixe ;
//! - Corbeille via `Microsoft.VisualBasic.FileIO` (`SendToRecycleBin`), jamais
//!   `Remove-Item -Recurse` ; la suppression définitive (option) passe par .NET, qui supprime un
//!   lien sans jamais entrer dedans ;
//! - simulation par défaut (`-WhatIf` actif tant que `-Executer` n'est pas donné),
//!   `$ErrorActionPreference = 'Stop'` élément par élément, journal dans un fichier, bilan final.
//!
//! Les commandes officielles sont écrites avec le chemin complet résolu par Lumen au moment de la
//! génération ; le script vérifie que le binaire est toujours là avant de le lancer.

use crate::check::ExpectedNode;
use crate::commands::OfficialCommand;
use crate::plan::{CommandRole, DeleteMode, Plan};

/// Durée de validité d'un script, en heures.
pub const SCRIPT_TTL_HOURS: u32 = 24;

/// Chaîne PowerShell entre apostrophes. PowerShell traite aussi ‘ ’ ‚ ‛ comme des apostrophes :
/// elles sont doublées comme `'`, sinon un nom de fichier pourrait fermer la chaîne.
pub fn ps_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// Taille en base 1024, à la française (`13,4 Go`).
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["o", "Ko", "Mo", "Go", "To", "Po"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        return format!("{bytes} o");
    }
    let text = if value >= 100.0 { format!("{value:.0}") } else { format!("{value:.1}") };
    format!("{} {}", text.replace('.', ","), UNITS[unit])
}

const PRELUDE: &str = r#"
[CmdletBinding(SupportsShouldProcess)]
param(
  # Sans ce commutateur, rien n'est supprimé ni lancé : simulation (-WhatIf).
  [switch]$Executer,
  # Journal de cette exécution.
  [string]$Journal = (Join-Path $env:LOCALAPPDATA ('Lumen\scripts\script-' + (Get-Date -Format 'yyyyMMdd-HHmmss') + '.log'))
)

Set-StrictMode -Version 2
$ErrorActionPreference = 'Stop'
if (-not $Executer) { $WhatIfPreference = $true }
$Bilan = @{ Faits = 0; Simules = 0; Sautes = 0; Refuses = 0; Echecs = 0 }
$Cloud = 0x441000 # OFFLINE | RECALL_ON_OPEN | RECALL_ON_DATA_ACCESS

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Journal) -WhatIf:$false | Out-Null

function Write-LumenJournal([string]$Message) {
  Add-Content -LiteralPath $Journal -Value ('{0:o}  {1}' -f (Get-Date).ToUniversalTime(), $Message) -Encoding UTF8 -WhatIf:$false
  Write-Host $Message
}

function Test-LumenLien([string]$Chemin) {
  # Vrai si l'élément ou l'un de ses parents est un lien (jonction, lien symbolique) ou dans le cloud.
  $courant = $Chemin
  while ($courant) {
    $attributs = [int][System.IO.File]::GetAttributes($courant)
    if (($attributs -band [int][System.IO.FileAttributes]::ReparsePoint) -or ($attributs -band $Cloud)) { return $true }
    $parent = [System.IO.Path]::GetDirectoryName($courant)
    if ($parent -eq $courant) { break }
    $courant = $parent
  }
  return $false
}

function Get-LumenMesure([string]$Chemin) {
  # Parcourt le dossier sans jamais entrer dans un lien.
  $m = @{ Fichiers = 0; Dossiers = 0; Taille = [int64]0; Date = [int64]0; Liens = 0 }
  $pile = New-Object System.Collections.Stack
  $pile.Push((New-Object System.IO.DirectoryInfo $Chemin))
  while ($pile.Count -gt 0) {
    foreach ($e in $pile.Pop().EnumerateFileSystemInfos()) {
      $a = [int]$e.Attributes
      if (($a -band [int][System.IO.FileAttributes]::ReparsePoint) -or ($a -band $Cloud)) { $m.Liens++; continue }
      if ($e -is [System.IO.DirectoryInfo]) { $m.Dossiers++; $pile.Push($e); continue }
      $m.Fichiers++
      $m.Taille += $e.Length
      $t = $e.LastWriteTimeUtc.ToFileTimeUtc()
      if ($t -gt $m.Date) { $m.Date = $t }
    }
  }
  return $m
}

function Send-LumenToRecycleBin {
  # Envoie à la Corbeille (annulable) ; respecte -WhatIf.
  [CmdletBinding(SupportsShouldProcess)]
  param([Parameter(Mandatory)][string]$LiteralPath, [switch]$Dossier)
  if ($PSCmdlet.ShouldProcess($LiteralPath, 'Envoyer à la Corbeille')) {
    Add-Type -AssemblyName Microsoft.VisualBasic
    $ui = [Microsoft.VisualBasic.FileIO.UIOption]::OnlyErrorDialogs
    $corbeille = [Microsoft.VisualBasic.FileIO.RecycleOption]::SendToRecycleBin
    if ($Dossier) {
      [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($LiteralPath, $ui, $corbeille)
    } else {
      [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($LiteralPath, $ui, $corbeille)
    }
    return $true
  }
  return $false
}

function Remove-LumenItem {
  # Suppression définitive ; respecte -WhatIf ; ne suit jamais les liens.
  [CmdletBinding(SupportsShouldProcess)]
  param([Parameter(Mandatory)][string]$LiteralPath, [switch]$Dossier)
  if ($PSCmdlet.ShouldProcess($LiteralPath, 'Supprimer définitivement')) {
    if ($Dossier) {
      [System.IO.Directory]::Delete($LiteralPath, $true)
    } else {
      [System.IO.File]::Delete($LiteralPath)
    }
    return $true
  }
  return $false
}

function Invoke-LumenElement([hashtable]$E) {
  $ErrorActionPreference = 'Stop'
  $chemin = $E.Chemin
  try {
    if (-not (Test-Path -LiteralPath $chemin)) {
      $Bilan.Sautes++; Write-LumenJournal "SAUTÉ (introuvable) : $chemin"; return
    }
    $refus = $null
    $lecteur = New-Object System.IO.DriveInfo ([System.IO.Path]::GetPathRoot($chemin))
    if ($lecteur.DriveType -ne [System.IO.DriveType]::Fixed) { $refus = 'pas un disque local fixe' }
    elseif (Test-LumenLien $chemin) { $refus = 'lien, jonction ou fichier cloud sur le chemin' }
    else {
      $item = Get-Item -LiteralPath $chemin -Force
      if ($E.Type -eq 'Fichier') {
        if ($item -isnot [System.IO.FileInfo]) { $refus = "n'est plus un fichier" }
        elseif ($item.Length -ne $E.Taille -or $item.LastWriteTimeUtc.ToFileTimeUtc() -ne $E.Date) { $refus = 'taille ou date différente de la génération' }
      } else {
        if ($item -isnot [System.IO.DirectoryInfo]) { $refus = "n'est plus un dossier" }
        else {
          $m = Get-LumenMesure $chemin
          if ($m.Liens -gt 0) { $refus = 'contient un lien ou un fichier cloud' }
          elseif ($m.Fichiers -ne $E.Fichiers -or $m.Dossiers -ne $E.Dossiers -or $m.Taille -ne $E.Taille -or $m.Date -ne $E.Date) {
            $refus = 'contenu différent de la génération'
          }
        }
      }
    }
    if ($refus) { $Bilan.Refuses++; Write-LumenJournal "REFUSÉ ($refus) : $chemin"; return }
    $dossier = $E.Type -eq 'Dossier'
    if ($Mode -eq 'Corbeille') { $fait = Send-LumenToRecycleBin -LiteralPath $chemin -Dossier:$dossier }
    else { $fait = Remove-LumenItem -LiteralPath $chemin -Dossier:$dossier }
    if ($fait) { $Bilan.Faits++; Write-LumenJournal "$Verbe : $chemin" }
    else { $Bilan.Simules++; Write-LumenJournal "[simulation] $Verbe : $chemin" }
  } catch {
    $Bilan.Echecs++
    Write-LumenJournal "ÉCHEC : $chemin : $($_.Exception.Message)"
  }
}

function Invoke-LumenCommande {
  # Commande officielle, par le chemin complet résolu à la génération ; respecte -WhatIf.
  [CmdletBinding(SupportsShouldProcess)]
  param([Parameter(Mandatory)][string]$Programme, [string[]]$Arguments = @(), [switch]$Admin)
  $ErrorActionPreference = 'Stop'
  try {
    if (-not (Test-Path -LiteralPath $Programme -PathType Leaf) -or (Test-LumenLien $Programme)) {
      $Bilan.Refuses++; Write-LumenJournal "REFUSÉ (binaire introuvable ou lien) : $Programme"; return
    }
    $principal = New-Object Security.Principal.WindowsPrincipal ([Security.Principal.WindowsIdentity]::GetCurrent())
    if ($Admin -and -not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
      $Bilan.Sautes++; Write-LumenJournal "SAUTÉ (console administrateur nécessaire) : $Programme $Arguments"; return
    }
    if ($PSCmdlet.ShouldProcess("$Programme $Arguments", 'Lancer')) {
      # Les lignes d'erreur d'un outil ne doivent pas interrompre le script (PowerShell 5.1).
      $ErrorActionPreference = 'Continue'
      & $Programme @Arguments 2>&1 | ForEach-Object { Write-LumenJournal "  $_" }
      $ErrorActionPreference = 'Stop'
      if ($LASTEXITCODE -ne 0) { throw "code de sortie $LASTEXITCODE" }
      $Bilan.Faits++; Write-LumenJournal "LANCÉ : $Programme $Arguments"
    } else {
      $Bilan.Simules++; Write-LumenJournal "[simulation] lancerait : $Programme $Arguments"
    }
  } catch {
    $Bilan.Echecs++
    Write-LumenJournal "ÉCHEC : $Programme : $($_.Exception.Message)"
  }
}
"#;

const EPILOGUE: &str = r#"
Write-LumenJournal ("Bilan : {0} fait(s), {1} simulé(s), {2} sauté(s), {3} refusé(s), {4} échec(s). Journal : {5}" -f $Bilan.Faits, $Bilan.Simules, $Bilan.Sautes, $Bilan.Refuses, $Bilan.Echecs, $Journal)
if ($Bilan.Echecs -gt 0) { exit 1 }
"#;

/// Date ISO 8601 en UTC d'un horodatage Unix (secondes).
pub fn iso_utc(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    // Jours depuis 1970 → date civile (algorithme de H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", secs / 3600, secs % 3600 / 60, secs % 60)
}

/// Mesure d'un dossier telle que le script la refait : fichiers, dossiers, taille, date la plus récente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DirMeasure {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub newest: i64,
}

pub fn measure(nodes: &[ExpectedNode]) -> DirMeasure {
    let mut m = DirMeasure::default();
    for n in nodes {
        if n.expected.directory {
            m.dirs += 1;
            let inner = measure(&n.children);
            m.files += inner.files;
            m.dirs += inner.dirs;
            m.bytes += inner.bytes;
            m.newest = m.newest.max(inner.newest);
        } else {
            m.files += 1;
            m.bytes += n.expected.size;
            m.newest = m.newest.max(n.expected.modified);
        }
    }
    m
}

/// Binaire d'une commande, résolu par Lumen (chemin complet + arguments en tête, ex. npm-cli.js).
pub type Resolved = Option<(String, Vec<String>)>;

fn command_lines(s: &mut String, cmd: &OfficialCommand, role: CommandRole, resolved: Resolved) {
    let what = match role {
        CommandRole::Prerequisite => "Prérequis",
        CommandRole::Action => "Commande officielle",
    };
    s.push_str(&format!("\n# {what} : {}\n", cmd.explanation()));
    let program = cmd.program();
    let (exe, mut args) = if program.is_system_tool() {
        (format!("(Join-Path $env:SystemRoot {})", ps_quote(&format!("System32\\{}", program.binary()))), Vec::new())
    } else {
        match resolved {
            Some((exe, prefix)) => (ps_quote(&exe), prefix.iter().map(|a| ps_quote(a)).collect()),
            None => {
                s.push_str(&format!("# {} introuvable sur cette machine à la génération : commande omise.\n", program.binary()));
                return;
            }
        }
    };
    args.extend(cmd.args().iter().map(|a| ps_quote(a)));
    let admin = if cmd.needs_elevation() { " -Admin" } else { "" };
    s.push_str(&format!("Invoke-LumenCommande -Programme {exe} -Arguments @({}){admin}\n", args.join(", ")));
}

/// Script complet : en-tête daté, garde-fous, commandes du plan (prérequis d'abord), éléments.
/// `generated_at` : horodatage Unix (secondes, UTC) de la génération.
pub fn render(title: &str, plan: &Plan, resolve: impl Fn(&OfficialCommand) -> Resolved, generated_at: i64) -> String {
    let mut s = String::new();
    let generated = iso_utc(generated_at);
    s.push_str(&format!("# Lumen · {} · {}\n", title.replace(['\r', '\n'], " "), format_size(plan.total_bytes())));
    s.push_str(&format!("# Généré le {generated} (UTC). Refuse de s'exécuter au-delà de {SCRIPT_TTL_HOURS} h : régénère-le depuis Lumen.\n"));
    s.push_str("# Chaque chemin a été vérifié par Lumen : liste blanche, zones protégées, liens, identité du fichier.\n");
    s.push_str("# Le script revérifie tout avant d'agir : chemin présent, aucun lien sur le chemin ni dans un dossier,\n");
    s.push_str("# taille et date identiques à la génération. Sinon l'élément est refusé et laissé en place.\n");
    s.push_str("# SIMULATION PAR DÉFAUT : rien n'est supprimé ni lancé. Pour agir, relis la liste puis lance-le avec -Executer.\n");
    s.push_str(&format!("# Empreinte du plan : {}\n", plan.fingerprint_hex()));
    s.push_str(PRELUDE);
    let (mode, verb) = match plan.mode() {
        DeleteMode::Trash => ("Corbeille", "À LA CORBEILLE"),
        DeleteMode::Permanent => ("Definitif", "SUPPRIMÉ DÉFINITIVEMENT"),
    };
    s.push_str(&format!("\n$GenereLe = [DateTime]::Parse('{generated}', [Globalization.CultureInfo]::InvariantCulture, [Globalization.DateTimeStyles]::AdjustToUniversal)\n"));
    s.push_str(&format!("$Mode = '{mode}'\n$Verbe = '{verb}'\n"));
    s.push_str(&format!(
        "$age = ((Get-Date).ToUniversalTime() - $GenereLe).TotalHours\n\
         if ($age -gt {SCRIPT_TTL_HOURS} -or $age -lt -1) {{\n  \
         Write-LumenJournal (\"REFUS : script généré le {generated} UTC, il y a {{0:N0}} h ; il expire au bout de {SCRIPT_TTL_HOURS} h. Régénère-le depuis Lumen.\" -f $age)\n  \
         exit 2\n}}\n"
    ));
    s.push_str("if ($WhatIfPreference) { Write-LumenJournal 'SIMULATION : rien ne sera supprimé ni lancé (-Executer pour agir).' }\n");

    if plan.commands().iter().any(|c| c.command.needs_elevation()) {
        s.push_str("\n# Certaines commandes demandent une console PowerShell lancée en administrateur.\n");
    }
    for c in plan.commands() {
        command_lines(&mut s, &c.command, c.role, resolve(&c.command));
    }

    if !plan.items().is_empty() {
        let what = match plan.mode() {
            DeleteMode::Trash => "à la Corbeille",
            DeleteMode::Permanent => "supprimés DÉFINITIVEMENT",
        };
        s.push_str(&format!("\n# {} élément(s) {what}\n", plan.items().len()));
        for item in plan.items() {
            let shown = item.requested.display_string();
            let path = ps_quote(&if shown.chars().count() >= 260 { item.requested.verbatim() } else { shown });
            match &item.contents {
                Some(nodes) => {
                    let m = measure(nodes);
                    s.push_str(&format!("# {} · dossier, {} fichier(s)\n", format_size(item.bytes), m.files));
                    s.push_str(&format!(
                        "Invoke-LumenElement @{{ Chemin = {path}; Type = 'Dossier'; Fichiers = {}; Dossiers = {}; Taille = {}; Date = {} }}\n",
                        m.files, m.dirs, m.bytes, m.newest
                    ));
                }
                None => {
                    s.push_str(&format!("# {}\n", format_size(item.bytes)));
                    s.push_str(&format!(
                        "Invoke-LumenElement @{{ Chemin = {path}; Type = 'Fichier'; Taille = {}; Date = {} }}\n",
                        item.expected.size, item.expected.modified
                    ));
                }
            }
        }
    }
    s.push_str(EPILOGUE);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allow::{Allowlist, Authority, RuleGrant, Scope};
    use crate::check::{Checker, Target};
    use crate::denylist::{test_layout, DenyList};
    use crate::path::WinPath;
    use crate::plan::PlannedCommand;
    use crate::probe::memory::MemoryProbe;

    const GENERATED: i64 = 1_792_526_400; // 2026-10-20T20:00:00Z

    fn plan(names: &[&str], mode: DeleteMode) -> Plan {
        let probe = MemoryProbe::new();
        for d in ["C:\\Users", "C:\\Users\\Arthur", "C:\\Users\\Arthur\\cache", "C:\\Users\\Arthur\\cache\\dossier"] {
            probe.dir(d);
        }
        probe.file("C:\\Users\\Arthur\\cache\\dossier\\x.bin", 7);
        probe.dir("C:\\Users\\Arthur\\cache\\dossier\\sous");
        probe.file("C:\\Users\\Arthur\\cache\\dossier\\sous\\y.bin", 5);
        let mut allow = Allowlist::new();
        allow.grant(RuleGrant {
            rule_id: "cache".into(),
            root: WinPath::parse("C:\\Users\\Arthur\\cache").unwrap(),
            scope: Scope::Contents,
            min_age_secs: None,
        });
        let deny = DenyList::new(&test_layout());
        let targets: Vec<Target> = names
            .iter()
            .map(|n| {
                let raw = format!("C:\\Users\\Arthur\\cache\\{n}");
                if !probe.exists(&raw) {
                    probe.file(&raw, 14_388_265_779);
                }
                probe.target(&raw, Authority::Rule("cache".into()))
            })
            .collect();
        let checker = Checker { probe: &probe, deny: &deny, allow: &allow, now: i64::MAX };
        Plan::build(&checker, &targets, mode)
    }

    fn none(_: &OfficialCommand) -> Resolved {
        None
    }

    #[test]
    fn apostrophes_de_powershell_doublees() {
        assert_eq!(ps_quote("C:\\l'été"), "'C:\\l''été'");
        assert_eq!(ps_quote("a\u{2019}b"), "'a\u{2019}\u{2019}b'");
        assert_eq!(ps_quote("a‘b‚c‛"), "'a‘‘b‚‚c‛‛'");
        assert_eq!(ps_quote("$env:TEMP `x`"), "'$env:TEMP `x`'");
    }

    #[test]
    fn date_de_generation_et_expiration() {
        assert_eq!(iso_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_utc(GENERATED), "2026-10-20T20:00:00Z");
        assert_eq!(iso_utc(951_782_400), "2000-02-29T00:00:00Z");
        let s = render("Cache", &plan(&["a.bin"], DeleteMode::Trash), none, GENERATED);
        assert!(s.contains("# Généré le 2026-10-20T20:00:00Z (UTC). Refuse de s'exécuter au-delà de 24 h"));
        assert!(s.contains("[DateTime]::Parse('2026-10-20T20:00:00Z'"));
        assert!(s.contains("if ($age -gt 24 -or $age -lt -1) {"));
        assert!(s.contains("exit 2"));
    }

    #[test]
    fn simulation_par_defaut_et_garde_fous() {
        let s = render("Cache npm", &plan(&["a.bin", "l'été 2024.mov"], DeleteMode::Trash), none, GENERATED);
        assert!(s.starts_with("# Lumen · Cache npm · 26,8 Go\n"));
        assert!(s.contains("SIMULATION PAR DÉFAUT"));
        assert!(s.contains("if (-not $Executer) { $WhatIfPreference = $true }"));
        assert!(s.contains("$ErrorActionPreference = 'Stop'"));
        assert!(s.contains("Invoke-LumenElement @{ Chemin = 'C:\\Users\\Arthur\\cache\\a.bin'; Type = 'Fichier'; Taille = 14388265779; Date = 133000000000000000 }"));
        assert!(s.contains("Chemin = 'C:\\Users\\Arthur\\cache\\l''été 2024.mov'"));
        assert!(s.contains("[Microsoft.VisualBasic.FileIO.RecycleOption]::SendToRecycleBin"));
        assert!(s.contains("[System.IO.FileAttributes]::ReparsePoint"));
        assert!(s.contains("GetDirectoryName"), "les parents sont contrôlés");
        assert!(s.contains("Add-Content -LiteralPath $Journal"));
        assert!(!s.contains("Remove-Item"));
        assert!(s.contains("$Mode = 'Corbeille'"));
    }

    #[test]
    fn dossier_mesure_sans_suivre_les_liens() {
        let s = render("Dossier", &plan(&["dossier"], DeleteMode::Trash), none, GENERATED);
        assert!(s.contains("Chemin = 'C:\\Users\\Arthur\\cache\\dossier'; Type = 'Dossier'; Fichiers = 2; Dossiers = 1; Taille = 12; Date = 133000000000000000 }"), "{s}");
        assert!(s.contains("$m.Liens++; continue"));
    }

    #[test]
    fn commandes_par_chemin_complet() {
        let p = plan(&["a.bin"], DeleteMode::Permanent).with_commands(vec![
            PlannedCommand { command: OfficialCommand::DiskCleanup(crate::commands::Drive::parse("C").unwrap()), role: CommandRole::Action, measured: None, bytes: 0 },
            PlannedCommand { command: OfficialCommand::NpmCacheClean, role: CommandRole::Action, measured: None, bytes: 0 },
            PlannedCommand { command: OfficialCommand::GradleStop, role: CommandRole::Prerequisite, measured: None, bytes: 0 },
        ]);
        let resolve = |c: &OfficialCommand| match c {
            OfficialCommand::NpmCacheClean => Some(("C:\\Program Files\\nodejs\\node.exe".to_owned(), vec!["C:\\Program Files\\nodejs\\node_modules\\npm\\bin\\npm-cli.js".to_owned()])),
            _ => None,
        };
        let s = render("Test", &p, resolve, GENERATED);
        assert!(s.contains("Invoke-LumenCommande -Programme 'C:\\Program Files\\nodejs\\node.exe' -Arguments @('C:\\Program Files\\nodejs\\node_modules\\npm\\bin\\npm-cli.js', 'cache', 'clean', '--force')\n"));
        assert!(s.contains("Invoke-LumenCommande -Programme (Join-Path $env:SystemRoot 'System32\\cleanmgr.exe') -Arguments @('/d', 'C:') -Admin"));
        assert!(s.contains("# gradle introuvable sur cette machine à la génération : commande omise."));
        assert!(s.find("Prérequis").unwrap() < s.find("cleanmgr").unwrap());
        assert!(s.contains("$Mode = 'Definitif'"));
        assert!(s.contains("administrateur"));
    }

    #[test]
    fn chemins_longs_en_verbatim() {
        let long = ["dossier avec espaces é"; 11].join("_");
        let p = plan(&[long.as_str()], DeleteMode::Trash);
        let s = render("Long", &p, none, GENERATED);
        assert!(s.contains("Chemin = '\\\\?\\C:\\Users\\Arthur\\cache\\dossier"));
    }

    #[test]
    fn titre_sans_saut_de_ligne() {
        let s = render("a\nRemove-Item C:\\", &plan(&[], DeleteMode::Trash), none, GENERATED);
        assert!(s.starts_with("# Lumen · a Remove-Item C:\\ · 0 o\n"));
    }

    #[test]
    fn tailles_francaises() {
        assert_eq!(format_size(512), "512 o");
        assert_eq!(format_size(1536), "1,5 Ko");
        assert_eq!(format_size(204 * 1024 * 1024 * 1024), "204 Go");
    }
}
