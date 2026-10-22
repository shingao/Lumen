//! Résultat d'un scan : une arène compacte de nœuds (≈ 72 octets chacun + leur nom), construite
//! par le parcours ou par la lecture de la MFT, puis dédoublonnée (liens physiques) et agrégée.
//! Le front n'en reçoit jamais qu'une tranche agrégée (voir [`Scan::slice`]).

use std::collections::HashMap;
use std::ops::Range;

pub type NodeId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Kind {
    Dir = 0,
    File = 1,
    /// Point d'analyse non traversé (jonction, lien symbolique, alias d'appli) : compté à part.
    Link = 2,
    /// Fichier dans le cloud, non téléchargé (OneDrive…) : jamais rapatrié, hors taille sur disque.
    CloudFile = 3,
    /// Dossier cloud pas encore peuplé localement : jamais énuméré (cela le téléchargerait).
    CloudDir = 4,
}

/// Ce que la source (parcours ou MFT) sait d'une entrée, sans jamais en lire le contenu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawEntry {
    pub name: String,
    pub kind: Kind,
    pub attributes: u32,
    /// File ID (NTFS : FRN 64 bits ; ReFS : 128 bits ; Linux : inode).
    pub file_id: u128,
    /// Taille logique (fin de fichier).
    pub logical: u64,
    /// Taille allouée sur le disque (compression, fichiers clairsemés, petits fichiers résidents).
    pub allocated: u64,
    /// Dernière écriture, en FILETIME (100 ns depuis 1601) : identique à ce que lit `safety`.
    pub modified: i64,
    /// Dernier accès, en FILETIME.
    pub accessed: i64,
}

/// Le nœud porte un lien physique déjà compté ailleurs : 0 octet, 0 fichier.
pub const F_HARDLINK_DUP: u8 = 1;
/// Le dossier n'a pas pu être lu (accès refusé, verrou…).
pub const F_UNREADABLE: u8 = 2;

#[derive(Debug, Clone)]
pub struct Node {
    pub parent: NodeId,
    pub first_child: NodeId,
    pub child_count: u32,
    name_off: u32,
    name_len: u16,
    pub kind: Kind,
    pub flags: u8,
    pub attributes: u32,
    /// 64 bits bas du file ID (les 64 bits hauts, rares, sont dans `Scan::id_high`).
    pub file_id: u64,
    /// Fichier : taille logique. Dossier : somme de son contenu (hors cloud et doublons).
    pub logical: u64,
    /// Fichier : taille allouée. Dossier : somme de son contenu (hors cloud et doublons).
    pub allocated: u64,
    pub modified: i64,
    /// Fichier : dernier accès. Dossier : dernière utilisation la plus récente de son contenu.
    pub accessed: i64,
    /// Nombre de fichiers comptés sous ce nœud (1 pour un fichier).
    pub files: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Walk,
    Mft,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub files: u64,
    pub dirs: u64,
    /// Points d'analyse non traversés.
    pub links: u64,
    pub cloud_files: u64,
    pub cloud_dirs: u64,
    /// Taille logique des fichiers restés dans le cloud.
    pub cloud_bytes: u64,
    /// Liens physiques supplémentaires (fichiers comptés une seule fois).
    pub hardlink_dups: u64,
    pub access_denied: u64,
    pub locked: u64,
    pub other_errors: u64,
    pub allocated: u64,
    pub logical: u64,
    pub elapsed_ms: u64,
    /// Premiers chemins en erreur, pour l'affichage.
    pub error_samples: Vec<String>,
}

impl Stats {
    pub fn errors(&self) -> u64 {
        self.access_denied + self.locked + self.other_errors
    }
}

#[derive(Debug, Clone)]
pub struct Scan {
    pub root_path: String,
    pub volume_serial: u64,
    pub method: Method,
    pub last_access_reliable: bool,
    pub nodes: Vec<Node>,
    names: String,
    id_high: HashMap<NodeId, u64>,
    pub stats: Stats,
}

/// Construction de l'arène : les enfants d'un dossier sont toujours contigus et d'indices
/// supérieurs à leur parent (ce qui permet d'agréger en un seul passage à rebours).
#[derive(Debug, Default)]
pub struct Builder {
    nodes: Vec<Node>,
    names: String,
    id_high: HashMap<NodeId, u64>,
}

impl Builder {
    pub fn new(root: &RawEntry) -> Builder {
        let mut b = Builder::default();
        b.push(root, 0);
        b
    }

    fn push(&mut self, e: &RawEntry, parent: NodeId) -> NodeId {
        let id = self.nodes.len() as NodeId;
        // Les noms NTFS font au plus 255 caractères UTF-16, soit au plus 765 octets UTF-8.
        let name = if e.name.len() > u16::MAX as usize { &e.name[..e.name.floor_char_boundary_compat(u16::MAX as usize)] } else { &e.name[..] };
        let name_off = self.names.len() as u32;
        self.names.push_str(name);
        let high = (e.file_id >> 64) as u64;
        if high != 0 {
            self.id_high.insert(id, high);
        }
        self.nodes.push(Node {
            parent,
            first_child: 0,
            child_count: 0,
            name_off,
            name_len: name.len() as u16,
            kind: e.kind,
            flags: 0,
            attributes: e.attributes,
            file_id: e.file_id as u64,
            logical: e.logical,
            allocated: e.allocated,
            modified: e.modified,
            accessed: e.accessed,
            files: 0,
        });
        id
    }

    /// Ajoute les entrées d'un dossier ; renvoie leurs identifiants (contigus).
    pub fn add_children(&mut self, parent: NodeId, entries: &[RawEntry]) -> Range<NodeId> {
        let start = self.nodes.len() as NodeId;
        for e in entries {
            self.push(e, parent);
        }
        let end = self.nodes.len() as NodeId;
        if let Some(p) = self.nodes.get_mut(parent as usize) {
            p.first_child = start;
            p.child_count = end - start;
        }
        start..end
    }

    pub fn mark_unreadable(&mut self, id: NodeId) {
        if let Some(n) = self.nodes.get_mut(id as usize) {
            n.flags |= F_UNREADABLE;
        }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Dédoublonne les liens physiques puis agrège les tailles du bas vers le haut.
    pub fn finish(self, root_path: String, volume_serial: u64, method: Method, last_access_reliable: bool, mut stats: Stats) -> Scan {
        let mut scan = Scan {
            root_path,
            volume_serial,
            method,
            last_access_reliable,
            nodes: self.nodes,
            names: self.names,
            id_high: self.id_high,
            stats: Stats::default(),
        };
        scan.dedupe_hard_links(&mut stats);
        scan.aggregate(&mut stats);
        scan.stats = stats;
        scan
    }
}

/// `str::floor_char_boundary` n'est pas encore stable partout : équivalent local.
trait FloorBoundary {
    fn floor_char_boundary_compat(&self, index: usize) -> usize;
}

impl FloorBoundary for str {
    fn floor_char_boundary_compat(&self, index: usize) -> usize {
        let mut i = index.min(self.len());
        while i > 0 && !self.is_char_boundary(i) {
            i -= 1;
        }
        i
    }
}

/// Élément d'une tranche envoyée au front.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SliceNode {
    /// Identifiant du nœud ; pour un regroupement « autres », −(parent + 1).
    pub id: i64,
    pub name: String,
    pub path: String,
    pub kind: Kind,
    pub allocated: u64,
    pub logical: u64,
    pub files: u64,
    pub last_used: i64,
    pub file_id: u128,
    pub modified: i64,
    /// Regroupement des plus petits éléments : nombre d'éléments regroupés.
    pub others: Option<u32>,
    /// Regroupement : les éléments regroupés (leur statut fait celui du regroupement).
    pub members: Vec<NodeId>,
    pub has_children: bool,
    pub children: Vec<SliceNode>,
}

impl Scan {
    /// Mémoire occupée (octets, estimation par excès) : nœuds, noms, file ID longs.
    pub fn memory_bytes(&self) -> u64 {
        let nodes = self.nodes.capacity() * std::mem::size_of::<Node>();
        // Table de hachage : clé, valeur et un octet de contrôle par case, à moitié pleine au pire.
        let ids = self.id_high.capacity() * (std::mem::size_of::<NodeId>() + std::mem::size_of::<u64>() + 1) * 2;
        (std::mem::size_of::<Scan>() + nodes + self.names.capacity() + ids + self.root_path.capacity()) as u64
    }

    pub fn root(&self) -> NodeId {
        0
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id as usize)
    }

    pub fn name(&self, id: NodeId) -> &str {
        self.node(id).map_or("", |n| &self.names[n.name_off as usize..n.name_off as usize + n.name_len as usize])
    }

    pub fn file_id(&self, id: NodeId) -> u128 {
        let low = self.node(id).map_or(0, |n| n.file_id);
        let high = self.id_high.get(&id).copied().unwrap_or(0);
        (u128::from(high) << 64) | u128::from(low)
    }

    pub fn children(&self, id: NodeId) -> Range<NodeId> {
        self.node(id).map_or(0..0, |n| n.first_child..n.first_child + n.child_count)
    }

    pub fn path(&self, id: NodeId) -> String {
        let mut parts = Vec::new();
        let mut cur = id;
        while cur != 0 {
            parts.push(self.name(cur));
            cur = self.node(cur).map_or(0, |n| n.parent);
        }
        // Chemins Windows ; « / » seulement pour les tests sous Linux.
        let sep = if self.root_path.starts_with('/') { '/' } else { '\\' };
        let mut path = self.root_path.clone();
        for p in parts.iter().rev() {
            if !path.ends_with(sep) {
                path.push(sep);
            }
            path.push_str(p);
        }
        path
    }

    /// Analyse restreinte au dossier `id`, reconstruite et réagrégée (liens physiques comptés
    /// dans ce seul dossier). Sert à extraire le dossier demandé d'une lecture de MFT, qui porte
    /// toujours sur le volume entier : l'assistant élevé ne reçoit jamais de chemin.
    pub fn subtree(&self, id: NodeId) -> Option<Scan> {
        let top = self.node(id)?;
        if top.kind != Kind::Dir {
            return None;
        }
        let raw = |i: NodeId| -> RawEntry {
            let n = &self.nodes[i as usize];
            RawEntry {
                name: self.name(i).to_owned(),
                kind: n.kind,
                attributes: n.attributes,
                file_id: self.file_id(i),
                logical: n.logical,
                allocated: n.allocated,
                modified: n.modified,
                accessed: n.accessed,
            }
        };
        let mut builder = Builder::new(&raw(id));
        if top.flags & F_UNREADABLE != 0 {
            builder.mark_unreadable(0);
        }
        let mut queue = std::collections::VecDeque::from([(id, 0 as NodeId)]);
        while let Some((old, new)) = queue.pop_front() {
            let olds: Vec<NodeId> = self.children(old).collect();
            let entries: Vec<RawEntry> = olds.iter().map(|&c| raw(c)).collect();
            let range = builder.add_children(new, &entries);
            for (k, &c) in olds.iter().enumerate() {
                let n = &self.nodes[c as usize];
                let child = range.start + k as NodeId;
                if n.flags & F_UNREADABLE != 0 {
                    builder.mark_unreadable(child);
                }
                if n.kind == Kind::Dir && n.child_count > 0 {
                    queue.push_back((c, child));
                }
            }
        }
        let root_path = self.path(id);
        let prefix = root_path.to_lowercase();
        let stats = Stats {
            elapsed_ms: self.stats.elapsed_ms,
            error_samples: self.stats.error_samples.iter().filter(|p| p.to_lowercase().starts_with(&prefix)).cloned().collect(),
            ..Stats::default()
        };
        Some(builder.finish(root_path, self.volume_serial, self.method, self.last_access_reliable, stats))
    }

    /// Nœud au chemin relatif donné (composants séparés par `\` ou `/`, sans tenir compte de la casse).
    pub fn find(&self, relative: &str) -> Option<NodeId> {
        let mut cur = self.root();
        for part in relative.split(['\\', '/']).filter(|p| !p.is_empty()) {
            cur = self.children(cur).find(|&c| self.name(c).eq_ignore_ascii_case(part) || self.name(c) == part)?;
        }
        Some(cur)
    }

    /// Dernière utilisation d'un fichier : dernier accès si Windows le tient à jour, sinon écriture.
    fn last_used_of_file(&self, n: &Node) -> i64 {
        if self.last_access_reliable {
            n.accessed.max(n.modified)
        } else {
            n.modified
        }
    }

    fn dedupe_hard_links(&mut self, stats: &mut Stats) {
        let mut ids: Vec<(u128, NodeId)> = self
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.kind == Kind::File)
            .map(|(i, _)| (self.file_id(i as NodeId), i as NodeId))
            .filter(|(fid, _)| *fid != 0)
            .collect();
        ids.sort_unstable();
        let mut i = 0;
        while i < ids.len() {
            let mut j = i + 1;
            while j < ids.len() && ids[j].0 == ids[i].0 {
                j += 1;
            }
            if j - i > 1 {
                // Le lien compté est celui au plus petit chemin : résultat stable d'un scan à l'autre,
                // quel que soit l'ordre du parcours parallèle.
                let keep = ids[i..j].iter().map(|&(_, n)| n).min_by_key(|&n| self.path(n).to_lowercase()).unwrap_or(ids[i].1);
                for &(_, n) in &ids[i..j] {
                    if n != keep {
                        if let Some(node) = self.nodes.get_mut(n as usize) {
                            node.flags |= F_HARDLINK_DUP;
                        }
                        stats.hardlink_dups += 1;
                    }
                }
            }
            i = j;
        }
    }

    fn aggregate(&mut self, stats: &mut Stats) {
        // Dossiers : on repart de zéro (leur taille propre n'est pas comptée, comme l'Explorateur).
        for n in self.nodes.iter_mut() {
            if matches!(n.kind, Kind::Dir | Kind::CloudDir) {
                n.logical = 0;
                n.allocated = 0;
                n.files = 0;
                n.accessed = i64::MIN;
            }
        }
        let mut files = 0u64;
        let mut dirs = 0u64;
        for i in (0..self.nodes.len()).rev() {
            let n = &self.nodes[i];
            let (alloc, logical, count, used) = match n.kind {
                Kind::File if n.flags & F_HARDLINK_DUP == 0 => {
                    files += 1;
                    (n.allocated, n.logical, 1u32, self.last_used_of_file(n))
                }
                Kind::File => (0, 0, 0, i64::MIN),
                Kind::Dir => {
                    dirs += 1;
                    (n.allocated, n.logical, n.files, n.accessed)
                }
                Kind::CloudDir => {
                    stats.cloud_dirs += 1;
                    (0, 0, 0, i64::MIN)
                }
                Kind::Link => {
                    stats.links += 1;
                    (0, 0, 0, i64::MIN)
                }
                Kind::CloudFile => {
                    stats.cloud_files += 1;
                    stats.cloud_bytes += n.logical;
                    (0, 0, 0, i64::MIN)
                }
            };
            if matches!(n.kind, Kind::File) {
                let node = &mut self.nodes[i];
                node.files = count;
            }
            if i == 0 {
                break;
            }
            let parent = self.nodes[i].parent as usize;
            let p = &mut self.nodes[parent];
            p.allocated += alloc;
            p.logical += logical;
            p.files += count;
            p.accessed = p.accessed.max(used);
        }
        // La racine est un dossier : elle-même n'est pas comptée parmi les dossiers rencontrés.
        stats.dirs = dirs.saturating_sub(1);
        stats.files = files;
        if let Some(root) = self.nodes.first() {
            stats.allocated = root.allocated;
            stats.logical = root.logical;
        }
    }

    fn slice_node(&self, id: NodeId) -> SliceNode {
        let n = &self.nodes[id as usize];
        let is_dir = matches!(n.kind, Kind::Dir);
        let last_used = if is_dir { n.accessed } else if n.kind == Kind::File { self.last_used_of_file(n) } else { n.modified };
        let dup = n.flags & F_HARDLINK_DUP != 0;
        SliceNode {
            id: i64::from(id),
            name: self.name(id).to_owned(),
            path: self.path(id),
            kind: n.kind,
            allocated: if dup { 0 } else { n.allocated },
            logical: if dup { 0 } else { n.logical },
            files: u64::from(n.files),
            last_used: if last_used == i64::MIN { n.modified } else { last_used },
            file_id: self.file_id(id),
            modified: n.modified,
            others: None,
            members: Vec::new(),
            has_children: is_dir && n.child_count > 0,
            children: Vec::new(),
        }
    }

    /// Tranche agrégée sous `id` : `depth` niveaux, au plus `top_n` enfants par niveau (les plus
    /// gros), le reste regroupé dans un nœud « autres ». Le front ne reçoit jamais toute l'arène.
    pub fn slice(&self, id: NodeId, depth: u8, top_n: usize) -> Option<SliceNode> {
        self.node(id)?;
        let mut node = self.slice_node(id);
        if depth > 0 {
            node.children = self.top_children(id, depth - 1, top_n);
        }
        Some(node)
    }

    /// Tranche à profondeur adaptative : enfants et petits-enfants toujours, puis jusqu'à
    /// `max_level` niveaux sous les seuls dossiers d'au moins `min_expand` octets (assez grands
    /// pour être dessinés avec leurs enfants). Le nombre de nœuds reste borné : peu de dossiers
    /// dépassent une fraction donnée de la racine.
    pub fn slice_adaptive(&self, id: NodeId, max_level: u8, top_n: usize, min_expand: u64) -> Option<SliceNode> {
        self.node(id)?;
        let mut node = self.slice_node(id);
        node.children = self.top_children_where(id, 1, top_n, &|level, size| level < 2 || (level < max_level && size >= min_expand));
        Some(node)
    }

    fn top_children(&self, id: NodeId, depth: u8, top_n: usize) -> Vec<SliceNode> {
        self.top_children_where(id, 1, top_n, &|level, _| level <= depth)
    }

    /// Enfants de niveau `level` ; `expand(niveau, taille)` dit si un enfant de ce niveau reçoit
    /// lui-même ses enfants.
    fn top_children_where(&self, id: NodeId, level: u8, top_n: usize, expand: &dyn Fn(u8, u64) -> bool) -> Vec<SliceNode> {
        let mut kids: Vec<NodeId> = self.children(id).collect();
        let size = |c: &NodeId| {
            let n = &self.nodes[*c as usize];
            if n.flags & F_HARDLINK_DUP != 0 {
                0
            } else {
                n.allocated
            }
        };
        kids.sort_by(|a, b| size(b).cmp(&size(a)).then_with(|| self.name(*a).cmp(self.name(*b))));
        let mut out: Vec<SliceNode> = kids
            .iter()
            .take(top_n)
            .map(|&c| {
                let mut s = self.slice_node(c);
                if s.has_children && expand(level, s.allocated) {
                    s.children = self.top_children_where(c, level + 1, top_n, expand);
                }
                s
            })
            .collect();
        let rest = &kids[kids.len().min(top_n)..];
        if !rest.is_empty() {
            let mut others = SliceNode {
                id: -(i64::from(id) + 1),
                name: String::new(),
                path: self.path(id),
                kind: Kind::Dir,
                allocated: 0,
                logical: 0,
                files: 0,
                last_used: i64::MIN,
                file_id: 0,
                modified: 0,
                others: Some(rest.len() as u32),
                members: rest.to_vec(),
                has_children: false,
                children: Vec::new(),
            };
            for &c in rest {
                let s = self.slice_node(c);
                others.allocated += s.allocated;
                others.logical += s.logical;
                others.files += s.files;
                others.last_used = others.last_used.max(s.last_used);
            }
            out.push(others);
        }
        out
    }
}

/// Sérialisation binaire de l'arène, pour la transmettre depuis l'assistant élevé (lecture de la
/// MFT). Le décodage valide chaque indice : un flux corrompu ou falsifié est rejeté, jamais suivi.
pub mod wire {
    use super::*;
    use std::io::{self, Read, Write};

    const MAGIC: &[u8; 4] = b"LUMS";
    const VERSION: u32 = 1;
    /// Bornes de sécurité du décodage (≈ 400 millions d'entrées, 4 Gio de noms).
    const MAX_NODES: u64 = 400_000_000;
    const MAX_NAMES: u64 = 4 << 30;

    fn bad(msg: &str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, msg.to_owned())
    }

    fn put_str(w: &mut impl Write, s: &str) -> io::Result<()> {
        w.write_all(&(s.len() as u64).to_le_bytes())?;
        w.write_all(s.as_bytes())
    }

    fn get<const N: usize>(r: &mut impl Read) -> io::Result<[u8; N]> {
        let mut b = [0u8; N];
        r.read_exact(&mut b)?;
        Ok(b)
    }

    fn get_u64(r: &mut impl Read) -> io::Result<u64> {
        Ok(u64::from_le_bytes(get::<8>(r)?))
    }

    fn get_str(r: &mut impl Read, max: u64) -> io::Result<String> {
        let len = get_u64(r)?;
        if len > max {
            return Err(bad("chaîne trop longue"));
        }
        let mut buf = vec![0u8; len as usize];
        r.read_exact(&mut buf)?;
        String::from_utf8(buf).map_err(|_| bad("UTF-8 invalide"))
    }

    pub fn encode(scan: &Scan, w: &mut impl Write) -> io::Result<()> {
        w.write_all(MAGIC)?;
        w.write_all(&VERSION.to_le_bytes())?;
        put_str(w, &scan.root_path)?;
        w.write_all(&scan.volume_serial.to_le_bytes())?;
        w.write_all(&[matches!(scan.method, Method::Mft) as u8, scan.last_access_reliable as u8])?;
        w.write_all(&(scan.nodes.len() as u64).to_le_bytes())?;
        for n in &scan.nodes {
            let mut b = [0u8; 70];
            b[0..4].copy_from_slice(&n.parent.to_le_bytes());
            b[4..8].copy_from_slice(&n.first_child.to_le_bytes());
            b[8..12].copy_from_slice(&n.child_count.to_le_bytes());
            b[12..16].copy_from_slice(&n.name_off.to_le_bytes());
            b[16..18].copy_from_slice(&n.name_len.to_le_bytes());
            b[18] = n.kind as u8;
            b[19] = n.flags;
            b[20..24].copy_from_slice(&n.attributes.to_le_bytes());
            b[24..32].copy_from_slice(&n.file_id.to_le_bytes());
            b[32..40].copy_from_slice(&n.logical.to_le_bytes());
            b[40..48].copy_from_slice(&n.allocated.to_le_bytes());
            b[48..56].copy_from_slice(&n.modified.to_le_bytes());
            b[56..64].copy_from_slice(&n.accessed.to_le_bytes());
            b[64..68].copy_from_slice(&n.files.to_le_bytes());
            w.write_all(&b)?;
        }
        put_str(w, &scan.names)?;
        w.write_all(&(scan.id_high.len() as u64).to_le_bytes())?;
        for (k, v) in &scan.id_high {
            w.write_all(&k.to_le_bytes())?;
            w.write_all(&v.to_le_bytes())?;
        }
        let st = &scan.stats;
        for v in [
            st.files, st.dirs, st.links, st.cloud_files, st.cloud_dirs, st.cloud_bytes, st.hardlink_dups,
            st.access_denied, st.locked, st.other_errors, st.allocated, st.logical, st.elapsed_ms,
        ] {
            w.write_all(&v.to_le_bytes())?;
        }
        w.write_all(&(st.error_samples.len() as u64).to_le_bytes())?;
        for e in &st.error_samples {
            put_str(w, e)?;
        }
        Ok(())
    }

    pub fn decode(r: &mut impl Read) -> io::Result<Scan> {
        if &get::<4>(r)? != MAGIC || u32::from_le_bytes(get::<4>(r)?) != VERSION {
            return Err(bad("format de scan inconnu"));
        }
        let root_path = get_str(r, 32 * 1024)?;
        let volume_serial = get_u64(r)?;
        let [method, last_access] = get::<2>(r)?;
        let count = get_u64(r)?;
        if count == 0 || count > MAX_NODES {
            return Err(bad("nombre d'entrées invalide"));
        }
        let mut nodes = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let b = get::<70>(r)?;
            let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
            let u64_at = |i: usize| u64::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3], b[i + 4], b[i + 5], b[i + 6], b[i + 7]]);
            let kind = match b[18] {
                0 => Kind::Dir,
                1 => Kind::File,
                2 => Kind::Link,
                3 => Kind::CloudFile,
                4 => Kind::CloudDir,
                _ => return Err(bad("type d'entrée invalide")),
            };
            nodes.push(Node {
                parent: u32_at(0),
                first_child: u32_at(4),
                child_count: u32_at(8),
                name_off: u32_at(12),
                name_len: u16::from_le_bytes([b[16], b[17]]),
                kind,
                flags: b[19],
                attributes: u32_at(20),
                file_id: u64_at(24),
                logical: u64_at(32),
                allocated: u64_at(40),
                modified: u64_at(48) as i64,
                accessed: u64_at(56) as i64,
                files: u32_at(64),
            });
        }
        let names = get_str(r, MAX_NAMES)?;
        let len = nodes.len() as u64;
        for (i, n) in nodes.iter().enumerate() {
            let end = u64::from(n.name_off) + u64::from(n.name_len);
            let children_end = u64::from(n.first_child) + u64::from(n.child_count);
            let ok = end <= names.len() as u64
                && names.is_char_boundary(n.name_off as usize)
                && names.is_char_boundary(end as usize)
                && (i == 0 || u64::from(n.parent) < i as u64)
                && (n.child_count == 0 || (u64::from(n.first_child) > i as u64 && children_end <= len));
            if !ok {
                return Err(bad("arène incohérente"));
            }
        }
        let high_count = get_u64(r)?;
        if high_count > len {
            return Err(bad("identifiants invalides"));
        }
        let mut id_high = HashMap::new();
        for _ in 0..high_count {
            let k = u32::from_le_bytes(get::<4>(r)?);
            let v = get_u64(r)?;
            id_high.insert(k, v);
        }
        let mut v = [0u64; 13];
        for x in v.iter_mut() {
            *x = get_u64(r)?;
        }
        let samples = get_u64(r)?;
        if samples > 1000 {
            return Err(bad("trop d'erreurs échantillonnées"));
        }
        let mut error_samples = Vec::new();
        for _ in 0..samples {
            error_samples.push(get_str(r, 32 * 1024)?);
        }
        let stats = Stats {
            files: v[0],
            dirs: v[1],
            links: v[2],
            cloud_files: v[3],
            cloud_dirs: v[4],
            cloud_bytes: v[5],
            hardlink_dups: v[6],
            access_denied: v[7],
            locked: v[8],
            other_errors: v[9],
            allocated: v[10],
            logical: v[11],
            elapsed_ms: v[12],
            error_samples,
        };
        Ok(Scan {
            root_path,
            volume_serial,
            method: if method == 1 { Method::Mft } else { Method::Walk },
            last_access_reliable: last_access == 1,
            nodes,
            names,
            id_high,
            stats,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, kind: Kind, id: u128, alloc: u64) -> RawEntry {
        RawEntry { name: name.into(), kind, attributes: 0, file_id: id, logical: alloc, allocated: alloc, modified: 10, accessed: 20 }
    }

    fn sample() -> Scan {
        let mut b = Builder::new(&entry("Arthur", Kind::Dir, 1, 0));
        let top = b.add_children(0, &[
            entry("Projets", Kind::Dir, 2, 0),
            entry("a.bin", Kind::File, 3, 100),
            entry("lien", Kind::Link, 4, 0),
            entry("cloud.docx", Kind::CloudFile, 5, 0),
        ]);
        let projets = top.start;
        b.add_children(projets, &[
            entry("x.bin", Kind::File, 6, 1000),
            entry("y.bin", Kind::File, 7, 10),
            entry("a-lien-physique.bin", Kind::File, 3, 100),
            entry("z.bin", Kind::File, 8, 1),
        ]);
        b.finish("C:\\Users\\Arthur".into(), 42, Method::Walk, false, Stats::default())
    }

    #[test]
    fn agregation_et_liens_physiques() {
        let s = sample();
        // a.bin et son lien physique ne comptent qu'une fois : a, x, y, z.
        assert_eq!(s.stats.files, 4);
        assert_eq!(s.stats.hardlink_dups, 1);
        assert_eq!(s.stats.allocated, 100 + 1000 + 10 + 1);
        assert_eq!(s.stats.links, 1);
        assert_eq!(s.stats.cloud_files, 1);
        assert_eq!(s.stats.dirs, 1);
        let projets = s.find("Projets").unwrap();
        // Le lien compté est celui au plus petit chemin : « …\arthur\a.bin » < « …\arthur\projets\… ».
        assert_eq!(s.node(projets).unwrap().allocated, 1011);
        assert_eq!(s.node(projets).unwrap().files, 3);
    }

    #[test]
    fn sous_arbre_reagrege() {
        let s = sample();
        let sub = s.subtree(s.find("Projets").unwrap()).unwrap();
        assert_eq!(sub.root_path, "C:\\Users\\Arthur\\Projets");
        // Le lien physique de a.bin, hors du dossier, est ici le seul nom : il est compté.
        assert_eq!(sub.stats.files, 4);
        assert_eq!(sub.stats.hardlink_dups, 0);
        assert_eq!(sub.stats.allocated, 1000 + 10 + 100 + 1);
        let y = sub.find("y.bin").unwrap();
        assert_eq!(sub.file_id(y), 7);
        assert_eq!(sub.path(y), "C:\\Users\\Arthur\\Projets\\y.bin");
        assert_eq!(sub.node(y).unwrap().modified, 10);
        // Le volume entier redonne la même analyse ; un fichier n'est pas un dossier.
        assert_eq!(s.subtree(0).unwrap().stats, s.stats);
        assert!(s.subtree(s.find("a.bin").unwrap()).is_none());
    }

    #[test]
    fn chemins_et_recherche() {
        let s = sample();
        let y = s.find("projets\\Y.BIN").unwrap();
        assert_eq!(s.path(y), "C:\\Users\\Arthur\\Projets\\y.bin");
        assert_eq!(s.file_id(y), 7);
        let mut b = Builder::new(&entry("C:", Kind::Dir, 5, 0));
        b.add_children(0, &[entry("Windows", Kind::Dir, 6, 0)]);
        let v = b.finish("C:\\".into(), 1, Method::Walk, false, Stats::default());
        assert_eq!(v.path(1), "C:\\Windows");
    }

    #[test]
    fn tranche_top_n_et_autres() {
        let s = sample();
        let slice = s.slice(s.find("Projets").unwrap(), 1, 2).unwrap();
        let names: Vec<&str> = slice.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["x.bin", "y.bin", ""]);
        let others = slice.children.last().unwrap();
        assert_eq!(others.others, Some(2));
        assert_eq!(others.allocated, 1);
        assert!(others.id < 0);
        let root = s.slice(0, 2, 10).unwrap();
        assert_eq!(root.children[0].name, "Projets");
        assert!(!root.children[0].children.is_empty());
    }

    /// Arbre profond : A (90 %) ⊃ B ⊃ C ⊃ D ⊃ E, et un petit dossier P ⊃ Q ⊃ R.
    fn deep() -> Scan {
        let mut b = Builder::new(&entry("Arthur", Kind::Dir, 1, 0));
        let top = b.add_children(0, &[entry("A", Kind::Dir, 2, 0), entry("P", Kind::Dir, 3, 0)]);
        let bb = b.add_children(top.start, &[entry("B", Kind::Dir, 4, 0)]).start;
        let c = b.add_children(bb, &[entry("C", Kind::Dir, 5, 0)]).start;
        let d = b.add_children(c, &[entry("D", Kind::Dir, 6, 0)]).start;
        let e = b.add_children(d, &[entry("E", Kind::Dir, 7, 0)]).start;
        b.add_children(e, &[entry("gros.bin", Kind::File, 8, 900)]);
        let q = b.add_children(top.start + 1, &[entry("Q", Kind::Dir, 9, 0)]).start;
        let r = b.add_children(q, &[entry("R", Kind::Dir, 10, 0)]).start;
        b.add_children(r, &[entry("petit.bin", Kind::File, 11, 100)]);
        b.finish("C:\\Users\\Arthur".into(), 1, Method::Walk, false, Stats::default())
    }

    fn depth_of(n: &SliceNode) -> usize {
        n.children.iter().map(|c| 1 + depth_of(c)).max().unwrap_or(0)
    }

    #[test]
    fn tranche_profondeur_adaptative() {
        let s = deep();
        let slice = s.slice_adaptive(0, 4, 30, 500).unwrap();
        let a = &slice.children[0];
        let p = &slice.children[1];
        assert_eq!((a.name.as_str(), p.name.as_str()), ("A", "P"));
        // Gros dossier : 4 niveaux sous la racine (A, B, C, D), jamais plus.
        assert_eq!(depth_of(&slice), 4);
        assert_eq!(a.children[0].children[0].children[0].name, "D");
        assert!(a.children[0].children[0].children[0].children.is_empty(), "D au 4e niveau : pas plus profond");
        // Petit dossier : enfants et petits-enfants seulement (P, Q), comme avant.
        assert_eq!(p.children[0].name, "Q");
        assert!(p.children[0].children.is_empty());
    }

    #[test]
    fn regroupement_garde_ses_membres() {
        let s = sample();
        let projets = s.find("Projets").unwrap();
        let slice = s.slice(projets, 1, 2).unwrap();
        let others = slice.children.last().unwrap();
        let names: Vec<&str> = others.members.iter().map(|&m| s.name(m)).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"z.bin"));
        assert!(slice.children[0].members.is_empty());
    }

    #[test]
    fn serialisation_aller_retour_et_rejet() {
        let s = sample();
        let mut buf = Vec::new();
        wire::encode(&s, &mut buf).unwrap();
        let back = wire::decode(&mut buf.as_slice()).unwrap();
        assert_eq!(back.stats, s.stats);
        assert_eq!(back.len(), s.len());
        let y = back.find("Projets/y.bin").unwrap();
        assert_eq!(back.path(y), s.path(y));
        assert_eq!(back.file_id(y), 7);
        // Flux tronqué ou falsifié : rejeté.
        assert!(wire::decode(&mut &buf[..buf.len() / 2]).is_err());
        let mut forged = buf.clone();
        let first_node = 4 + 4 + 8 + s.root_path.len() + 8 + 2 + 8;
        forged[first_node + 70 + 4..first_node + 70 + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        forged[first_node + 70 + 8..first_node + 70 + 12].copy_from_slice(&5u32.to_le_bytes());
        assert!(wire::decode(&mut forged.as_slice()).is_err());
    }

    #[test]
    fn identifiants_128_bits() {
        let mut b = Builder::new(&entry("R", Kind::Dir, 1, 0));
        b.add_children(0, &[entry("refs.bin", Kind::File, (7u128 << 64) | 9, 1), entry("autre.bin", Kind::File, 9, 1)]);
        let s = b.finish("D:\\".into(), 1, Method::Walk, false, Stats::default());
        assert_eq!(s.file_id(1), (7u128 << 64) | 9);
        // Même 64 bits bas, ID différents : pas des liens physiques.
        assert_eq!(s.stats.hardlink_dups, 0);
    }
}
