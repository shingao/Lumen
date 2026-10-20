//! Génère le script PowerShell de Lumen pour des fixtures (tests Pester de la CI Windows).
//!
//! `script <dossier-cache> <sortie.ps1> <âge-en-heures> <corbeille|definitif> <élément relatif>…`
//!
//! Chaque élément passe par `safety` exactement comme dans Lumen : liste blanche (le contenu du
//! dossier cache), denylist, liens, identité ; le script n'en reçoit que les acceptés.

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use safety::allow::{Allowlist, Authority, RuleGrant, Scope};
    use safety::check::{Checker, Expected, ExpectedNode, Target};
    use safety::denylist::DenyList;
    use safety::path::WinPath;
    use safety::plan::{DeleteMode, Plan};
    use safety::probe::{Probe, ProbeError};
    use safety::win::{system_layout, WinEffects, WinProbe};

    fn tree(dir: &WinPath) -> Result<Vec<ExpectedNode>, ProbeError> {
        let mut out = Vec::new();
        for name in WinProbe.list(dir)?.names {
            let p = dir.join(&name).map_err(|e| ProbeError::BadPath(e.to_string()))?;
            let facts = WinProbe.facts(&p)?;
            let children = if facts.attributes.directory && !facts.attributes.reparse_point { tree(&p)? } else { Vec::new() };
            let bytes = if facts.attributes.directory { children.iter().map(|c| c.bytes).sum() } else { facts.size };
            out.push(ExpectedNode { name, expected: Expected::from_facts(&facts), bytes, children });
        }
        Ok(out)
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 6 {
        return Err("usage : script <dossier-cache> <sortie.ps1> <âge-en-heures> <corbeille|definitif> <élément>…".into());
    }
    let cache = WinProbe.canonicalize(&WinPath::parse(&args[1])?)?;
    let age_hours: i64 = args[3].parse()?;
    let mode = if args[4] == "definitif" { DeleteMode::Permanent } else { DeleteMode::Trash };
    let mut allow = Allowlist::new();
    allow.grant(RuleGrant { rule_id: "fixtures".into(), root: cache.clone(), scope: Scope::Contents, min_age_secs: None });
    let deny = DenyList::new(&system_layout()?);
    let mut targets = Vec::new();
    for rel in &args[5..] {
        let path = rel.split('\\').try_fold(cache.clone(), |p, c| p.join(c))?;
        let facts = WinProbe.facts(&path)?;
        let contents = if facts.attributes.directory { Some(tree(&path)?) } else { None };
        let bytes = contents.as_ref().map_or(facts.size, |c| c.iter().map(|n| n.bytes).sum());
        targets.push(Target {
            path: path.display_string(),
            authority: Authority::Rule("fixtures".into()),
            expected: Expected::from_facts(&facts),
            bytes,
            contents,
        });
    }
    let checker = Checker { probe: &WinProbe, deny: &deny, allow: &allow, now: i64::MAX };
    let plan = Plan::build(&checker, &targets, mode);
    for (path, refusal) in plan.refused() {
        eprintln!("refusé : {path} : {refusal}");
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs() as i64;
    let effects = WinEffects::new();
    let script = safety::script::render("Fixtures", &plan, |c| effects.resolve(c).map(|(e, a)| (e.display_string(), a)), now - age_hours * 3600);
    // UTF-8 avec BOM : Windows PowerShell 5.1 lit ainsi les accents correctement.
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(script.as_bytes());
    std::fs::write(&args[2], bytes)?;
    println!("{} élément(s), {} refusé(s)", plan.items().len(), plan.refused().len());
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows uniquement");
}
