//! Fichiers dormants : pas ouverts (ou pas modifiés) depuis N mois. Toujours « À vérifier ».
//!
//! La date de dernier accès n'est utilisée que si Windows la tient à jour
//! (`NtfsDisableLastAccessUpdate`, ce que lit `fsutil behavior query disablelastaccess`) et si
//! l'analyse l'a jugée fiable ; sinon on bascule sur la date de modification et l'interface le dit.
//! Exclus : dossiers d'applis et système, projets git, fichiers système ou cachés, fichiers
//! cloud, éléments déjà couverts par une recommandation.

use std::collections::HashMap;

use serde::Serialize;

use safety::denylist::DenyList;
use scan::{NodeId, Scan};

use super::files::{denied, plain_file, row, FileRowDto, Zones};

/// En dessous, un fichier dormant ne vaut pas la peine d'être proposé.
pub const MIN_SIZE: u64 = 1 << 20;
/// Dossiers et fichiers renvoyés, au plus (les totaux restent exacts).
const MAX_FOLDERS: usize = 200;
const MAX_FILES_PER_FOLDER: usize = 100;
const TICKS_PER_DAY: i64 = 86_400 * 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Basis {
    /// Dernier accès (tenu à jour par Windows).
    LastAccess,
    /// Dernière modification : le dernier accès n'est pas fiable sur ce PC.
    LastWrite,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DormantFolderDto {
    pub path: String,
    pub size: u64,
    pub count: usize,
    pub files: Vec<FileRowDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DormantDto {
    pub months: u32,
    pub basis: Basis,
    /// Valeur brute de `NtfsDisableLastAccessUpdate` (`fsutil behavior query disablelastaccess`).
    pub last_access_raw: Option<u32>,
    pub total: u64,
    pub count: usize,
    pub folders: Vec<DormantFolderDto>,
}

pub struct Dormant {
    pub dto: DormantDto,
    /// Nœuds proposés : seuls ceux-là peuvent entrer dans une sélection.
    pub eligible: Vec<NodeId>,
}

pub struct Inputs<'a> {
    pub scan: &'a Scan,
    pub deny: Option<&'a DenyList>,
    /// FILETIME actuel.
    pub now: i64,
    pub months: u32,
    /// Windows tient-il à jour le dernier accès ? (registre)
    pub last_access_updated: bool,
    pub last_access_raw: Option<u32>,
    /// Élément déjà couvert par une recommandation.
    pub covered: &'a dyn Fn(NodeId) -> bool,
    pub display: &'a dyn Fn(&str) -> String,
}

pub fn find(i: &Inputs<'_>) -> Dormant {
    let scan = i.scan;
    let basis = if i.last_access_updated && scan.last_access_reliable { Basis::LastAccess } else { Basis::LastWrite };
    let threshold = i.now - (f64::from(i.months) * 30.4375) as i64 * TICKS_PER_DAY;
    let zones = Zones::new(scan);
    let mut by_folder: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    let mut eligible = Vec::new();
    for id in 0..scan.len() as NodeId {
        if !plain_file(scan, id) || zones.in_app(id) || zones.in_git(id) || (i.covered)(id) {
            continue;
        }
        let Some(n) = scan.node(id) else { continue };
        if n.allocated < MIN_SIZE {
            continue;
        }
        let used = match basis {
            Basis::LastAccess => n.accessed.max(n.modified),
            Basis::LastWrite => n.modified,
        };
        if used <= 0 || used > threshold || denied(i.deny, scan, id) {
            continue;
        }
        eligible.push(id);
        by_folder.entry(n.parent).or_default().push(id);
    }
    let size = |id: &NodeId| scan.node(*id).map_or(0, |n| n.allocated);
    let mut folders: Vec<(NodeId, Vec<NodeId>)> = by_folder.into_iter().collect();
    for (_, files) in &mut folders {
        files.sort_by_key(|f| std::cmp::Reverse(size(f)));
    }
    folders.sort_by_key(|(_, files)| std::cmp::Reverse(files.iter().map(size).sum::<u64>()));
    let total = eligible.iter().map(size).sum();
    let dto = DormantDto {
        months: i.months,
        basis,
        last_access_raw: i.last_access_raw,
        total,
        count: eligible.len(),
        folders: folders
            .iter()
            .take(MAX_FOLDERS)
            .map(|(dir, files)| DormantFolderDto {
                path: (i.display)(&scan.path(*dir)),
                size: files.iter().map(size).sum(),
                count: files.len(),
                files: files.iter().take(MAX_FILES_PER_FOLDER).map(|&f| row(scan, f, &zones, i.display)).collect(),
            })
            .collect(),
    };
    Dormant { dto, eligible }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scan::model::Builder;
    use scan::{Kind, Method, RawEntry, Stats};

    const DAY: i64 = TICKS_PER_DAY;
    const NOW: i64 = 133_700_000_000_000_000;

    fn e(name: &str, kind: Kind, size: u64, modified: i64, accessed: i64) -> RawEntry {
        RawEntry { name: name.into(), kind, attributes: 0, file_id: 0, logical: size, allocated: size, modified, accessed }
    }

    /// Profil : un vieux film jamais rouvert, un vieux document rouvert hier, un projet git, une
    /// appli dans AppData, un petit fichier.
    fn profile(reliable: bool) -> Scan {
        let old = NOW - 400 * DAY;
        let mut b = Builder::new(&e("Arthur", Kind::Dir, 0, old, old));
        let top = b.add_children(
            0,
            &[
                e("Videos", Kind::Dir, 0, old, old),
                e("Documents", Kind::Dir, 0, old, old),
                e("Projets", Kind::Dir, 0, old, old),
                e("AppData", Kind::Dir, 0, old, old),
            ],
        );
        b.add_children(top.start, &[e("vacances.mp4", Kind::File, 900 << 20, old, old)]);
        b.add_children(top.start + 1, &[e("rapport.pdf", Kind::File, 5 << 20, old, NOW - DAY), e("petit.txt", Kind::File, 10, old, old)]);
        let p = b.add_children(top.start + 2, &[e("site", Kind::Dir, 0, old, old)]);
        b.add_children(p.start, &[e(".git", Kind::Dir, 0, old, old), e("logo.psd", Kind::File, 50 << 20, old, old)]);
        b.add_children(top.start + 3, &[e("vieux.bin", Kind::File, 80 << 20, old, old)]);
        b.finish("C:\\Users\\Arthur".into(), 1, Method::Walk, reliable, Stats::default())
    }

    fn names(scan: &Scan, d: &Dormant) -> Vec<String> {
        d.eligible.iter().map(|&id| scan.name(id).to_owned()).collect()
    }

    fn run(scan: &Scan, updated: bool) -> Dormant {
        find(&Inputs {
            scan,
            deny: None,
            now: NOW,
            months: 6,
            last_access_updated: updated,
            last_access_raw: Some(if updated { 0x8000_0002 } else { 0x8000_0003 }),
            covered: &|_| false,
            display: &|p| p.to_owned(),
        })
    }

    #[test]
    fn dernier_acces_fiable_seulement_si_windows_le_tient() {
        let scan = profile(true);
        let d = run(&scan, true);
        assert_eq!(d.dto.basis, Basis::LastAccess);
        // Le document rouvert hier n'est pas dormant ; le film l'est.
        assert_eq!(names(&scan, &d), ["vacances.mp4"]);

        // Mise à jour du dernier accès désactivée : bascule sur la date de modification, signalée.
        let d = run(&scan, false);
        assert_eq!(d.dto.basis, Basis::LastWrite);
        assert_eq!(d.dto.last_access_raw, Some(0x8000_0003));
        let mut n = names(&scan, &d);
        n.sort();
        assert_eq!(n, ["rapport.pdf", "vacances.mp4"]);
        // Analyse qui jugeait le dernier accès non fiable : même bascule.
        assert_eq!(run(&profile(false), true).dto.basis, Basis::LastWrite);
    }

    #[test]
    fn jamais_applis_projets_git_ni_petits_fichiers() {
        let scan = profile(true);
        let d = run(&scan, false);
        let n = names(&scan, &d);
        assert!(!n.contains(&"logo.psd".to_owned()), "projet git exclu");
        assert!(!n.contains(&"vieux.bin".to_owned()), "AppData exclu");
        assert!(!n.contains(&"petit.txt".to_owned()), "moins de 1 Mio");
        // Regroupés par dossier, le plus gros d'abord.
        assert!(d.dto.folders[0].path.ends_with("Videos"));
        assert_eq!(d.dto.total, (900 << 20) + (5 << 20));
    }

    #[test]
    fn deja_couverts_par_une_recommandation_exclus() {
        let scan = profile(true);
        let film = scan.find("Videos\\vacances.mp4").unwrap();
        let d = find(&Inputs {
            scan: &scan,
            deny: None,
            now: NOW,
            months: 6,
            last_access_updated: true,
            last_access_raw: None,
            covered: &|id| id == film,
            display: &|p| p.to_owned(),
        });
        assert!(d.eligible.is_empty());
    }
}
