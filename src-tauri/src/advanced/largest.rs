//! Les 100 plus gros fichiers de l'analyse, filtrables par type. Toujours « À vérifier » : la
//! taille ne dit rien de l'utilité. Les fichiers protégés (denylist, système, cachés) et les
//! fichiers cloud n'y figurent pas.

use std::cmp::Reverse;

use serde::Serialize;

use safety::denylist::DenyList;
use scan::{NodeId, Scan};

use super::files::{denied, plain_file, row, FileKind, FileRowDto, Zones};

pub const TOP: usize = 100;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LargestDto {
    pub files: Vec<FileRowDto>,
    /// Nombre de fichiers par type parmi les 100 plus gros (tous types), pour le filtre.
    pub kinds: Vec<(FileKind, usize)>,
}

pub struct Largest {
    pub dto: LargestDto,
    pub eligible: Vec<NodeId>,
}

pub fn find(scan: &Scan, deny: Option<&DenyList>, kind: Option<FileKind>, display: &dyn Fn(&str) -> String) -> Largest {
    let zones = Zones::new(scan);
    let size = |id: NodeId| scan.node(id).map_or(0, |n| n.allocated);
    // Tous types : les 100 plus gros (plus quelques-uns pour remplacer les fichiers protégés).
    let mut all: Vec<NodeId> = (0..scan.len() as NodeId).filter(|&id| plain_file(scan, id)).collect();
    all.sort_unstable_by_key(|&id| Reverse(size(id)));
    let mut top: Vec<NodeId> = Vec::new();
    let mut by_kind: Vec<(FileKind, usize)> = Vec::new();
    for &id in &all {
        if top.len() >= TOP && by_kind.iter().map(|(_, n)| n).sum::<usize>() >= TOP {
            break;
        }
        if denied(deny, scan, id) {
            continue;
        }
        let k = FileKind::of(scan.name(id));
        if by_kind.iter().map(|(_, n)| n).sum::<usize>() < TOP {
            match by_kind.iter_mut().find(|(kk, _)| *kk == k) {
                Some((_, n)) => *n += 1,
                None => by_kind.push((k, 1)),
            }
        }
        if top.len() < TOP && (kind.is_none() || kind == Some(k)) {
            top.push(id);
        }
    }
    by_kind.sort_by_key(|(_, n)| Reverse(*n));
    Largest { dto: LargestDto { files: top.iter().map(|&id| row(scan, id, &zones, display)).collect(), kinds: by_kind }, eligible: top }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scan::model::Builder;
    use scan::{Kind, Method, RawEntry, Stats};

    fn e(name: &str, kind: Kind, size: u64) -> RawEntry {
        RawEntry { name: name.into(), kind, attributes: 0, file_id: 0, logical: size, allocated: size, modified: 1, accessed: 1 }
    }

    #[test]
    fn top_par_taille_et_par_type() {
        let mut b = Builder::new(&e("D:", Kind::Dir, 0));
        let mut files: Vec<RawEntry> = (0..150).map(|i| e(&format!("clip{i:03}.mp4"), Kind::File, 1000 + i)).collect();
        files.push(e("ubuntu.iso", Kind::File, 5_000_000));
        files.push(e("systeme.sys", Kind::File, 9_000_000));
        let mut hidden = e("cache.bin", Kind::File, 8_000_000);
        hidden.attributes = 0x2;
        files.push(hidden);
        b.add_children(0, &files);
        let scan = b.finish("D:\\".into(), 1, Method::Walk, false, Stats::default());
        let all = find(&scan, None, None, &|p| p.to_owned());
        assert_eq!(all.dto.files.len(), TOP);
        assert_eq!(all.dto.files[0].name, "systeme.sys");
        assert_eq!(all.dto.files[1].name, "ubuntu.iso");
        assert!(!all.dto.files.iter().any(|f| f.name == "cache.bin"), "caché : exclu");
        let disks = find(&scan, None, Some(FileKind::Disk), &|p| p.to_owned());
        assert_eq!(disks.dto.files.len(), 1);
        assert_eq!(disks.eligible.len(), 1);
        assert_eq!(all.dto.kinds[0], (FileKind::Video, 98));
    }
}
