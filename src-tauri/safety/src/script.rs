//! Scripts PowerShell lisibles, générés uniquement depuis un plan validé : chemins absolus déjà
//! contrôlés par `safety`, `-WhatIf` par défaut, commentaires en français.
//!
//! Pas de `Remove-Item -Recurse` : sous Windows PowerShell 5.1, il suit les jonctions contenues
//! dans un dossier. La Corbeille passe par l'API du Shell, la suppression définitive par .NET,
//! qui supprime un lien sans jamais entrer dedans.

use crate::commands::OfficialCommand;
use crate::plan::{DeleteMode, Plan};

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

const PRELUDE: &str = r#"$ErrorActionPreference = 'Stop'

function Send-LumenToRecycleBin {
  # Envoie à la Corbeille (annulable) ; respecte -WhatIf.
  [CmdletBinding(SupportsShouldProcess)]
  param([Parameter(Mandatory)][string]$LiteralPath)
  if ($PSCmdlet.ShouldProcess($LiteralPath, 'Envoyer à la Corbeille')) {
    Add-Type -AssemblyName Microsoft.VisualBasic
    if (Test-Path -LiteralPath $LiteralPath -PathType Container) {
      [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($LiteralPath, 'OnlyErrorDialogs', 'SendToRecycleBin')
    } else {
      [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($LiteralPath, 'OnlyErrorDialogs', 'SendToRecycleBin')
    }
  }
}

function Remove-LumenItem {
  # Suppression définitive ; respecte -WhatIf ; ne suit jamais les liens.
  [CmdletBinding(SupportsShouldProcess)]
  param([Parameter(Mandatory)][string]$LiteralPath)
  if ($PSCmdlet.ShouldProcess($LiteralPath, 'Supprimer définitivement')) {
    if (Test-Path -LiteralPath $LiteralPath -PathType Container) {
      [System.IO.Directory]::Delete($LiteralPath, $true)
    } else {
      [System.IO.File]::Delete($LiteralPath)
    }
  }
}
"#;

fn command_line(cmd: &OfficialCommand) -> String {
    let program = cmd.program();
    let exe = if program.is_system_tool() {
        format!("(Join-Path $env:SystemRoot {})", ps_quote(&format!("System32\\{}", program.binary())))
    } else {
        ps_quote(program.binary())
    };
    let args: Vec<String> = cmd.args().iter().map(|a| ps_quote(a)).collect();
    format!("& {exe} {}", args.join(" "))
}

/// Script complet : commandes officielles d'abord, puis les éléments du plan.
pub fn render(title: &str, plan: &Plan, commands: &[OfficialCommand], what_if: bool) -> String {
    let mut s = String::new();
    s.push_str(&format!("# Lumen · {} · {}\n", title.replace(['\r', '\n'], " "), format_size(plan.total_bytes())));
    s.push_str("# Chaque chemin a été vérifié par Lumen : liste blanche, zones protégées, liens, identité du fichier.\n");
    if what_if {
        s.push_str("# SIMULATION : -WhatIf est actif, rien ne sera supprimé. Les commandes sont en commentaire.\n");
    } else {
        s.push_str("# Exécution réelle : relis la liste avant de lancer ce script.\n");
    }
    s.push_str(&format!("# Empreinte du plan : {}\n\n", plan.fingerprint_hex()));
    s.push_str(PRELUDE);

    if commands.iter().any(OfficialCommand::needs_elevation) {
        s.push_str("\n# Certaines commandes demandent une console PowerShell lancée en administrateur.\n");
    }
    for cmd in commands {
        s.push_str(&format!("\n# {}\n", cmd.explanation()));
        let line = command_line(cmd);
        if what_if {
            s.push_str(&format!("# [simulation] {line}\n"));
        } else {
            s.push_str(&format!("{line}\n"));
        }
    }

    if !plan.items().is_empty() {
        let (verb, function) = match plan.mode() {
            DeleteMode::Trash => ("à la Corbeille", "Send-LumenToRecycleBin"),
            DeleteMode::Permanent => ("supprimés DÉFINITIVEMENT", "Remove-LumenItem"),
        };
        s.push_str(&format!("\n# {} élément(s) {verb}\n", plan.items().len()));
        for item in plan.items() {
            let shown = item.path.display_string();
            let path = if shown.chars().count() >= 260 { item.path.verbatim() } else { shown };
            let flag = if what_if { " -WhatIf" } else { "" };
            s.push_str(&format!("# {}\n{function} -LiteralPath {}{flag}\n", format_size(item.bytes), ps_quote(&path)));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::allow::{Allowlist, Authority, RuleGrant, Scope};
    use crate::check::{Checker, Expected, Target};
    use crate::commands::{Drive, OfficialCommand};
    use crate::denylist::{test_layout, DenyList};
    use crate::path::WinPath;
    use crate::probe::memory::MemoryProbe;
    use crate::probe::Probe;

    fn plan(names: &[&str], mode: DeleteMode) -> Plan {
        let probe = MemoryProbe::new();
        for d in ["C:\\Users", "C:\\Users\\Arthur", "C:\\Users\\Arthur\\cache"] {
            probe.dir(d);
        }
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
                let path = probe.file(&raw, 14_388_265_779);
                let facts = probe.facts(&path).unwrap();
                Target { path: raw, authority: Authority::Rule("cache".into()), expected: Expected::from_facts(&facts), bytes: facts.size, contents: None }
            })
            .collect();
        let checker = Checker { probe: &probe, deny: &deny, allow: &allow, now: i64::MAX };
        Plan::build(&checker, &targets, mode)
    }

    #[test]
    fn apostrophes_de_powershell_doublees() {
        assert_eq!(ps_quote("C:\\l'été"), "'C:\\l''été'");
        assert_eq!(ps_quote("a\u{2019}b"), "'a\u{2019}\u{2019}b'");
        assert_eq!(ps_quote("a‘b‚c‛"), "'a‘‘b‚‚c‛‛'");
        assert_eq!(ps_quote("$env:TEMP `x`"), "'$env:TEMP `x`'");
    }

    #[test]
    fn simulation_par_defaut_et_commentaires() {
        let s = render("Cache npm", &plan(&["a.bin", "l'été 2024.mov"], DeleteMode::Trash), &[OfficialCommand::NpmCacheClean], true);
        assert!(s.starts_with("# Lumen · Cache npm · 26,8 Go\n"));
        assert!(s.contains("# SIMULATION"));
        assert!(s.contains("# [simulation] & 'npm' 'cache' 'clean' '--force'"));
        assert!(s.contains("Send-LumenToRecycleBin -LiteralPath 'C:\\Users\\Arthur\\cache\\a.bin' -WhatIf"));
        assert!(s.contains("-LiteralPath 'C:\\Users\\Arthur\\cache\\l''été 2024.mov' -WhatIf"));
        assert!(!s.contains("Remove-Item"));
        // Chaque ligne d'action porte -WhatIf.
        for line in s.lines().filter(|l| l.starts_with("Send-LumenToRecycleBin ")) {
            assert!(line.ends_with(" -WhatIf"), "{line}");
        }
    }

    #[test]
    fn execution_reelle_et_definitive() {
        let s = render("Test", &plan(&["a.bin"], DeleteMode::Permanent), &[OfficialCommand::DiskCleanup(Drive::parse("C").unwrap())], false);
        assert!(s.contains("Remove-LumenItem -LiteralPath 'C:\\Users\\Arthur\\cache\\a.bin'\n"));
        assert!(s.contains("supprimés DÉFINITIVEMENT"));
        assert!(s.contains("& (Join-Path $env:SystemRoot 'System32\\cleanmgr.exe') '/d' 'C:'"));
        assert!(s.contains("administrateur"));
    }

    #[test]
    fn chemins_longs_en_verbatim() {
        let long = ["dossier avec espaces é"; 11].join("_");
        let p = plan(&[long.as_str()], DeleteMode::Trash);
        let s = render("Long", &p, &[], true);
        assert!(s.contains("-LiteralPath '\\\\?\\C:\\Users\\Arthur\\cache\\dossier"));
    }

    #[test]
    fn titre_sans_saut_de_ligne() {
        let s = render("a\nRemove-Item C:\\", &plan(&[], DeleteMode::Trash), &[], true);
        assert!(s.starts_with("# Lumen · a Remove-Item C:\\ · 0 o\n"));
    }

    #[test]
    fn tailles_francaises() {
        assert_eq!(format_size(512), "512 o");
        assert_eq!(format_size(1536), "1,5 Ko");
        assert_eq!(format_size(204 * 1024 * 1024 * 1024), "204 Go");
    }
}
