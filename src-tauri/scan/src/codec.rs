//! Format binaire d'une analyse, pour la garder entre deux lancements (la base SQLite de l'appli
//! le compresse). Petit-boutiste, versionné ; le décodage vérifie tout (bornes, noms, parents,
//! enfants contigus) et refuse un enregistrement abîmé au lieu de paniquer. Les file ID gardés
//! ne servent qu'à l'affichage et à la recherche : `safety` revérifie tout au moment d'agir.

use std::collections::HashMap;

use crate::model::{Kind, Method, Node, NodeId, Scan, Stats};

const MAGIC: &[u8; 8] = b"LUMSCAN\0";
const VERSION: u32 = 1;
/// Octets d'un nœud encodé.
const NODE_BYTES: usize = 4 + 4 + 4 + 4 + 2 + 1 + 1 + 4 + 8 + 8 + 8 + 8 + 8 + 4;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("enregistrement tronqué")]
    Truncated,
    #[error("ce n'est pas une analyse de Lumen")]
    BadMagic,
    #[error("version d'enregistrement {0} inconnue")]
    Version(u32),
    #[error("enregistrement incohérent : {0}")]
    Invalid(&'static str),
}

struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) {
        self.u64(s.len() as u64);
        self.0.extend_from_slice(s.as_bytes());
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.at.checked_add(n).ok_or(DecodeError::Truncated)?;
        let s = self.b.get(self.at..end).ok_or(DecodeError::Truncated)?;
        self.at = end;
        Ok(s)
    }
    fn arr<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.arr::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.arr()?))
    }
    fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.arr()?))
    }
    fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.arr()?))
    }
    fn i64(&mut self) -> Result<i64, DecodeError> {
        Ok(i64::from_le_bytes(self.arr()?))
    }
    /// Longueur annoncée, bornée par ce qui reste à lire (jamais d'allocation démesurée).
    fn len(&mut self, unit: usize) -> Result<usize, DecodeError> {
        let n = usize::try_from(self.u64()?).map_err(|_| DecodeError::Truncated)?;
        if n.checked_mul(unit.max(1)).map_or(true, |bytes| bytes > self.b.len() - self.at) {
            return Err(DecodeError::Truncated);
        }
        Ok(n)
    }
    fn str(&mut self) -> Result<String, DecodeError> {
        let n = self.len(1)?;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| DecodeError::Invalid("texte non UTF-8"))
    }
}

fn kind_of(v: u8) -> Result<Kind, DecodeError> {
    Ok(match v {
        0 => Kind::Dir,
        1 => Kind::File,
        2 => Kind::Link,
        3 => Kind::CloudFile,
        4 => Kind::CloudDir,
        _ => return Err(DecodeError::Invalid("type d'élément inconnu")),
    })
}

impl Scan {
    /// Analyse encodée (non compressée).
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer(Vec::with_capacity(64 + self.nodes.len() * NODE_BYTES + self.names.len()));
        w.0.extend_from_slice(MAGIC);
        w.u32(VERSION);
        w.str(&self.root_path);
        w.u64(self.volume_serial);
        w.u8(match self.method {
            Method::Walk => 0,
            Method::Mft => 1,
        });
        w.u8(u8::from(self.last_access_reliable));
        let s = &self.stats;
        for v in [s.files, s.dirs, s.links, s.cloud_files, s.cloud_dirs, s.cloud_bytes, s.hardlink_dups, s.access_denied, s.locked, s.other_errors, s.allocated, s.logical, s.elapsed_ms] {
            w.u64(v);
        }
        w.u64(s.error_samples.len() as u64);
        for e in &s.error_samples {
            w.str(e);
        }
        w.u64(self.nodes.len() as u64);
        for n in &self.nodes {
            w.u32(n.parent);
            w.u32(n.first_child);
            w.u32(n.child_count);
            w.u32(n.name_off);
            w.u16(n.name_len);
            w.u8(n.kind as u8);
            w.u8(n.flags);
            w.u32(n.attributes);
            w.u64(n.file_id);
            w.u64(n.logical);
            w.u64(n.allocated);
            w.i64(n.modified);
            w.i64(n.accessed);
            w.u32(n.files);
        }
        w.str(&self.names);
        let mut high: Vec<(&NodeId, &u64)> = self.id_high.iter().collect();
        high.sort();
        w.u64(high.len() as u64);
        for (id, v) in high {
            w.u32(*id);
            w.u64(*v);
        }
        w.0
    }

    /// Analyse décodée et vérifiée.
    pub fn decode(bytes: &[u8]) -> Result<Scan, DecodeError> {
        let mut r = Reader { b: bytes, at: 0 };
        if r.take(MAGIC.len())? != MAGIC {
            return Err(DecodeError::BadMagic);
        }
        let version = r.u32()?;
        if version != VERSION {
            return Err(DecodeError::Version(version));
        }
        let root_path = r.str()?;
        let volume_serial = r.u64()?;
        let method = match r.u8()? {
            0 => Method::Walk,
            1 => Method::Mft,
            _ => return Err(DecodeError::Invalid("méthode inconnue")),
        };
        let last_access_reliable = r.u8()? != 0;
        let mut v = [0u64; 13];
        for x in &mut v {
            *x = r.u64()?;
        }
        let samples = r.len(8)?;
        let mut error_samples = Vec::with_capacity(samples);
        for _ in 0..samples {
            error_samples.push(r.str()?);
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
        let count = r.len(NODE_BYTES)?;
        if count == 0 || count > u32::MAX as usize {
            return Err(DecodeError::Invalid("nombre d'éléments"));
        }
        let mut nodes = Vec::with_capacity(count);
        for _ in 0..count {
            nodes.push(Node {
                parent: r.u32()?,
                first_child: r.u32()?,
                child_count: r.u32()?,
                name_off: r.u32()?,
                name_len: r.u16()?,
                kind: kind_of(r.u8()?)?,
                flags: r.u8()?,
                attributes: r.u32()?,
                file_id: r.u64()?,
                logical: r.u64()?,
                allocated: r.u64()?,
                modified: r.i64()?,
                accessed: r.i64()?,
                files: r.u32()?,
            });
        }
        let names = r.str()?;
        let high = r.len(12)?;
        let mut id_high = HashMap::with_capacity(high);
        for _ in 0..high {
            let id = r.u32()?;
            let value = r.u64()?;
            if id as usize >= count {
                return Err(DecodeError::Invalid("file ID d'un élément absent"));
            }
            id_high.insert(id, value);
        }
        if r.at != bytes.len() {
            return Err(DecodeError::Invalid("octets en trop"));
        }
        // Arène cohérente : noms dans le texte, parents avant leurs enfants, enfants contigus.
        for (i, n) in nodes.iter().enumerate() {
            let start = n.name_off as usize;
            let end = start + n.name_len as usize;
            if end > names.len() || !names.is_char_boundary(start) || !names.is_char_boundary(end) {
                return Err(DecodeError::Invalid("nom hors limites"));
            }
            if i > 0 && n.parent as usize >= i {
                return Err(DecodeError::Invalid("parent après son enfant"));
            }
            if n.child_count > 0 {
                let first = n.first_child as usize;
                let last = first.checked_add(n.child_count as usize).ok_or(DecodeError::Invalid("enfants"))?;
                if first <= i || last > count || nodes[first..last].iter().any(|c| c.parent as usize != i) {
                    return Err(DecodeError::Invalid("enfants incohérents"));
                }
            }
        }
        Ok(Scan { root_path, volume_serial, method, last_access_reliable, nodes, names, id_high, stats })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Builder;
    use crate::RawEntry;

    fn e(name: &str, kind: Kind, size: u64, id: u128) -> RawEntry {
        RawEntry { name: name.into(), kind, attributes: 0x20, file_id: id, logical: size, allocated: size + 1, modified: 133_000_000_000_000_000, accessed: 133_100_000_000_000_000 }
    }

    fn sample() -> Scan {
        let mut b = Builder::new(&e("C:", Kind::Dir, 0, 5));
        let top = b.add_children(0, &[e("Vidéos", Kind::Dir, 0, 6), e("pagefile.sys", Kind::File, 8 << 30, 7), e("OneDrive", Kind::CloudDir, 0, 8)]);
        b.add_children(top.start, &[e("été 2026.mp4", Kind::File, 3 << 30, (1u128 << 64) | 9), e("raccourci", Kind::Link, 0, 10)]);
        let mut stats = Stats { access_denied: 2, elapsed_ms: 1234, ..Stats::default() };
        stats.error_samples.push("C:\\Système\\x".into());
        b.finish("C:\\".into(), 0xABCD, Method::Mft, true, stats)
    }

    #[test]
    fn aller_retour_identique() {
        let scan = sample();
        let back = Scan::decode(&scan.encode()).unwrap();
        assert_eq!(back.encode(), scan.encode());
        assert_eq!((back.len(), back.root_path.as_str(), back.method, back.last_access_reliable), (scan.len(), "C:\\", Method::Mft, true));
        assert_eq!(back.path(back.find("Vidéos\\été 2026.mp4").unwrap()), "C:\\Vidéos\\été 2026.mp4");
        assert_eq!(back.file_id(back.find("Vidéos\\été 2026.mp4").unwrap()), (1u128 << 64) | 9);
        assert_eq!(back.stats.error_samples, ["C:\\Système\\x"]);
    }

    #[test]
    fn enregistrements_abimes_refuses_sans_paniquer() {
        let good = sample().encode();
        // Tronqué à chaque longueur possible : toujours une erreur, jamais une panique.
        for n in 0..good.len() {
            assert!(Scan::decode(&good[..n]).is_err(), "{n}");
        }
        assert_eq!(Scan::decode(b"PASUNSCAN").err(), Some(DecodeError::BadMagic));
        let mut v2 = good.clone();
        v2[8] = 9;
        assert_eq!(Scan::decode(&v2).err(), Some(DecodeError::Version(9)));
        // Octets modifiés au hasard (déterministe) : erreur ou analyse cohérente, jamais de panique.
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        for _ in 0..2000 {
            let mut bad = good.clone();
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let i = (seed % bad.len() as u64) as usize;
            bad[i] ^= (seed >> 32) as u8 | 1;
            if let Ok(scan) = Scan::decode(&bad) {
                for id in 0..scan.len() as NodeId {
                    let _ = scan.path(id);
                }
            }
        }
        // Longueur annoncée démesurée : refusée avant toute allocation.
        let mut huge = good.clone();
        huge[12..20].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(Scan::decode(&huge).is_err());
    }
}
