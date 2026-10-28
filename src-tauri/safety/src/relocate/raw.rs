//! Structures binaires de Windows utilisées par le déplacement, lues et écrites en logique pure
//! (testées partout) : tampon de point de montage (jonction), liste d'accès (ACL), noms de flux.

use crate::path::WinPath;

/// `IO_REPARSE_TAG_MOUNT_POINT` : jonction.
pub const TAG_MOUNT_POINT: u32 = 0xA000_0003;
/// En-tête du tampon (tag, longueur, réservé) puis les 4 champs de MountPointReparseBuffer.
const HEADER: usize = 8;
const FIELDS: usize = 8;
/// Taille maximale d'un tampon de point d'analyse (`MAXIMUM_REPARSE_DATA_BUFFER_SIZE`).
pub const MAX_BUFFER: usize = 16 * 1024;

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    b.get(o..o + 2).map(|x| u16::from_le_bytes([x[0], x[1]]))
}

/// Tampon `FSCTL_SET_REPARSE_POINT` d'une jonction vers `target` (dossier sur un disque local) :
/// nom de substitution `\??\D:\…`, nom affiché `D:\…`.
pub fn mount_point_buffer(target: &WinPath) -> Option<Vec<u8>> {
    if !matches!(target.prefix(), crate::path::Prefix::Drive(_)) || target.is_volume_root() {
        return None;
    }
    let print = target.display_string();
    let sub = utf16(&format!("\\??\\{print}"));
    let print = utf16(&print);
    let data_len = FIELDS + sub.len() + 2 + print.len() + 2;
    if HEADER + data_len > MAX_BUFFER {
        return None;
    }
    let mut b = Vec::with_capacity(HEADER + data_len);
    b.extend_from_slice(&TAG_MOUNT_POINT.to_le_bytes());
    b.extend_from_slice(&(data_len as u16).to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes());
    b.extend_from_slice(&(sub.len() as u16).to_le_bytes());
    b.extend_from_slice(&((sub.len() + 2) as u16).to_le_bytes());
    b.extend_from_slice(&(print.len() as u16).to_le_bytes());
    b.extend_from_slice(&sub);
    b.extend_from_slice(&[0, 0]);
    b.extend_from_slice(&print);
    b.extend_from_slice(&[0, 0]);
    Some(b)
}

/// Cible d'une jonction lue par `FSCTL_GET_REPARSE_POINT` ; `None` pour tout autre point
/// d'analyse (lien symbolique, OneDrive…), une cible par GUID de volume ou un tampon abîmé.
pub fn parse_mount_point(b: &[u8]) -> Option<WinPath> {
    let tag = b.get(0..4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))?;
    if tag != TAG_MOUNT_POINT {
        return None;
    }
    let data_len = usize::from(u16_at(b, 4)?);
    let data = b.get(HEADER..HEADER + data_len)?;
    let (off, len) = (usize::from(u16_at(data, 0)?), usize::from(u16_at(data, 2)?));
    let name = data.get(FIELDS + off..FIELDS + off + len)?;
    if len % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = name.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let s = String::from_utf16(&units).ok()?;
    let rest = s.strip_prefix("\\??\\")?;
    // Un lecteur et un chemin : jamais `Volume{…}` ni un périphérique.
    let bytes = rest.as_bytes();
    if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' || bytes[2] != b'\\' {
        return None;
    }
    WinPath::parse(rest.trim_end_matches('\\')).ok().or_else(|| WinPath::parse(rest).ok())
}

/// `INHERITED_ACE` dans `AceFlags`.
const INHERITED_ACE: u8 = 0x10;

/// Copie d'une ACL où chaque entrée héritée devient explicite : posée en ACL protégée sur le
/// dossier déplacé, elle garde ses droits effectifs sans hériter du nouveau parent.
pub fn explicit_acl(acl: &[u8]) -> Option<Vec<u8>> {
    let size = usize::from(u16_at(acl, 2)?);
    let count = usize::from(u16_at(acl, 4)?);
    let mut out = acl.get(..size)?.to_vec();
    let mut off = 8;
    for _ in 0..count {
        let ace_size = usize::from(u16_at(&out, off + 2)?);
        if ace_size < 4 || off + ace_size > size {
            return None;
        }
        out[off + 1] &= !INHERITED_ACE;
        off += ace_size;
    }
    Some(out)
}

/// Flux nommé à copier (`:nom:$DATA`) ; `None` pour le flux principal (`::$DATA`).
pub fn named_stream(raw: &str) -> Option<&str> {
    (raw != "::$DATA" && raw.starts_with(':') && raw.len() > ":$DATA".len() + 1 && raw.ends_with(":$DATA")).then_some(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jonction_aller_retour() {
        for raw in ["D:\\Lumen\\Déplacés\\C\\Users\\Arthur\\Vidéos", "E:\\x"] {
            let t = WinPath::parse(raw).unwrap();
            let b = mount_point_buffer(&t).unwrap();
            assert_eq!(parse_mount_point(&b), Some(t));
        }
        assert_eq!(mount_point_buffer(&WinPath::parse("D:\\").unwrap()), None);
        assert_eq!(mount_point_buffer(&WinPath::parse("\\\\nas\\partage\\x").unwrap()), None, "jamais un lecteur réseau");
        let long = format!("D:\\{}", vec!["a".repeat(200); 50].join("\\"));
        assert_eq!(mount_point_buffer(&WinPath::parse(&long).unwrap()), None, "tampon trop grand");
    }

    #[test]
    fn autres_points_d_analyse_ignores() {
        let t = WinPath::parse("D:\\x").unwrap();
        let mut b = mount_point_buffer(&t).unwrap();
        // Lien symbolique (IO_REPARSE_TAG_SYMLINK) : pas une jonction de Lumen.
        b[..4].copy_from_slice(&0xA000_000Cu32.to_le_bytes());
        assert_eq!(parse_mount_point(&b), None);
        // Cible par GUID de volume.
        let sub = utf16("\\??\\Volume{0b1c2d3e-0000-0000-0000-000000000000}\\x");
        let mut v = TAG_MOUNT_POINT.to_le_bytes().to_vec();
        v.extend_from_slice(&((FIELDS + sub.len() + 4) as u16).to_le_bytes());
        v.extend_from_slice(&[0, 0, 0, 0]);
        v.extend_from_slice(&(sub.len() as u16).to_le_bytes());
        v.extend_from_slice(&[0; 4]);
        v.extend_from_slice(&sub);
        v.extend_from_slice(&[0; 4]);
        assert_eq!(parse_mount_point(&v), None);
        // Tampons tronqués : jamais de panique.
        let good = mount_point_buffer(&t).unwrap();
        for n in 0..good.len() {
            let _ = parse_mount_point(&good[..n]);
        }
    }

    #[test]
    fn acl_heritee_rendue_explicite() {
        // ACL : en-tête (révision 2, taille, 2 entrées), une entrée explicite, une héritée.
        let ace = |flags: u8| {
            let mut a = vec![0u8, flags, 20, 0];
            a.extend_from_slice(&[0xFF; 16]);
            a
        };
        let mut acl = vec![2, 0, 48, 0, 2, 0, 0, 0];
        acl.extend(ace(0x03));
        acl.extend(ace(0x13));
        let out = explicit_acl(&acl).unwrap();
        assert_eq!((out[9], out[29]), (0x03, 0x03));
        assert_eq!(out.len(), 48);
        let mut bad = acl.clone();
        bad[30] = 200;
        assert_eq!(explicit_acl(&bad), None);
        assert_eq!(explicit_acl(&acl[..20]), None);
    }

    #[test]
    fn flux_nommes() {
        assert_eq!(named_stream("::$DATA"), None);
        assert_eq!(named_stream(":Zone.Identifier:$DATA"), Some(":Zone.Identifier:$DATA"));
        assert_eq!(named_stream(":$DATA"), None);
    }
}
