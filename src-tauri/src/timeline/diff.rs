//! Différence entre deux snapshots d'une même série : dossiers qui ont le plus grossi ou
//! rétréci, nouveaux et disparus. Un dossier n'est cité que s'il explique sa variation : un
//! parent dont un seul sous-dossier porte l'essentiel du changement s'efface devant lui, et un
//! dossier nouveau (ou disparu) ne cite pas ses sous-dossiers.

use std::collections::HashMap;

use serde::Serialize;

/// Part de la variation d'un parent qu'un seul enfant doit porter pour le remplacer.
const EXPLAINED: f64 = 0.9;

/// Une ligne d'un snapshot : dossier (chemin relatif à la racine, `\` entre les noms).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub path: String,
    pub allocated: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Change {
    pub path: String,
    pub before: Option<u64>,
    pub after: Option<u64>,
    pub delta: i64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Diff {
    pub grown: Vec<Change>,
    pub shrunk: Vec<Change>,
    pub added: Vec<Change>,
    pub removed: Vec<Change>,
    /// Variation de la racine.
    pub total: i64,
}

fn parent(path: &str) -> Option<&str> {
    path.rfind('\\').map(|i| &path[..i]).or_else(|| (!path.is_empty()).then_some(""))
}

/// `used_*` : taille de la racine dans chaque snapshot ; `limit` : lignes par liste.
pub fn diff(a: &[Row], b: &[Row], used_a: u64, used_b: u64, limit: usize) -> Diff {
    let before: HashMap<&str, u64> = a.iter().map(|r| (r.path.as_str(), r.allocated)).collect();
    let after: HashMap<&str, u64> = b.iter().map(|r| (r.path.as_str(), r.allocated)).collect();
    let mut both: HashMap<&str, i64> = HashMap::new();
    for (p, &y) in &after {
        if let Some(&x) = before.get(p) {
            both.insert(p, y as i64 - x as i64);
        }
    }
    // Plus forte variation de même signe d'un enfant présent dans les deux snapshots.
    let mut best_child: HashMap<&str, (i64, i64)> = HashMap::new();
    for (&p, &d) in &both {
        if let Some(par) = parent(p) {
            let e = best_child.entry(par).or_insert((0, 0));
            e.0 = e.0.max(d);
            e.1 = e.1.min(d);
        }
    }
    let explained = |p: &str, d: i64| {
        let (up, down) = best_child.get(p).copied().unwrap_or((0, 0));
        let child = if d > 0 { up } else { down };
        child.signum() == d.signum() && (child as f64).abs() >= EXPLAINED * (d as f64).abs()
    };
    let change = |p: &str| Change { path: p.to_owned(), before: before.get(p).copied(), after: after.get(p).copied(), delta: after.get(p).copied().unwrap_or(0) as i64 - before.get(p).copied().unwrap_or(0) as i64 };

    let mut grown: Vec<Change> = both.iter().filter(|(p, &d)| d > 0 && !explained(p, d)).map(|(p, _)| change(p)).collect();
    let mut shrunk: Vec<Change> = both.iter().filter(|(p, &d)| d < 0 && !explained(p, d)).map(|(p, _)| change(p)).collect();
    // Nouveau : absent avant, et son parent était là (sinon c'est le parent qui est nouveau).
    let mut added: Vec<Change> = after.keys().filter(|p| !before.contains_key(*p) && parent(p).map_or(true, |q| q.is_empty() || before.contains_key(q))).map(|p| change(p)).collect();
    let mut removed: Vec<Change> = before.keys().filter(|p| !after.contains_key(*p) && parent(p).map_or(true, |q| q.is_empty() || after.contains_key(q))).map(|p| change(p)).collect();

    let order = |v: &mut Vec<Change>, key: fn(&Change) -> i64| {
        v.sort_by(|x, y| key(y).cmp(&key(x)).then_with(|| x.path.cmp(&y.path)));
        v.truncate(limit);
    };
    order(&mut grown, |c| c.delta);
    order(&mut shrunk, |c| -c.delta);
    order(&mut added, |c| c.delta);
    order(&mut removed, |c| -c.delta);
    Diff { grown, shrunk, added, removed, total: used_b as i64 - used_a as i64 }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1 << 30;

    fn rows(list: &[(&str, u64)]) -> Vec<Row> {
        list.iter().map(|(p, s)| Row { path: (*p).to_owned(), allocated: *s }).collect()
    }

    /// Deux snapshots de fixtures du profil, à une semaine d'écart.
    fn fixtures() -> (Vec<Row>, Vec<Row>) {
        let a = rows(&[
            ("AppData", 40 * GIB),
            ("AppData\\Local", 35 * GIB),
            ("AppData\\Local\\npm-cache", 8 * GIB),
            ("AppData\\Local\\NVIDIA", 2 * GIB),
            ("AppData\\Local\\Temp", 5 * GIB),
            ("AppData\\Roaming", 5 * GIB),
            ("Videos", 60 * GIB),
            ("Videos\\Mariage", 30 * GIB),
            ("Videos\\Rushes", 30 * GIB),
            ("Downloads", 12 * GIB),
            ("Downloads\\iso", 10 * GIB),
            ("Projets", 20 * GIB),
            ("Projets\\vieux", 3 * GIB),
        ]);
        let b = rows(&[
            ("AppData", 52 * GIB),
            ("AppData\\Local", 47 * GIB),
            ("AppData\\Local\\npm-cache", 20 * GIB), // +12 : explique Local et AppData
            ("AppData\\Local\\NVIDIA", 2 * GIB),
            ("AppData\\Local\\Temp", 5 * GIB),
            ("AppData\\Roaming", 5 * GIB),
            ("Videos", 45 * GIB),
            ("Videos\\Mariage", 30 * GIB),
            ("Videos\\Rushes", 15 * GIB), // −15 : explique Videos
            ("Downloads", 4 * GIB), // −8 : iso disparu, Downloads reste cité
            ("Projets", 26 * GIB),
            ("Projets\\nouveau", 9 * GIB), // nouveau
            ("Projets\\nouveau\\node_modules", 6 * GIB), // sous un nouveau : pas cité à part
            ("Jeux", 30 * GIB), // nouveau à la racine
        ]);
        (a, b)
    }

    #[test]
    fn diff_de_deux_snapshots() {
        let (a, b) = fixtures();
        let d = diff(&a, &b, 200 * GIB, 230 * GIB, 10);
        let paths = |v: &[Change]| v.iter().map(|c| c.path.clone()).collect::<Vec<_>>();
        assert_eq!(d.total, 30 * GIB as i64);
        // npm-cache explique AppData\Local et AppData : seul lui est cité.
        assert_eq!(paths(&d.grown), ["AppData\\Local\\npm-cache", "Projets"]);
        assert_eq!(d.grown[0].delta, 12 * GIB as i64);
        assert_eq!((d.grown[0].before, d.grown[0].after), (Some(8 * GIB), Some(20 * GIB)));
        // Rushes explique Videos ; Downloads a perdu un sous-dossier disparu (pas un enfant
        // présent des deux côtés) : il reste cité.
        assert_eq!(paths(&d.shrunk), ["Videos\\Rushes", "Downloads"]);
        assert_eq!(paths(&d.added), ["Jeux", "Projets\\nouveau"]);
        assert_eq!(d.added[1].before, None);
        assert_eq!(paths(&d.removed), ["Downloads\\iso", "Projets\\vieux"]);
        assert_eq!(d.removed[0].delta, -(10 * GIB as i64));
    }

    #[test]
    fn variation_partagee_entre_plusieurs_enfants() {
        // Deux enfants grossissent de 5 Gio chacun : aucun n'explique 90 % du parent, qui est
        // cité avec eux.
        let a = rows(&[("A", 10 * GIB), ("A\\x", 5 * GIB), ("A\\y", 5 * GIB)]);
        let b = rows(&[("A", 20 * GIB), ("A\\x", 10 * GIB), ("A\\y", 10 * GIB)]);
        let d = diff(&a, &b, 10 * GIB, 20 * GIB, 10);
        assert_eq!(d.grown.iter().map(|c| c.path.as_str()).collect::<Vec<_>>(), ["A", "A\\x", "A\\y"]);
        assert!(d.shrunk.is_empty() && d.added.is_empty() && d.removed.is_empty());
    }

    #[test]
    fn limite_et_snapshots_identiques() {
        let (a, _) = fixtures();
        let d = diff(&a, &a, 1, 1, 10);
        assert_eq!(d, Diff::default());
        let (a, b) = fixtures();
        let d = diff(&a, &b, 0, 0, 1);
        assert_eq!((d.grown.len(), d.shrunk.len(), d.added.len(), d.removed.len()), (1, 1, 1, 1));
    }
}
