//! Système de fichiers en mémoire pour les tests portables du déplacement : disques, dossiers,
//! fichiers (contenu et métadonnées), jonctions suivies comme Windows, espace libre décompté à la
//! copie, programmes qui tiennent des fichiers, et une panne injectable sur n'importe quel effet.

use super::*;
use crate::path::fold;
use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Dir { meta: u32 },
    File { content: Vec<u8>, meta: u32 },
    Link { target: WinPath },
}

/// Effet qui échouera (une fois, à sa `n`-ième occurrence).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    CreateDir,
    CopyFile,
    CopyDirMeta,
    Rename,
    CreateJunction,
    RemoveJunction,
    DeleteTree,
    Recycle,
    /// La copie d'un fichier est altérée (un octet change) : la vérification doit le voir.
    CorruptCopy,
    /// La jonction est créée vers un autre dossier.
    WrongJunction,
}

#[derive(Default)]
pub struct MemFs {
    nodes: RefCell<BTreeMap<String, (WinPath, Node, bool, bool)>>,
    pub volumes: RefCell<BTreeMap<char, VolumeInfo>>,
    pub holders: RefCell<Vec<(String, String)>>,
    fail: RefCell<Vec<(Op, u32)>>,
    pub recycled: RefCell<Vec<WinPath>>,
    pub log: RefCell<Vec<String>>,
}

fn key(p: &WinPath) -> String {
    fold(&p.display_string())
}

fn p(raw: &str) -> WinPath {
    WinPath::parse(raw).unwrap()
}

impl MemFs {
    pub fn new() -> MemFs {
        let fs = MemFs::default();
        let fixed = VolumeInfo { kind: DriveKind::Fixed, subst: false, ntfs: true, free: 1 << 40 };
        fs.volumes.borrow_mut().insert('C', fixed);
        fs.volumes.borrow_mut().insert('D', fixed);
        fs
    }

    fn put(&self, path: WinPath, node: Node, reparse: bool, cloud: bool) {
        // Les parents existent toujours, comme sur un vrai disque.
        let chain = path.chain();
        for parent in &chain[..chain.len().saturating_sub(1)] {
            self.nodes.borrow_mut().entry(key(parent)).or_insert_with(|| (parent.clone(), Node::Dir { meta: 0 }, false, false));
        }
        self.nodes.borrow_mut().insert(key(&path), (path, node, reparse, cloud));
    }

    pub fn dir(&self, raw: &str) -> WinPath {
        let path = p(raw);
        self.put(path.clone(), Node::Dir { meta: 7 }, false, false);
        path
    }

    pub fn file(&self, raw: &str, content: &[u8]) -> WinPath {
        let path = p(raw);
        self.put(path.clone(), Node::File { content: content.to_vec(), meta: 42 }, false, false);
        path
    }

    pub fn cloud_file(&self, raw: &str) {
        self.put(p(raw), Node::File { content: vec![1; 10], meta: 0 }, false, true);
    }

    pub fn link(&self, raw: &str, target: &str) {
        self.put(p(raw), Node::Link { target: p(target) }, true, false);
    }

    pub fn hold(&self, raw: &str, app: &str) {
        self.holders.borrow_mut().push((key(&p(raw)), app.into()));
    }

    /// L'effet `op` échouera à sa `nth` occurrence (plusieurs pannes possibles).
    pub fn fail_on(&self, op: Op, nth: u32) {
        self.fail.borrow_mut().push((op, nth));
    }

    pub fn set_free(&self, drive: char, free: u64) {
        if let Some(v) = self.volumes.borrow_mut().get_mut(&drive) {
            v.free = free;
        }
    }

    pub fn free(&self, drive: char) -> u64 {
        self.volumes.borrow().get(&drive).map_or(0, |v| v.free)
    }

    /// Le nœud à ce chemin, sans suivre le dernier composant.
    pub fn node(&self, raw: &str) -> Option<Node> {
        let real = self.resolve(&p(raw), false)?;
        self.nodes.borrow().get(&key(&real)).map(|n| n.1.clone())
    }

    /// Tout le contenu sous un chemin réel : (relatif, nœud), trié.
    pub fn snapshot(&self, raw: &str) -> Vec<(String, Node)> {
        let Some(root) = self.resolve(&p(raw), true) else { return Vec::new() };
        let prefix = key(&root) + "\\";
        self.nodes.borrow().iter().filter(|(k, _)| k.starts_with(&prefix)).map(|(k, n)| (k[prefix.len()..].to_owned(), n.1.clone())).collect()
    }

    /// Chemin réel : jonctions des composants intermédiaires suivies (et du dernier si `follow`).
    fn resolve(&self, path: &WinPath, follow: bool) -> Option<WinPath> {
        let mut current = path.volume_root();
        let names = path.components();
        for (i, name) in names.iter().enumerate() {
            let next = current.join(name).ok()?;
            let last = i + 1 == names.len();
            let node = self.nodes.borrow().get(&key(&next)).map(|n| n.1.clone());
            current = match node {
                Some(Node::Link { target }) if !last || follow => target,
                Some(_) => next,
                None if last => next,
                None => return None,
            };
        }
        Some(current)
    }

    fn trip(&self, op: Op) -> bool {
        let mut fail = self.fail.borrow_mut();
        let mut hit = false;
        fail.retain_mut(|(o, n)| {
            if *o != op || hit {
                return true;
            }
            if *n <= 1 {
                hit = true;
                false
            } else {
                *n -= 1;
                true
            }
        });
        hit
    }

    fn check(&self, op: Op) -> Result<(), MoveIoError> {
        if self.trip(op) {
            Err(MoveIoError::Io(format!("panne simulée : {op:?}")))
        } else {
            Ok(())
        }
    }

    fn get(&self, path: &WinPath) -> Option<(WinPath, Node)> {
        if path.is_volume_root() {
            return self.volumes.borrow().contains_key(&Self::drive_of(path)).then(|| (path.clone(), Node::Dir { meta: 0 }));
        }
        let real = self.resolve(path, false)?;
        self.nodes.borrow().get(&key(&real)).map(|n| (real.clone(), n.1.clone()))
    }

    fn drive_of(path: &WinPath) -> char {
        match path.prefix() {
            crate::path::Prefix::Drive(d) => *d,
            crate::path::Prefix::Unc { .. } => '?',
        }
    }
}

impl MoveProbe for MemFs {
    fn volume(&self, drive: char) -> Option<VolumeInfo> {
        self.volumes.borrow().get(&drive).copied()
    }

    fn kind(&self, path: &WinPath) -> Option<PathKind> {
        self.get(path).map(|(_, n)| match n {
            Node::Dir { .. } => PathKind::Dir,
            Node::File { .. } => PathKind::File,
            Node::Link { .. } => PathKind::Link,
        })
    }

    fn tree(&self, dir: &WinPath) -> Result<Vec<TreeEntry>, ProbeError> {
        let real = self.resolve(dir, true).ok_or(ProbeError::NotFound)?;
        let prefix = key(&real) + "\\";
        let nodes = self.nodes.borrow();
        let mut out = Vec::new();
        for (k, (path, node, reparse, cloud)) in nodes.iter() {
            if !k.starts_with(&prefix) {
                continue;
            }
            // Jamais à travers un lien : ce qui est sous un lien n'est pas listé.
            let rel_parts = &path.components()[real.depth()..];
            let mut under_link = false;
            let mut q = real.clone();
            for c in &rel_parts[..rel_parts.len().saturating_sub(1)] {
                q = q.join(c).map_err(|e| ProbeError::BadPath(e.to_string()))?;
                if matches!(nodes.get(&key(&q)), Some((_, Node::Link { .. }, _, _))) {
                    under_link = true;
                }
            }
            if under_link {
                continue;
            }
            let (is_dir, size) = match node {
                Node::Dir { .. } | Node::Link { .. } => (true, 0),
                Node::File { content, .. } => (false, content.len() as u64),
            };
            out.push(TreeEntry { rel: rel_parts.join("\\"), is_dir, size, reparse: *reparse, cloud: *cloud });
        }
        Ok(out)
    }

    fn holders(&self, files: &[WinPath]) -> Vec<String> {
        let held = self.holders.borrow();
        let mut apps: Vec<String> = Vec::new();
        for f in files {
            let Some(real) = self.resolve(f, true) else { continue };
            for (k, app) in held.iter() {
                if *k == key(&real) && !apps.contains(app) {
                    apps.push(app.clone());
                }
            }
        }
        apps
    }

    fn hash(&self, file: &WinPath) -> Result<[u8; 32], ProbeError> {
        let real = self.resolve(file, true).ok_or(ProbeError::NotFound)?;
        match self.nodes.borrow().get(&key(&real)) {
            Some((_, Node::File { content, .. }, _, _)) => Ok(*blake3::hash(content).as_bytes()),
            _ => Err(ProbeError::NotFound),
        }
    }

    fn junction_target(&self, link: &WinPath) -> Option<WinPath> {
        match self.get(link)?.1 {
            Node::Link { target } => Some(target),
            _ => None,
        }
    }
}

impl MoveEffects for MemFs {
    fn create_dir(&self, path: &WinPath, template: &WinPath) -> Result<(), MoveIoError> {
        self.check(Op::CreateDir)?;
        if self.get(path).is_some() {
            return Err(MoveIoError::Io("existe déjà".into()));
        }
        if self.get(template).is_none() {
            return Err(MoveIoError::Io("modèle introuvable".into()));
        }
        let real = self.resolve(path, false).ok_or(MoveIoError::Io("parent introuvable".into()))?;
        self.log.borrow_mut().push(format!("mkdir {real}"));
        // Dates et ACL viennent ensuite (`copy_dir_meta`) : le dossier neuf n'en a pas encore.
        self.nodes.borrow_mut().insert(key(&real), (real, Node::Dir { meta: 0 }, false, false));
        Ok(())
    }

    fn copy_file(&self, from: &WinPath, to: &WinPath) -> Result<(), MoveIoError> {
        self.check(Op::CopyFile)?;
        let Some((_, Node::File { mut content, meta })) = self.get(from) else { return Err(MoveIoError::Io("source introuvable".into())) };
        if self.get(to).is_some() {
            return Err(MoveIoError::Io("existe déjà".into()));
        }
        let real = self.resolve(to, false).ok_or(MoveIoError::Io("parent introuvable".into()))?;
        let d = Self::drive_of(&real);
        let size = content.len() as u64;
        {
            let mut vols = self.volumes.borrow_mut();
            let v = vols.get_mut(&d).ok_or(MoveIoError::Io("disque absent".into()))?;
            if v.free < size {
                return Err(MoveIoError::Io("disque plein".into()));
            }
            v.free -= size;
        }
        if self.trip(Op::CorruptCopy) {
            if let Some(b) = content.first_mut() {
                *b ^= 0xFF;
            }
        }
        self.nodes.borrow_mut().insert(key(&real), (real, Node::File { content, meta }, false, false));
        Ok(())
    }

    fn copy_dir_meta(&self, from: &WinPath, to: &WinPath, _top: bool) -> Result<(), MoveIoError> {
        self.check(Op::CopyDirMeta)?;
        let Some((_, Node::Dir { meta })) = self.get(from) else { return Err(MoveIoError::Io("source introuvable".into())) };
        let real = self.resolve(to, true).ok_or(MoveIoError::Io("cible introuvable".into()))?;
        self.nodes.borrow_mut().insert(key(&real), (real, Node::Dir { meta }, false, false));
        Ok(())
    }

    fn rename(&self, from: &WinPath, to: &WinPath) -> Result<(), MoveIoError> {
        self.check(Op::Rename)?;
        let (real_from, _) = self.get(from).ok_or(MoveIoError::Io("introuvable".into()))?;
        if self.get(to).is_some() {
            return Err(MoveIoError::Io("existe déjà".into()));
        }
        let real_to = self.resolve(to, false).ok_or(MoveIoError::Io("parent introuvable".into()))?;
        if Self::drive_of(&real_from) != Self::drive_of(&real_to) {
            return Err(MoveIoError::Io("renommage entre deux disques".into()));
        }
        self.log.borrow_mut().push(format!("rename {real_from} -> {real_to}"));
        let mut nodes = self.nodes.borrow_mut();
        let moved: Vec<String> = nodes.keys().filter(|k| **k == key(&real_from) || k.starts_with(&(key(&real_from) + "\\"))).cloned().collect();
        for k in moved {
            if let Some((path, node, r, c)) = nodes.remove(&k) {
                let rest = &path.components()[real_from.depth()..];
                let new = rest.iter().try_fold(real_to.clone(), |acc, c| acc.join(c)).map_err(|e| MoveIoError::Io(e.to_string()))?;
                nodes.insert(key(&new), (new, node, r, c));
            }
        }
        Ok(())
    }

    fn create_junction(&self, link: &WinPath, target: &WinPath) -> Result<(), MoveIoError> {
        self.check(Op::CreateJunction)?;
        if self.get(link).is_some() {
            return Err(MoveIoError::Io("existe déjà".into()));
        }
        let target = if self.trip(Op::WrongJunction) { target.parent().unwrap_or_else(|| target.clone()) } else { target.clone() };
        let real = self.resolve(link, false).ok_or(MoveIoError::Io("parent introuvable".into()))?;
        self.log.borrow_mut().push(format!("junction {real} -> {target}"));
        self.nodes.borrow_mut().insert(key(&real), (real, Node::Link { target }, true, false));
        Ok(())
    }

    fn remove_junction(&self, link: &WinPath, expected: Option<&WinPath>) -> Result<(), MoveIoError> {
        self.check(Op::RemoveJunction)?;
        let (real, node) = self.get(link).ok_or(MoveIoError::Io("introuvable".into()))?;
        let Node::Link { target } = node else { return Err(MoveIoError::Io("pas une jonction".into())) };
        if expected.is_some_and(|e| *e != target) {
            return Err(MoveIoError::BadJunction);
        }
        self.log.borrow_mut().push(format!("unlink {real}"));
        self.nodes.borrow_mut().remove(&key(&real));
        Ok(())
    }

    fn delete_tree(&self, path: &WinPath) -> Result<(), MoveIoError> {
        self.check(Op::DeleteTree)?;
        let (real, _) = self.get(path).ok_or(MoveIoError::Io("introuvable".into()))?;
        let d = Self::drive_of(&real);
        self.log.borrow_mut().push(format!("delete {real}"));
        let mut nodes = self.nodes.borrow_mut();
        let k = key(&real);
        let gone: Vec<String> = nodes.keys().filter(|x| **x == k || x.starts_with(&(k.clone() + "\\"))).cloned().collect();
        let mut freed = 0;
        for g in gone {
            // Jamais à travers un lien : seul le lien lui-même part (ce qui est sous lui n'est
            // pas dans cet arbre, ses clés sont ailleurs).
            if let Some((_, Node::File { content, .. }, _, _)) = nodes.remove(&g) {
                freed += content.len() as u64;
            }
        }
        drop(nodes);
        if let Some(v) = self.volumes.borrow_mut().get_mut(&d) {
            v.free += freed;
        }
        Ok(())
    }

    fn recycle(&self, path: &WinPath) -> Result<(), MoveIoError> {
        self.check(Op::Recycle)?;
        let (real, _) = self.get(path).ok_or(MoveIoError::Io("introuvable".into()))?;
        let k = key(&real);
        self.nodes.borrow_mut().retain(|x, _| *x != k && !x.starts_with(&(k.clone() + "\\")));
        self.recycled.borrow_mut().push(real);
        Ok(())
    }
}
