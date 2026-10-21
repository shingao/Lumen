//! Doublons : fichiers au contenu identique. C'est la seule fonctionnalité de Lumen qui lit le
//! contenu des fichiers (par `safety::win::content` : dernier accès gelé, E/S basse priorité).
//!
//! 1. Candidats de l'analyse : fichiers ordinaires et locaux d'au moins N Mo, hors applis,
//!    système, projets git et denylist. Un fichier cloud n'est jamais lu (ni rapatrié).
//! 2. Liens physiques : un même fichier (même file ID) n'est compté qu'une fois ; ses autres noms
//!    ne sont jamais des doublons (en supprimer un ne libère rien).
//! 3. Même taille → mêmes bords (64 Kio au début et à la fin) → même contenu complet (BLAKE3).
//! 4. Les empreintes sont en cache par file ID + taille + date de modification.
//! 5. Un exemplaire à garder est présélectionné (le plus ancien) ; `safety` en garde toujours au
//!    moins un et recalcule l'empreinte juste avant la suppression.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use safety::denylist::DenyList;
use scan::model::F_HARDLINK_DUP;
use scan::{Kind, NodeId, Scan};

use super::files::{denied, plain_file, row, FileRowDto, Zones};

/// Groupes renvoyés à l'interface, au plus (les totaux restent exacts).
const MAX_GROUPS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))] // lus seulement sous Windows (et dans les tests)
pub enum ReadError {
    /// Fichier dans le cloud : jamais lu.
    Cloud,
    /// Date de dernier accès non gelable : non lu.
    NotFrozen,
    Cancelled,
    Other(String),
}

/// Lecture du contenu (réelle : `safety::win::content` ; tests : en mémoire).
pub trait ContentReader {
    fn edges(&self, path: &str, cancel: &AtomicBool) -> Result<[u8; 32], ReadError>;
    fn full(&self, path: &str, cancel: &AtomicBool) -> Result<[u8; 32], ReadError>;
}

/// Clé du cache : un fichier précis, dans un état précis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CacheKey {
    pub volume: u64,
    pub file_id: u128,
    pub size: u64,
    pub modified: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheEntry {
    pub edges: Option<[u8; 32]>,
    pub full: Option<[u8; 32]>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressDto {
    /// `edges` puis `full`.
    pub phase: &'static str,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatsDto {
    pub candidates: usize,
    /// Fichiers lus (bords ou complets), et empreintes reprises du cache.
    pub read: usize,
    pub cached: usize,
    pub skipped_cloud: usize,
    pub skipped_not_frozen: usize,
    pub errors: usize,
    /// Noms supplémentaires d'un même fichier (liens physiques), jamais comptés comme doublons.
    pub hard_links: usize,
}

#[derive(Debug, Clone)]
pub struct Group {
    pub hash: [u8; 32],
    /// Taille logique d'un exemplaire.
    pub size: u64,
    /// Un nœud par fichier distinct (file ID), l'exemplaire à garder en premier.
    pub members: Vec<NodeId>,
}

pub struct Found {
    pub groups: Vec<Group>,
    pub stats: StatsDto,
    /// Empreintes nouvelles ou complétées, à écrire dans le cache.
    pub cache: Vec<(CacheKey, CacheEntry)>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupDto {
    pub id: usize,
    pub size: u64,
    /// Ce que libérerait la suppression de tous les exemplaires sauf un.
    pub wasted: u64,
    pub members: Vec<FileRowDto>,
    /// Exemplaire présélectionné pour être gardé.
    pub keep: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicatesDto {
    pub groups: Vec<GroupDto>,
    pub more_groups: usize,
    pub wasted: u64,
    pub stats: StatsDto,
}

pub fn key(scan: &Scan, id: NodeId) -> CacheKey {
    let n = scan.node(id);
    CacheKey { volume: scan.volume_serial, file_id: scan.file_id(id), size: n.map_or(0, |n| n.logical), modified: n.map_or(0, |n| n.modified) }
}

pub struct Inputs<'a, R: ContentReader> {
    pub scan: &'a Scan,
    pub deny: Option<&'a DenyList>,
    pub min_size: u64,
    pub reader: &'a R,
    pub cache: &'a HashMap<CacheKey, CacheEntry>,
    pub cancel: &'a AtomicBool,
}

fn group_by<K: std::hash::Hash + Eq>(ids: &[NodeId], mut key: impl FnMut(NodeId) -> Option<K>) -> Vec<Vec<NodeId>> {
    let mut map: HashMap<K, Vec<NodeId>> = HashMap::new();
    for &id in ids {
        if let Some(k) = key(id) {
            map.entry(k).or_default().push(id);
        }
    }
    map.into_values().filter(|g| g.len() >= 2).collect()
}

/// Recherche annulable ; `on_progress` est appelé à chaque fichier lu.
pub fn find<R: ContentReader>(i: &Inputs<'_, R>, mut on_progress: impl FnMut(ProgressDto)) -> Result<Found, ReadError> {
    let scan = i.scan;
    let zones = Zones::new(scan);
    let mut stats = StatsDto::default();

    // 1-2. Candidats, un nœud par fichier distinct.
    let mut seen: HashMap<(u64, u128), NodeId> = HashMap::new();
    let mut candidates = Vec::new();
    for id in 0..scan.len() as NodeId {
        if scan.node(id).is_some_and(|n| n.kind == Kind::File && n.flags & F_HARDLINK_DUP != 0) {
            // Autre nom d'un fichier déjà compté (marqué par l'analyse).
            stats.hard_links += 1;
            continue;
        }
        if !plain_file(scan, id) || zones.in_app(id) || zones.in_git(id) {
            continue;
        }
        let Some(n) = scan.node(id) else { continue };
        if n.logical < i.min_size || n.logical == 0 || denied(i.deny, scan, id) {
            continue;
        }
        let fid = (scan.volume_serial, scan.file_id(id));
        if fid.1 != 0 && seen.insert(fid, id).is_some() {
            stats.hard_links += 1;
            continue;
        }
        candidates.push(id);
    }
    stats.candidates = candidates.len();
    let size_of = |id: NodeId| scan.node(id).map_or(0, |n| n.logical);

    // 3. Même taille, puis mêmes bords, puis même contenu.
    let by_size = group_by(&candidates, |id| Some(size_of(id)));
    let mut entries: HashMap<CacheKey, CacheEntry> = HashMap::new();
    let mut digest = |id: NodeId, full: bool, stats: &mut StatsDto| -> Result<Option<[u8; 32]>, ReadError> {
        let k = key(scan, id);
        let cached = i.cache.get(&k).copied().unwrap_or_default();
        let entry = entries.entry(k).or_insert(cached);
        let known = if full { entry.full } else { entry.edges };
        if let Some(h) = known {
            stats.cached += 1;
            return Ok(Some(h));
        }
        let path = scan.path(id);
        let result = if full { i.reader.full(&path, i.cancel) } else { i.reader.edges(&path, i.cancel) };
        match result {
            Ok(h) => {
                stats.read += 1;
                if full {
                    entry.full = Some(h);
                } else {
                    entry.edges = Some(h);
                }
                Ok(Some(h))
            }
            Err(ReadError::Cancelled) => Err(ReadError::Cancelled),
            Err(ReadError::Cloud) => {
                stats.skipped_cloud += 1;
                Ok(None)
            }
            Err(ReadError::NotFrozen) => {
                stats.skipped_not_frozen += 1;
                Ok(None)
            }
            Err(ReadError::Other(_)) => {
                stats.errors += 1;
                Ok(None)
            }
        }
    };

    let total: usize = by_size.iter().map(Vec::len).sum();
    let mut done = 0;
    let mut by_edges = Vec::new();
    for group in &by_size {
        let mut hashed = Vec::new();
        for &id in group {
            if i.cancel.load(Ordering::SeqCst) {
                return Err(ReadError::Cancelled);
            }
            if let Some(h) = digest(id, false, &mut stats)? {
                hashed.push((id, h));
            }
            done += 1;
            on_progress(ProgressDto { phase: "edges", done, total });
        }
        by_edges.extend(group_by(&hashed.iter().map(|(id, _)| *id).collect::<Vec<_>>(), |id| hashed.iter().find(|(x, _)| *x == id).map(|(_, h)| *h)));
    }

    let total: usize = by_edges.iter().map(Vec::len).sum();
    let mut done = 0;
    let mut groups = Vec::new();
    for group in &by_edges {
        let mut hashed: Vec<(NodeId, [u8; 32])> = Vec::new();
        for &id in group {
            if i.cancel.load(Ordering::SeqCst) {
                return Err(ReadError::Cancelled);
            }
            if let Some(h) = digest(id, true, &mut stats)? {
                hashed.push((id, h));
            }
            done += 1;
            on_progress(ProgressDto { phase: "full", done, total });
        }
        let mut by_hash: HashMap<[u8; 32], Vec<NodeId>> = HashMap::new();
        for (id, h) in hashed {
            by_hash.entry(h).or_default().push(id);
        }
        for (hash, mut members) in by_hash.into_iter().filter(|(_, m)| m.len() >= 2) {
            // 5. Exemplaire à garder : le plus ancien, puis le chemin le plus court.
            members.sort_by_key(|&id| (scan.node(id).map_or(i64::MAX, |n| n.modified), scan.path(id).len(), id));
            groups.push(Group { hash, size: size_of(members[0]), members });
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.size * (g.members.len() as u64 - 1)));
    Ok(Found { groups, stats, cache: entries.into_iter().collect() })
}

pub fn dto(scan: &Scan, found: &Found, display: &dyn Fn(&str) -> String) -> DuplicatesDto {
    let zones = Zones::new(scan);
    DuplicatesDto {
        groups: found
            .groups
            .iter()
            .enumerate()
            .take(MAX_GROUPS)
            .map(|(gi, g)| GroupDto {
                id: gi,
                size: g.size,
                wasted: g.size * (g.members.len() as u64 - 1),
                members: g.members.iter().map(|&id| row(scan, id, &zones, display)).collect(),
                keep: i64::from(g.members[0]),
            })
            .collect(),
        more_groups: found.groups.len().saturating_sub(MAX_GROUPS),
        wasted: found.groups.iter().map(|g| g.size * (g.members.len() as u64 - 1)).sum(),
        stats: found.stats.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scan::model::Builder;
    use scan::{Method, RawEntry, Stats};
    use std::cell::RefCell;

    /// Contenus par chemin ; compte les lectures.
    struct Mem {
        content: HashMap<String, Vec<u8>>,
        reads: RefCell<Vec<String>>,
        cloud: Vec<String>,
    }

    impl ContentReader for Mem {
        fn edges(&self, path: &str, _: &AtomicBool) -> Result<[u8; 32], ReadError> {
            self.full(path, &AtomicBool::new(false)).map(|mut h| {
                h[0] ^= 0xFF; // empreinte différente de la complète, mais égale pour un même contenu
                h
            })
        }
        fn full(&self, path: &str, _: &AtomicBool) -> Result<[u8; 32], ReadError> {
            if self.cloud.iter().any(|c| path.ends_with(c.as_str())) {
                return Err(ReadError::Cloud);
            }
            self.reads.borrow_mut().push(path.to_owned());
            let c = self.content.get(path).ok_or_else(|| ReadError::Other("absent".into()))?;
            // Empreinte factice : longueur et premiers octets suffisent aux contenus des tests.
            let mut h = [0u8; 32];
            h[..8].copy_from_slice(&(c.len() as u64).to_le_bytes());
            for (d, b) in h[8..].iter_mut().zip(c) {
                *d = *b;
            }
            Ok(h)
        }
    }

    fn e(name: &str, kind: Kind, size: u64, id: u128, modified: i64) -> RawEntry {
        RawEntry { name: name.into(), kind, attributes: 0, file_id: id, logical: size, allocated: size, modified, accessed: modified }
    }

    const MB: u64 = 1 << 20;

    /// Trois copies d'une vidéo (une plus ancienne), un lien physique d'une d'elles, un fichier
    /// de même taille au contenu différent, un fichier cloud de même taille, une copie dans AppData.
    fn world() -> (Scan, Mem) {
        let mut b = Builder::new(&e("Arthur", Kind::Dir, 0, 1, 0));
        let top = b.add_children(
            0,
            &[e("Videos", Kind::Dir, 0, 2, 0), e("Bureau", Kind::Dir, 0, 3, 0), e("AppData", Kind::Dir, 0, 4, 0)],
        );
        b.add_children(
            top.start,
            &[
                e("film.mp4", Kind::File, 5 * MB, 10, 100),
                e("film (1).mp4", Kind::File, 5 * MB, 11, 300),
                e("autre.mp4", Kind::File, 5 * MB, 12, 50),
                e("lien vers film.mp4", Kind::File, 5 * MB, 10, 100),
                e("cloud.mp4", Kind::CloudFile, 5 * MB, 13, 10),
            ],
        );
        b.add_children(top.start + 1, &[e("film - copie.mp4", Kind::File, 5 * MB, 14, 200)]);
        b.add_children(top.start + 2, &[e("cache-film.mp4", Kind::File, 5 * MB, 15, 1)]);
        let scan = b.finish("C:\\Users\\Arthur".into(), 7, Method::Walk, true, Stats::default());
        let same = vec![1u8; 64];
        let mut content = HashMap::new();
        for p in ["Videos\\film.mp4", "Videos\\film (1).mp4", "Videos\\lien vers film.mp4", "Bureau\\film - copie.mp4", "AppData\\cache-film.mp4"] {
            content.insert(format!("C:\\Users\\Arthur\\{p}"), same.clone());
        }
        content.insert("C:\\Users\\Arthur\\Videos\\autre.mp4".into(), vec![2u8; 64]);
        (scan, Mem { content, reads: RefCell::default(), cloud: vec!["cloud.mp4".into()] })
    }

    fn run(scan: &Scan, mem: &Mem, cache: &HashMap<CacheKey, CacheEntry>) -> Found {
        find(&Inputs { scan, deny: None, min_size: MB, reader: mem, cache, cancel: &AtomicBool::new(false) }, |_| {}).unwrap()
    }

    fn names(scan: &Scan, g: &Group) -> Vec<String> {
        g.members.iter().map(|&id| scan.name(id).to_owned()).collect()
    }

    #[test]
    fn copies_trouvees_exemplaire_le_plus_ancien_garde() {
        let (scan, mem) = world();
        let f = run(&scan, &mem, &HashMap::new());
        assert_eq!(f.groups.len(), 1);
        assert_eq!(names(&scan, &f.groups[0]), ["film.mp4", "film - copie.mp4", "film (1).mp4"]);
        let d = dto(&scan, &f, &|p| p.to_owned());
        assert_eq!(d.wasted, 10 * MB);
        assert_eq!(d.groups[0].keep, i64::from(f.groups[0].members[0]));
    }

    #[test]
    fn liens_physiques_jamais_des_doublons() {
        let (scan, mem) = world();
        let f = run(&scan, &mem, &HashMap::new());
        // « lien vers film.mp4 » a le même file ID que « film.mp4 » : un seul fichier.
        assert!(!names(&scan, &f.groups[0]).contains(&"lien vers film.mp4".to_owned()));
        assert_eq!(f.stats.hard_links, 1);
        assert!(!mem.reads.borrow().iter().any(|p| p.ends_with("lien vers film.mp4")));
    }

    #[test]
    fn cloud_applis_jamais_lus() {
        let (scan, mem) = world();
        let f = run(&scan, &mem, &HashMap::new());
        let reads = mem.reads.borrow();
        assert!(!reads.iter().any(|p| p.contains("cloud.mp4")), "fichier cloud jamais lu");
        assert!(!reads.iter().any(|p| p.contains("AppData")), "dossier d'appli exclu");
        assert_eq!(f.stats.skipped_cloud, 0, "le fichier cloud n'est même pas candidat");
    }

    #[test]
    fn cache_par_file_id_taille_et_date() {
        let (scan, mem) = world();
        let first = run(&scan, &mem, &HashMap::new());
        let reads_first = mem.reads.borrow().len();
        assert!(reads_first > 0);
        let cache: HashMap<CacheKey, CacheEntry> = first.cache.into_iter().collect();
        mem.reads.borrow_mut().clear();
        let second = run(&scan, &mem, &cache);
        assert!(mem.reads.borrow().is_empty(), "rien n'est relu : {:?}", mem.reads.borrow());
        assert_eq!(second.groups.len(), 1);
        // Date de modification différente : la clé ne correspond plus, le fichier est relu.
        let film = scan.find("Videos\\film.mp4").unwrap();
        let mut stale = cache.clone();
        let k = key(&scan, film);
        let v = stale.remove(&k).unwrap();
        stale.insert(CacheKey { modified: k.modified + 1, ..k }, v);
        mem.reads.borrow_mut().clear();
        run(&scan, &mem, &stale);
        assert!(mem.reads.borrow().iter().any(|p| p.ends_with("Videos\\film.mp4")));
    }

    #[test]
    fn annulable() {
        let (scan, mem) = world();
        let cancel = AtomicBool::new(true);
        let r = find(&Inputs { scan: &scan, deny: None, min_size: MB, reader: &mem, cache: &HashMap::new(), cancel: &cancel }, |_| {});
        assert!(matches!(r, Err(ReadError::Cancelled)));
    }
}
