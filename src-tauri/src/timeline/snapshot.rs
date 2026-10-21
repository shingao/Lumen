//! Snapshot d'une analyse : tailles agrégées par dossier sur 4 niveaux au plus sous la racine,
//! jamais la liste des fichiers. Les dossiers de moins de 1 Mio ne sont pas gardés (ils restent
//! comptés dans leur parent), et au plus 5 000 dossiers, les plus gros : un parent est toujours
//! au moins aussi gros que ses enfants, donc un dossier gardé a toujours ses parents.

use scan::{Kind, NodeId, Scan};

pub const MAX_DEPTH: u8 = 4;
pub const MIN_BYTES: u64 = 1 << 20;
pub const MAX_ROWS: usize = 5000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotRow {
    pub path: String,
    pub allocated: u64,
    pub files: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Taken {
    /// Taille allouée de la racine.
    pub used: u64,
    pub files: u64,
    pub rows: Vec<SnapshotRow>,
    /// Plus petit dossier gardé (seuil effectif, au moins `MIN_BYTES`).
    pub min_bytes: u64,
}

pub fn from_scan(scan: &Scan) -> Taken {
    let root = scan.root();
    let (used, files) = scan.node(root).map_or((0, 0), |n| (n.allocated, u64::from(n.files)));
    let mut rows = Vec::new();
    let mut stack: Vec<(NodeId, u8, String)> = vec![(root, 0, String::new())];
    while let Some((id, depth, path)) = stack.pop() {
        if depth == MAX_DEPTH {
            continue;
        }
        for child in scan.children(id) {
            let Some(n) = scan.node(child) else { continue };
            if n.kind != Kind::Dir || n.allocated < MIN_BYTES {
                continue;
            }
            let name = scan.name(child);
            let p = if path.is_empty() { name.to_owned() } else { format!("{path}\\{name}") };
            rows.push(SnapshotRow { path: p.clone(), allocated: n.allocated, files: u64::from(n.files) });
            stack.push((child, depth + 1, p));
        }
    }
    rows.sort_by(|a, b| b.allocated.cmp(&a.allocated).then_with(|| a.path.cmp(&b.path)));
    rows.truncate(MAX_ROWS);
    let min_bytes = if rows.len() == MAX_ROWS { rows.last().map_or(MIN_BYTES, |r| r.allocated.max(MIN_BYTES)) } else { MIN_BYTES };
    Taken { used, files, rows, min_bytes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scan::{Method, RawEntry, Stats};

    const MIB: u64 = 1 << 20;

    fn entry(name: &str, kind: Kind, allocated: u64) -> RawEntry {
        // File IDs distincts : un même file ID serait compté comme un lien physique.
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let file_id = u128::from(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        RawEntry { name: name.into(), kind, attributes: 0, file_id, logical: allocated, allocated, modified: 0, accessed: 0 }
    }

    /// Profil : 6 niveaux de dossiers, un gros fichier à la racine, un petit dossier.
    pub(crate) fn sample() -> Scan {
        let mut b = scan::model::Builder::new(&entry("Arthur", Kind::Dir, 0));
        let top = b.add_children(0, &[entry("AppData", Kind::Dir, 0), entry("video.mp4", Kind::File, 900 * MIB), entry("petit", Kind::Dir, 0)]);
        let appdata = top.start;
        let petit = top.start + 2;
        b.add_children(petit, &[entry("note.txt", Kind::File, 4096)]);
        let mut parent = appdata;
        for name in ["Local", "Pkg", "Cache", "Deep", "Deeper"] {
            let r = b.add_children(parent, &[entry(name, Kind::Dir, 0), entry(&format!("{name}.bin"), Kind::File, 10 * MIB)]);
            parent = r.start;
        }
        b.add_children(parent, &[entry("bottom.bin", Kind::File, 5 * MIB)]);
        b.finish("C:\\Users\\Arthur".into(), 7, Method::Walk, true, Stats::default())
    }

    #[test]
    fn dossiers_seulement_sur_quatre_niveaux() {
        let t = from_scan(&sample());
        let paths: Vec<&str> = t.rows.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(paths, ["AppData", "AppData\\Local", "AppData\\Local\\Pkg", "AppData\\Local\\Pkg\\Cache"]);
        // Jamais un fichier, jamais un dossier de moins de 1 Mio, jamais au-delà de 4 niveaux ;
        // les niveaux plus profonds restent comptés dans leur parent.
        assert_eq!(t.rows[3].allocated, 10 * MIB * 2 + 5 * MIB);
        assert_eq!(t.rows[0].allocated, 10 * MIB * 5 + 5 * MIB);
        assert_eq!(t.used, 900 * MIB + 55 * MIB + 4096);
        assert_eq!(t.min_bytes, MIN_BYTES);
    }
}
