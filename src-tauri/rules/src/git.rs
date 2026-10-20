//! Date du dernier commit d'un dépôt, lue dans `.git` **sans lancer git** : `HEAD` et les
//! branches locales (fichiers de références ou `packed-refs`), puis chaque commit, objet isolé
//! (zlib) ou dans un packfile (index v2, deltas `OFS_DELTA` / `REF_DELTA`). SHA-1 et SHA-256.
//! Les worktrees et sous-modules (`.git` fichier, `commondir`) sont suivis. Rien n'est écrit,
//! et les dates de dernier accès ne bougent pas (`fsread`).

use std::fs::File;
use std::path::{Path, PathBuf};

use miniz_oxide::inflate::core::{decompress, inflate_flags, DecompressorOxide};
use miniz_oxide::inflate::{decompress_to_vec_zlib_with_limit, TINFLStatus};

use crate::fsread;

/// Taille maximale d'un objet décompressé (un commit fait quelques centaines d'octets).
const MAX_OBJECT: usize = 16 << 20;
const MAX_DELTA_DEPTH: u32 = 64;
const MAX_REFS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GitError {
    #[error("pas un dépôt git")]
    NotARepo,
    #[error("dépôt sans commit")]
    Unborn,
    #[error("objet {0} introuvable")]
    Missing(String),
    #[error("dépôt illisible : {0}")]
    Corrupt(String),
}

fn corrupt(e: impl std::fmt::Display) -> GitError {
    GitError::Corrupt(e.to_string())
}

struct Repo {
    git_dir: PathBuf,
    common: PathBuf,
}

fn read_text(path: &Path, max: u64) -> Option<String> {
    fsread::read_limited(path, max).ok().map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn is_hex_id(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()).collect()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

impl Repo {
    /// `entry` : le `.git` d'un projet (dossier, ou fichier `gitdir:` d'un worktree/sous-module).
    fn open(entry: &Path) -> Result<Repo, GitError> {
        let git_dir = if entry.is_file() {
            let text = read_text(entry, 4096).ok_or(GitError::NotARepo)?;
            let target = text.trim().strip_prefix("gitdir:").ok_or(GitError::NotARepo)?.trim().to_owned();
            let base = entry.parent().ok_or(GitError::NotARepo)?;
            base.join(target)
        } else {
            entry.to_path_buf()
        };
        if !git_dir.join("HEAD").is_file() {
            return Err(GitError::NotARepo);
        }
        let common = match read_text(&git_dir.join("commondir"), 4096) {
            Some(c) if !c.trim().is_empty() => git_dir.join(c.trim()),
            _ => git_dir.clone(),
        };
        Ok(Repo { git_dir, common })
    }

    /// Valeur d'une référence (`refs/heads/main`), en suivant les références symboliques.
    fn resolve(&self, name: &str, depth: u32) -> Option<String> {
        if depth > 5 || name.split('/').any(|p| p.is_empty() || p == "..") {
            return None;
        }
        for dir in [&self.git_dir, &self.common] {
            if let Some(text) = read_text(&dir.join(name), 4096) {
                let t = text.trim();
                if let Some(target) = t.strip_prefix("ref:") {
                    return self.resolve(target.trim(), depth + 1);
                }
                if is_hex_id(t) {
                    return Some(t.to_ascii_lowercase());
                }
            }
        }
        self.packed_refs().into_iter().find(|(_, r)| r == name).map(|(id, _)| id)
    }

    fn packed_refs(&self) -> Vec<(String, String)> {
        let Some(text) = read_text(&self.common.join("packed-refs"), 16 << 20) else { return Vec::new() };
        text.lines()
            .filter(|l| !l.starts_with('#') && !l.starts_with('^'))
            .filter_map(|l| l.split_once(' '))
            .filter(|(id, _)| is_hex_id(id))
            .map(|(id, name)| (id.to_ascii_lowercase(), name.trim().to_owned()))
            .take(MAX_REFS * 4)
            .collect()
    }

    /// Commits de `HEAD` et des branches locales.
    fn tips(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(head) = self.resolve("HEAD", 0) {
            out.push(head);
        }
        let mut stack = vec![(self.common.join("refs").join("heads"), "refs/heads".to_owned())];
        while let Some((dir, name)) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else { continue };
            for e in entries.flatten() {
                if out.len() >= MAX_REFS {
                    break;
                }
                let child = format!("{name}/{}", e.file_name().to_string_lossy());
                if e.path().is_dir() {
                    stack.push((e.path(), child));
                } else if let Some(id) = self.resolve(&child, 0) {
                    out.push(id);
                }
            }
        }
        out.extend(self.packed_refs().into_iter().filter(|(_, n)| n.starts_with("refs/heads/")).map(|(id, _)| id).take(MAX_REFS));
        out.sort();
        out.dedup();
        out
    }

    /// Objet `id` : (type, contenu).
    fn object(&self, id: &str, depth: u32) -> Result<(u8, Vec<u8>), GitError> {
        if depth > MAX_DELTA_DEPTH {
            return Err(corrupt("chaîne de deltas trop longue"));
        }
        let objects = self.common.join("objects");
        let loose = objects.join(&id[..2]).join(&id[2..]);
        if loose.is_file() {
            let raw = fsread::read_limited(&loose, MAX_OBJECT as u64).map_err(corrupt)?;
            let data = decompress_to_vec_zlib_with_limit(&raw, MAX_OBJECT).map_err(|_| corrupt("objet isolé illisible"))?;
            let nul = data.iter().position(|&b| b == 0).ok_or_else(|| corrupt("en-tête d'objet"))?;
            let kind = match data.get(..nul).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| h.split(' ').next()) {
                Some("commit") => 1,
                Some("tree") => 2,
                Some("blob") => 3,
                Some("tag") => 4,
                _ => return Err(corrupt("type d'objet")),
            };
            return Ok((kind, data[nul + 1..].to_vec()));
        }
        let raw_id = unhex(id).ok_or_else(|| corrupt("identifiant"))?;
        let Ok(packs) = std::fs::read_dir(objects.join("pack")) else { return Err(GitError::Missing(id.to_owned())) };
        for e in packs.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("idx") {
                continue;
            }
            if let Some(offset) = find_in_index(&path, &raw_id)? {
                let mut pack = fsread::open(&path.with_extension("pack")).map_err(corrupt)?;
                return self.pack_entry(&mut pack, offset, raw_id.len(), depth);
            }
        }
        Err(GitError::Missing(id.to_owned()))
    }

    fn pack_entry(&self, pack: &mut File, offset: u64, hash_len: usize, depth: u32) -> Result<(u8, Vec<u8>), GitError> {
        if depth > MAX_DELTA_DEPTH {
            return Err(corrupt("chaîne de deltas trop longue"));
        }
        let head = fsread::read_at(pack, offset, 32 + hash_len).map_err(corrupt)?;
        let mut i = 0;
        let first = *head.first().ok_or_else(|| corrupt("entrée de pack"))?;
        let kind = (first >> 4) & 7;
        let mut size = u64::from(first & 15);
        let mut shift = 4;
        let mut b = first;
        while b & 0x80 != 0 {
            i += 1;
            b = *head.get(i).ok_or_else(|| corrupt("taille d'entrée"))?;
            size |= u64::from(b & 0x7f) << shift;
            shift += 7;
            if shift > 63 {
                return Err(corrupt("taille d'entrée"));
            }
        }
        i += 1;
        if size as usize > MAX_OBJECT {
            return Err(corrupt("objet trop gros"));
        }
        match kind {
            1..=4 => Ok((kind, inflate_at(pack, offset + i as u64, size as usize)?)),
            6 => {
                let mut c = *head.get(i).ok_or_else(|| corrupt("delta"))?;
                let mut back = u64::from(c & 0x7f);
                while c & 0x80 != 0 {
                    i += 1;
                    c = *head.get(i).ok_or_else(|| corrupt("delta"))?;
                    back = ((back + 1) << 7) | u64::from(c & 0x7f);
                }
                i += 1;
                let base_offset = offset.checked_sub(back).filter(|_| back > 0).ok_or_else(|| corrupt("delta hors du pack"))?;
                let delta = inflate_at(pack, offset + i as u64, size as usize)?;
                let (base_kind, base) = self.pack_entry(pack, base_offset, hash_len, depth + 1)?;
                Ok((base_kind, apply_delta(&base, &delta)?))
            }
            7 => {
                let base_id = head.get(i..i + hash_len).ok_or_else(|| corrupt("delta"))?;
                let delta = inflate_at(pack, offset + (i + hash_len) as u64, size as usize)?;
                let (base_kind, base) = self.object(&hex(base_id), depth + 1)?;
                Ok((base_kind, apply_delta(&base, &delta)?))
            }
            _ => Err(corrupt("type d'entrée de pack")),
        }
    }
}

fn be32(b: &[u8]) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(..4)?.try_into().ok()?))
}

/// Position de l'objet dans le pack, d'après l'index v2 (recherche dichotomique, lectures ciblées).
fn find_in_index(path: &Path, id: &[u8]) -> Result<Option<u64>, GitError> {
    let mut f = fsread::open(path).map_err(corrupt)?;
    let head = fsread::read_at(&mut f, 0, 8 + 1024).map_err(corrupt)?;
    if head.len() < 8 + 1024 || head[..4] != [0xff, b't', b'O', b'c'] || be32(&head[4..]) != Some(2) {
        return Err(corrupt("index de pack non v2"));
    }
    let fan = |i: usize| be32(&head[8 + i * 4..]).unwrap_or(0) as u64;
    let total = fan(255);
    let b = id[0] as usize;
    let (mut lo, mut hi) = (if b == 0 { 0 } else { fan(b - 1) }, fan(b));
    let hl = id.len() as u64;
    let names = 8 + 1024u64;
    while lo < hi {
        let mid = (lo + hi) / 2;
        let name = fsread::read_at(&mut f, names + mid * hl, hl as usize).map_err(corrupt)?;
        match name.as_slice().cmp(id) {
            std::cmp::Ordering::Equal => {
                let offsets = names + total * hl + total * 4;
                let raw = fsread::read_at(&mut f, offsets + mid * 4, 4).map_err(corrupt)?;
                let off = be32(&raw).ok_or_else(|| corrupt("index"))?;
                if off & 0x8000_0000 == 0 {
                    return Ok(Some(u64::from(off)));
                }
                let large = offsets + total * 4 + u64::from(off & 0x7fff_ffff) * 8;
                let raw = fsread::read_at(&mut f, large, 8).map_err(corrupt)?;
                return Ok(Some(u64::from_be_bytes(raw.as_slice().try_into().map_err(|_| corrupt("index"))?)));
            }
            std::cmp::Ordering::Less => lo = mid + 1,
            std::cmp::Ordering::Greater => hi = mid,
        }
    }
    Ok(None)
}

/// Décompresse le flux zlib qui commence à `offset` (taille décompressée connue).
fn inflate_at(pack: &mut File, offset: u64, size: usize) -> Result<Vec<u8>, GitError> {
    let mut want = size + size / 2 + 4096;
    loop {
        let input = fsread::read_at(pack, offset, want).map_err(corrupt)?;
        let mut out = vec![0u8; size];
        let mut state = Box::new(DecompressorOxide::new());
        let flags = inflate_flags::TINFL_FLAG_PARSE_ZLIB_HEADER | inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;
        let (status, _, written) = decompress(&mut state, &input, &mut out, 0, flags);
        match status {
            TINFLStatus::Done if written == size => return Ok(out),
            TINFLStatus::FailedCannotMakeProgress | TINFLStatus::NeedsMoreInput if input.len() == want && want < MAX_OBJECT * 2 => {
                want *= 2;
            }
            _ => return Err(corrupt("entrée de pack illisible")),
        }
    }
}

fn varint(d: &[u8], i: &mut usize) -> Option<usize> {
    let (mut v, mut shift) = (0usize, 0);
    loop {
        let b = *d.get(*i)?;
        *i += 1;
        v |= usize::from(b & 0x7f).checked_shl(shift)?;
        shift += 7;
        if b & 0x80 == 0 || shift > 56 {
            return Some(v);
        }
    }
}

/// Applique un delta git (copies depuis la base, insertions).
fn apply_delta(base: &[u8], d: &[u8]) -> Result<Vec<u8>, GitError> {
    let bad = || corrupt("delta invalide");
    let mut i = 0;
    let src = varint(d, &mut i).ok_or_else(bad)?;
    let dst = varint(d, &mut i).ok_or_else(bad)?;
    if src != base.len() || dst > MAX_OBJECT {
        return Err(bad());
    }
    let mut out = Vec::with_capacity(dst);
    while i < d.len() {
        let op = d[i];
        i += 1;
        if op & 0x80 != 0 {
            let mut off = 0usize;
            let mut len = 0usize;
            for k in 0..4 {
                if op & (1 << k) != 0 {
                    off |= usize::from(*d.get(i).ok_or_else(bad)?) << (8 * k);
                    i += 1;
                }
            }
            for k in 0..3 {
                if op & (0x10 << k) != 0 {
                    len |= usize::from(*d.get(i).ok_or_else(bad)?) << (8 * k);
                    i += 1;
                }
            }
            if len == 0 {
                len = 0x10000;
            }
            out.extend_from_slice(base.get(off..off.checked_add(len).ok_or_else(bad)?).ok_or_else(bad)?);
        } else if op != 0 {
            let n = usize::from(op);
            out.extend_from_slice(d.get(i..i + n).ok_or_else(bad)?);
            i += n;
        } else {
            return Err(bad());
        }
        if out.len() > dst {
            return Err(bad());
        }
    }
    if out.len() != dst {
        return Err(bad());
    }
    Ok(out)
}

/// Date du committer (secondes depuis 1970) d'un commit.
fn committer_time(body: &[u8]) -> Option<i64> {
    let text = String::from_utf8_lossy(body);
    let header = text.split("\n\n").next()?;
    let line = header.lines().find(|l| l.starts_with("committer "))?;
    let mut parts = line.rsplitn(3, ' ');
    let _tz = parts.next()?;
    parts.next()?.parse().ok()
}

/// Date (secondes depuis 1970) du commit le plus récent parmi `HEAD` et les branches locales.
pub fn last_commit(git_entry: &Path) -> Result<i64, GitError> {
    let repo = Repo::open(git_entry)?;
    let tips = repo.tips();
    if tips.is_empty() {
        return Err(GitError::Unborn);
    }
    let mut best: Option<i64> = None;
    let mut first_error = None;
    for id in tips {
        match repo.object(&id, 0) {
            Ok((1, body)) => match committer_time(&body) {
                Some(t) => best = Some(best.map_or(t, |b| b.max(t))),
                None => first_error = first_error.or(Some(corrupt("commit sans date"))),
            },
            Ok(_) => first_error = first_error.or(Some(corrupt("la référence ne désigne pas un commit"))),
            Err(e) => first_error = first_error.or(Some(e)),
        }
    }
    best.ok_or_else(|| first_error.unwrap_or(GitError::Unborn))
}

#[cfg(test)]
mod tests {
    use super::*;
    use miniz_oxide::deflate::compress_to_vec_zlib;
    use std::fs;

    fn commit(ts: i64, msg: &str) -> Vec<u8> {
        format!(
            "tree 4b825dc642cb6eb9a060e54bf8d69288fbee4904\nauthor Arthur <a@exemple.fr> 1600000000 +0200\ncommitter Arthur <a@exemple.fr> {ts} +0200\n\n{msg}\n"
        )
        .into_bytes()
    }

    fn write(path: &Path, data: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, data).unwrap();
    }

    fn loose(git: &Path, id: &str, body: &[u8]) {
        let mut raw = format!("commit {}\0", body.len()).into_bytes();
        raw.extend_from_slice(body);
        write(&git.join("objects").join(&id[..2]).join(&id[2..]), &compress_to_vec_zlib(&raw, 6));
    }

    fn entry_header(kind: u8, size: usize) -> Vec<u8> {
        let mut out = Vec::new();
        let mut b = (kind << 4) | (size & 15) as u8;
        let mut rest = size >> 4;
        while rest > 0 {
            out.push(b | 0x80);
            b = (rest & 0x7f) as u8;
            rest >>= 7;
        }
        out.push(b);
        out
    }

    fn varint_enc(mut v: usize) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return out;
            }
            out.push(b | 0x80);
        }
    }

    /// Delta : copie du préfixe commun de la base, puis insertion du reste.
    fn delta(base: &[u8], target: &[u8]) -> Vec<u8> {
        let common = base.iter().zip(target).take_while(|(a, b)| a == b).count().min(0xff);
        let mut d = varint_enc(base.len());
        d.extend(varint_enc(target.len()));
        d.extend([0x80 | 0x10, common as u8]); // copie : décalage 0, longueur sur un octet
        for chunk in target[common..].chunks(127) {
            d.push(chunk.len() as u8);
            d.extend_from_slice(chunk);
        }
        d
    }

    /// Pack + index v2 : une base (commit), un OFS_DELTA et un REF_DELTA.
    fn pack(git: &Path, base_id: &[u8], ofs_id: &[u8], ref_id: &[u8], bodies: [&[u8]; 3]) {
        let mut p = b"PACK".to_vec();
        p.extend(2u32.to_be_bytes());
        p.extend(3u32.to_be_bytes());
        let base_at = p.len() as u64;
        p.extend(entry_header(1, bodies[0].len()));
        p.extend(compress_to_vec_zlib(bodies[0], 6));
        let ofs_at = p.len() as u64;
        let d1 = delta(bodies[0], bodies[1]);
        p.extend(entry_header(6, d1.len()));
        // Distance vers la base, encodage git (« décalage + 1 » à chaque octet de continuation).
        let mut back = ofs_at - base_at;
        let mut enc = vec![(back & 0x7f) as u8];
        back >>= 7;
        while back > 0 {
            back -= 1;
            enc.push(0x80 | (back & 0x7f) as u8);
            back >>= 7;
        }
        enc.reverse();
        p.extend(enc);
        p.extend(compress_to_vec_zlib(&d1, 6));
        let ref_at = p.len() as u64;
        let d2 = delta(bodies[1], bodies[2]);
        p.extend(entry_header(7, d2.len()));
        p.extend_from_slice(ofs_id);
        p.extend(compress_to_vec_zlib(&d2, 6));
        write(&git.join("objects/pack/pack-test.pack"), &p);

        let mut entries = vec![(base_id.to_vec(), base_at), (ofs_id.to_vec(), ofs_at), (ref_id.to_vec(), ref_at)];
        entries.sort();
        let mut idx = vec![0xff, b't', b'O', b'c'];
        idx.extend(2u32.to_be_bytes());
        for b in 0..256 {
            let n = entries.iter().filter(|(id, _)| usize::from(id[0]) <= b).count() as u32;
            idx.extend(n.to_be_bytes());
        }
        for (id, _) in &entries {
            idx.extend(id);
        }
        idx.extend(vec![0u8; 4 * entries.len()]);
        for (_, off) in &entries {
            idx.extend((*off as u32).to_be_bytes());
        }
        write(&git.join("objects/pack/pack-test.idx"), &idx);
    }

    fn id(fill: u8, len: usize) -> Vec<u8> {
        let mut v = vec![fill; len];
        v[len - 1] = fill.wrapping_add(1);
        v
    }

    #[test]
    fn objet_isole_et_branches() {
        let d = tempfile::tempdir().unwrap();
        let git = d.path().join(".git");
        let (a, b) = ("1".repeat(40), "2".repeat(40));
        write(&git.join("HEAD"), b"ref: refs/heads/main\n");
        write(&git.join("refs/heads/main"), format!("{a}\n").as_bytes());
        // Branche plus récente que HEAD, dans packed-refs : c'est elle qui compte.
        write(&git.join("packed-refs"), format!("# pack-refs with: peeled\n{b} refs/heads/travail\n^{a}\n").as_bytes());
        loose(&git, &a, &commit(1_700_000_000, "ancien"));
        loose(&git, &b, &commit(1_741_064_767, "récent"));
        assert_eq!(last_commit(&git), Ok(1_741_064_767));
    }

    #[test]
    fn packfile_avec_deltas_sha1_et_sha256() {
        for hash_len in [20, 32] {
            let d = tempfile::tempdir().unwrap();
            let git = d.path().join(".git");
            let (base, ofs, refd) = (id(0x10, hash_len), id(0x80, hash_len), id(0xf0, hash_len));
            let bodies = [commit(1_600_000_000, "base"), commit(1_650_000_000, "via OFS_DELTA"), commit(1_741_064_767, "via REF_DELTA")];
            pack(&git, &base, &ofs, &refd, [&bodies[0], &bodies[1], &bodies[2]]);
            write(&git.join("HEAD"), format!("{}\n", hex(&refd)).as_bytes());
            assert_eq!(last_commit(&git), Ok(1_741_064_767), "{hash_len}");
            let repo = Repo::open(&git).unwrap();
            assert_eq!(repo.object(&hex(&ofs), 0).unwrap().1, bodies[1]);
        }
    }

    #[test]
    fn worktree_et_depots_incomplets() {
        let d = tempfile::tempdir().unwrap();
        let main = d.path().join("principal/.git");
        let a = "3".repeat(40);
        write(&main.join("HEAD"), b"ref: refs/heads/main\n");
        write(&main.join("refs/heads/main"), a.as_bytes());
        loose(&main, &a, &commit(1_700_000_123, "x"));
        // Worktree : `.git` est un fichier, ses références communes sont dans le dépôt principal.
        let wt = main.join("worktrees/wt");
        write(&wt.join("HEAD"), b"ref: refs/heads/main\n");
        write(&wt.join("commondir"), b"../..\n");
        let entry = d.path().join("wt/.git");
        write(&entry, format!("gitdir: {}\n", wt.display()).as_bytes());
        assert_eq!(last_commit(&entry), Ok(1_700_000_123));

        let empty = d.path().join("vide/.git");
        write(&empty.join("HEAD"), b"ref: refs/heads/main\n");
        assert_eq!(last_commit(&empty), Err(GitError::Unborn));
        assert_eq!(last_commit(&d.path().join("rien/.git")), Err(GitError::NotARepo));
        let missing = d.path().join("absent/.git");
        write(&missing.join("HEAD"), format!("{}\n", "4".repeat(40)).as_bytes());
        assert!(matches!(last_commit(&missing), Err(GitError::Missing(_))));
        // Référence qui sort du dépôt : ignorée.
        let evil = d.path().join("evil/.git");
        write(&evil.join("HEAD"), b"ref: refs/../../../secret\n");
        assert_eq!(last_commit(&evil), Err(GitError::Unborn));
    }

    #[test]
    fn deltas_invalides_refuses() {
        let base = b"abcdef";
        assert_eq!(apply_delta(base, &[6, 3, 0x91, 0, 3]).unwrap(), b"abc");
        assert!(apply_delta(base, &[5, 3, 0x91, 0, 3]).is_err(), "taille de base fausse");
        assert!(apply_delta(base, &[6, 3, 0x91, 4, 9]).is_err(), "copie hors de la base");
        assert!(apply_delta(base, &[6, 9, 2, b'x']).is_err(), "insertion tronquée");
        assert!(apply_delta(base, &[6, 1, 0]).is_err(), "opération nulle");
    }

    /// Dépôts réels créés par la CI avec git (objets isolés, packs, SHA-256, worktree) :
    /// `LUMEN_TEST_GIT` = « chemin=date » séparés par `;`.
    #[test]
    fn depots_reels_crees_par_git() {
        let Ok(list) = std::env::var("LUMEN_TEST_GIT") else {
            assert!(std::env::var("CI").is_err() || std::env::var("LUMEN_TEST_GIT_OPTIONAL").is_ok(), "la CI doit fournir LUMEN_TEST_GIT");
            return;
        };
        for item in list.split(';').filter(|s| !s.is_empty()) {
            let (path, expected) = item.rsplit_once('=').unwrap();
            assert_eq!(last_commit(Path::new(path)), Ok(expected.trim().parse::<i64>().unwrap()), "{path}");
        }
    }

    /// Lire `.git` ne touche pas à la date de dernier accès de ses fichiers (fichiers dormants).
    #[cfg(windows)]
    #[test]
    fn lecture_sans_toucher_au_dernier_acces() {
        use std::fs::{FileTimes, OpenOptions};
        use std::time::{Duration, UNIX_EPOCH};
        let base = std::env::var("LUMEN_TEST_NTFS").ok();
        let d = match &base {
            Some(v) => tempfile::tempdir_in(v),
            None => tempfile::tempdir(),
        }
        .unwrap();
        let git = d.path().join(".git");
        let a = "5".repeat(40);
        write(&git.join("HEAD"), b"ref: refs/heads/main\n");
        write(&git.join("refs/heads/main"), a.as_bytes());
        loose(&git, &a, &commit(1_741_064_767, "x"));
        let control = d.path().join("temoin.txt");
        write(&control, b"temoin");
        let watched = [git.join("HEAD"), git.join("refs/heads/main"), git.join("objects").join(&a[..2]).join(&a[2..])];
        let old = UNIX_EPOCH + Duration::from_secs(1_577_836_800);
        let accessed = |p: &Path| fs::metadata(p).unwrap().accessed().unwrap();
        for p in watched.iter().chain([&control]) {
            OpenOptions::new().write(true).open(p).unwrap().set_times(FileTimes::new().set_accessed(old)).unwrap();
            assert_eq!(accessed(p), old);
        }
        let _ = fs::read(&control).unwrap();
        if accessed(&control) == old {
            assert!(std::env::var("CI").is_err(), "la CI doit fournir un volume où le dernier accès est tenu à jour (LUMEN_TEST_NTFS)");
            eprintln!("ignoré : Windows ne tient pas la date de dernier accès à jour sur ce volume");
            return;
        }
        assert_eq!(last_commit(&git), Ok(1_741_064_767));
        for p in &watched {
            assert_eq!(accessed(p), old, "dernier accès de {} modifié", p.display());
        }
    }
}
