//! Recherche instantanée dans une analyse : **noms seulement**, jamais le contenu des fichiers.
//! Index compact (noms pliés — minuscules, sans accents — mis bout à bout, et un masque des
//! caractères présents par nom pour écarter d'un coup ce qui ne peut pas correspondre), requête
//! floue (lettres dans l'ordre, bonus aux débuts de mot et aux suites) avec filtres :
//! `ext:mp4`, `>1go`, `<500mo`, `type:vidéo`, `type:dossier`, `disque:D`.

use crate::kinds::FileType;
use crate::model::{Kind, NodeId, Scan};

/// Noms pliés de toute une analyse.
#[derive(Debug, Default)]
pub struct Index {
    folded: Vec<u8>,
    /// Début de chaque nom dans `folded` (un de plus que de nœuds).
    offs: Vec<u32>,
    masks: Vec<u64>,
}

/// Lettre pliée : minuscule, accents retirés pour l'alphabet latin courant.
fn fold_into(c: char, out: &mut Vec<u8>) {
    let ascii = |s: &str, out: &mut Vec<u8>| out.extend_from_slice(s.as_bytes());
    for l in c.to_lowercase() {
        match l {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => out.push(b'a'),
            'ç' | 'ć' | 'č' => out.push(b'c'),
            'ď' | 'đ' => out.push(b'd'),
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => out.push(b'e'),
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' => out.push(b'i'),
            'ł' | 'ľ' => out.push(b'l'),
            'ñ' | 'ń' | 'ň' => out.push(b'n'),
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => out.push(b'o'),
            'ř' => out.push(b'r'),
            'ś' | 'š' | 'ş' => out.push(b's'),
            'ť' | 'ţ' => out.push(b't'),
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' => out.push(b'u'),
            'ý' | 'ÿ' => out.push(b'y'),
            'ź' | 'ż' | 'ž' => out.push(b'z'),
            'ß' => ascii("ss", out),
            'œ' => ascii("oe", out),
            'æ' => ascii("ae", out),
            other => {
                let mut buf = [0u8; 4];
                out.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
            }
        }
    }
}

/// Texte plié (requête ou nom).
pub fn fold(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        fold_into(c, &mut out);
    }
    out
}

/// Masque des octets présents : un bit par lettre et chiffre ASCII, le reste réparti sur 28 bits.
fn mask(bytes: &[u8]) -> u64 {
    let mut m = 0u64;
    for &b in bytes {
        let bit = match b {
            b'a'..=b'z' => b - b'a',
            b'0'..=b'9' => 26 + (b - b'0'),
            other => 36 + other % 28,
        };
        m |= 1 << bit;
    }
    m
}

impl Index {
    pub fn build(scan: &Scan) -> Index {
        let n = scan.len();
        let mut idx = Index { folded: Vec::with_capacity(scan.names.len() + n / 8), offs: Vec::with_capacity(n + 1), masks: Vec::with_capacity(n) };
        for id in 0..n as NodeId {
            let start = idx.folded.len();
            idx.offs.push(start as u32);
            for c in scan.name(id).chars() {
                fold_into(c, &mut idx.folded);
            }
            idx.masks.push(mask(&idx.folded[start..]));
        }
        idx.offs.push(idx.folded.len() as u32);
        idx
    }

    fn name(&self, id: usize) -> &[u8] {
        let (a, b) = (self.offs[id] as usize, self.offs[id + 1] as usize);
        &self.folded[a..b]
    }

    /// Mémoire occupée (octets).
    pub fn memory_bytes(&self) -> u64 {
        (self.folded.capacity() + self.offs.capacity() * 4 + self.masks.capacity() * 8) as u64
    }
}

/// Requête analysée.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// Mots à trouver dans le nom (pliés), tous.
    pub words: Vec<Vec<u8>>,
    /// Extensions acceptées (pliées, sans point).
    pub exts: Vec<Vec<u8>>,
    pub min_size: Option<u64>,
    pub max_size: Option<u64>,
    pub types: Vec<FileType>,
    /// `type:dossier` : seulement les dossiers.
    pub dirs_only: bool,
    /// `disque:D` : lettres acceptées (majuscules).
    pub drives: Vec<char>,
}

impl Query {
    pub fn is_empty(&self) -> bool {
        self.words.is_empty() && self.exts.is_empty() && self.min_size.is_none() && self.max_size.is_none() && self.types.is_empty() && !self.dirs_only && self.drives.is_empty()
    }
}

/// `1go`, `1,5 Go`, `500mo`, `2gb`, `10k` en octets (base 1024, comme l'Explorateur).
pub fn parse_size(raw: &str) -> Option<u64> {
    let s = raw.trim().to_lowercase().replace(',', ".");
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let value: f64 = num.parse().ok()?;
    let mult: f64 = match unit.trim() {
        "" | "o" | "b" | "octets" | "bytes" => 1.0,
        "k" | "ko" | "kb" | "kio" | "kib" => 1024.0,
        "m" | "mo" | "mb" | "mio" | "mib" => 1024f64.powi(2),
        "g" | "go" | "gb" | "gio" | "gib" => 1024f64.powi(3),
        "t" | "to" | "tb" | "tio" | "tib" => 1024f64.powi(4),
        _ => return None,
    };
    (value.is_finite() && value >= 0.0).then(|| (value * mult).round() as u64)
}

fn file_type(word: &str) -> Option<Result<FileType, ()>> {
    Some(Ok(match String::from_utf8_lossy(&fold(word)).as_ref() {
        "video" | "videos" | "film" | "films" => FileType::Video,
        "image" | "images" | "photo" | "photos" => FileType::Image,
        "audio" | "musique" | "music" | "son" => FileType::Audio,
        "archive" | "archives" => FileType::Archive,
        "disque" | "disk" | "iso" | "image-disque" => FileType::Disk,
        "installeur" | "installer" | "programme" => FileType::Installer,
        "document" | "documents" | "doc" => FileType::Document,
        "dossier" | "dossiers" | "folder" | "dir" => return Some(Err(())),
        _ => return None,
    }))
}

/// Requête de l'utilisateur ; un filtre illisible reste un mot à chercher.
pub fn parse(q: &str) -> Query {
    let mut out = Query::default();
    for token in q.split_whitespace() {
        let lower = token.to_lowercase();
        let (key, value) = lower.split_once(':').map_or(("", lower.as_str()), |(k, v)| (k, v));
        let handled = match key {
            "ext" | "extension" => {
                out.exts.extend(value.split(',').map(|e| fold(e.trim_start_matches('.'))).filter(|e| !e.is_empty()));
                true
            }
            "type" => match file_type(value) {
                Some(Ok(t)) => {
                    out.types.push(t);
                    true
                }
                Some(Err(())) => {
                    out.dirs_only = true;
                    true
                }
                None => false,
            },
            "disque" | "drive" | "lecteur" | "disk" => match value.trim_end_matches([':', '\\']).chars().collect::<Vec<_>>().as_slice() {
                [c] if c.is_ascii_alphabetic() => {
                    out.drives.push(c.to_ascii_uppercase());
                    true
                }
                _ => false,
            },
            _ => {
                let size = |s: &str| parse_size(s);
                if let Some(rest) = lower.strip_prefix(">=").or_else(|| lower.strip_prefix('>')) {
                    size(rest).map(|v| out.min_size = Some(v)).is_some()
                } else if let Some(rest) = lower.strip_prefix("<=").or_else(|| lower.strip_prefix('<')) {
                    size(rest).map(|v| out.max_size = Some(v)).is_some()
                } else {
                    false
                }
            }
        };
        if !handled {
            let w = fold(token);
            if !w.is_empty() {
                out.words.push(w);
            }
        }
    }
    out
}

fn boundary(b: u8) -> bool {
    matches!(b, b' ' | b'.' | b'_' | b'-' | b'(' | b')' | b'[' | b']' | b'\\' | b'/' | b',' | b'+')
}

/// Score d'un mot dans un nom : `None` si ses lettres n'y sont pas dans l'ordre.
fn score_word(q: &[u8], t: &[u8]) -> Option<i64> {
    if q.is_empty() {
        return Some(0);
    }
    let mut qi = 0;
    let mut score = 0i64;
    let mut prev: Option<usize> = None;
    for (i, &b) in t.iter().enumerate() {
        if qi < q.len() && b == q[qi] {
            score += 1;
            if prev == Some(i.wrapping_sub(1)) && i > 0 {
                score += 3;
            }
            if i == 0 || boundary(t[i - 1]) {
                score += 2;
            }
            prev = Some(i);
            qi += 1;
        }
    }
    if qi < q.len() {
        return None;
    }
    // Mot entier présent tel quel : nettement devant ; nom identique ou qui commence par lui, encore plus.
    if let Some(pos) = t.windows(q.len()).position(|w| w == q) {
        score += 10 + if pos == 0 { 6 } else { 0 } + if t.len() == q.len() { 10 } else { 0 };
    }
    Some(score * 100 - t.len() as i64)
}

/// Résultat dans une analyse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hit {
    pub id: NodeId,
    pub score: i64,
}

/// Les `limit` meilleurs résultats d'une analyse (racine exclue), du meilleur au moins bon ;
/// à score égal, le plus gros d'abord. Sans mot, les plus gros qui passent les filtres.
pub fn search(scan: &Scan, idx: &Index, q: &Query, limit: usize) -> Vec<Hit> {
    use rayon::prelude::*;
    if limit == 0 || q.is_empty() || idx.masks.len() != scan.len() {
        return Vec::new();
    }
    let size = |id: usize| scan.nodes[id].allocated;
    // Tranches parcourues en parallèle, chacune garde ses meilleurs ; puis fusion.
    const CHUNK: usize = 1 << 16;
    let chunks: Vec<usize> = (0..scan.len().div_ceil(CHUNK)).collect();
    let mut hits: Vec<Hit> = chunks
        .par_iter()
        .map(|&c| {
            let start = (c * CHUNK).max(1);
            let end = ((c + 1) * CHUNK).min(scan.len());
            let mut local = Vec::new();
            scan_range(scan, idx, q, start..end, limit, &mut local);
            local
        })
        .reduce(Vec::new, |mut a, b| {
            a.extend(b);
            a
        });
    trim(&mut hits, limit, &size);
    hits
}

fn scan_range(scan: &Scan, idx: &Index, q: &Query, range: std::ops::Range<usize>, limit: usize, hits: &mut Vec<Hit>) {
    let need = q.words.iter().fold(0u64, |m, w| m | mask(w));
    let size = |id: usize| scan.nodes[id].allocated;
    for id in range {
        if idx.masks[id] & need != need {
            continue;
        }
        let node = &scan.nodes[id];
        let is_dir = matches!(node.kind, Kind::Dir | Kind::CloudDir);
        if q.dirs_only && !is_dir {
            continue;
        }
        if q.min_size.is_some_and(|m| node.allocated < m) || q.max_size.is_some_and(|m| node.allocated > m) {
            continue;
        }
        let name = idx.name(id);
        if !q.exts.is_empty() || !q.types.is_empty() {
            if is_dir {
                continue;
            }
            let ext = name.iter().rposition(|&b| b == b'.').map_or(&[][..], |p| &name[p + 1..]);
            if !q.exts.is_empty() && !q.exts.iter().any(|e| e.as_slice() == ext) {
                continue;
            }
            if !q.types.is_empty() && !q.types.contains(&FileType::of_ext(&String::from_utf8_lossy(ext))) {
                continue;
            }
        }
        let mut total = 0i64;
        let mut ok = true;
        for w in &q.words {
            match score_word(w, name) {
                Some(s) => total += s,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        hits.push(Hit { id: id as NodeId, score: total });
        // Garde la liste courte : les meilleurs seulement.
        if hits.len() >= limit * 8 + 64 {
            trim(hits, limit, &size);
        }
    }
    trim(hits, limit, &size);
}

fn trim(hits: &mut Vec<Hit>, limit: usize, size: &dyn Fn(usize) -> u64) {
    hits.sort_unstable_by(|a, b| b.score.cmp(&a.score).then_with(|| size(b.id as usize).cmp(&size(a.id as usize))).then_with(|| a.id.cmp(&b.id)));
    hits.truncate(limit);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Builder;
    use crate::{Method, RawEntry, Stats};

    fn e(name: &str, kind: Kind, size: u64) -> RawEntry {
        RawEntry { name: name.into(), kind, attributes: 0, file_id: 0, logical: size, allocated: size, modified: 0, accessed: 0 }
    }

    const MB: u64 = 1 << 20;
    const GB: u64 = 1 << 30;

    fn sample() -> Scan {
        let mut b = Builder::new(&e("D:", Kind::Dir, 0));
        let top = b.add_children(0, &[e("Vidéos", Kind::Dir, 0), e("Projets", Kind::Dir, 0), e("Musique", Kind::Dir, 0)]);
        b.add_children(
            top.start,
            &[
                e("Mariage Julie.mp4", Kind::File, 9 * GB),
                e("Été 2026 - montage.MKV", Kind::File, 3 * GB),
                e("bande annonce.mp4", Kind::File, 200 * MB),
                e("notes mariage.txt", Kind::File, 1024),
            ],
        );
        b.add_children(top.start + 1, &[e("hinageshi-web", Kind::Dir, 0), e("Cargo.toml", Kind::File, 300)]);
        b.add_children(top.start + 2, &[e("Ma playlist.flac", Kind::File, 40 * MB)]);
        b.finish("D:\\".into(), 1, Method::Walk, true, Stats::default())
    }

    fn names(scan: &Scan, q: &str) -> Vec<String> {
        let idx = Index::build(scan);
        search(scan, &idx, &parse(q), 10).iter().map(|h| scan.name(h.id).to_owned()).collect()
    }

    #[test]
    fn noms_flous_sans_accents_ni_casse() {
        let s = sample();
        assert_eq!(names(&s, "mariage"), ["Mariage Julie.mp4", "notes mariage.txt"]);
        assert_eq!(names(&s, "ete montage"), ["Été 2026 - montage.MKV"]);
        assert_eq!(names(&s, "VIDEOS"), ["Vidéos"]);
        // Lettres dans l'ordre (sous-séquence) : « mrgjl » trouve « Mariage Julie ».
        assert_eq!(names(&s, "mrgjl"), ["Mariage Julie.mp4"]);
        assert!(names(&s, "zzz").is_empty());
        assert!(names(&s, "").is_empty());
    }

    #[test]
    fn filtres_extension_taille_type_dossier() {
        let s = sample();
        assert_eq!(names(&s, "ext:mp4"), ["Mariage Julie.mp4", "bande annonce.mp4"], "plus gros d'abord sans mot");
        assert_eq!(names(&s, "ext:mkv,flac"), ["Été 2026 - montage.MKV", "Ma playlist.flac"]);
        assert_eq!(names(&s, ">1go"), ["Vidéos", "Mariage Julie.mp4", "Été 2026 - montage.MKV"]);
        assert_eq!(names(&s, ">1go type:vidéo"), ["Mariage Julie.mp4", "Été 2026 - montage.MKV"]);
        assert_eq!(names(&s, "type:video <500mo"), ["bande annonce.mp4"]);
        assert_eq!(names(&s, "type:musique"), ["Ma playlist.flac"]);
        assert_eq!(names(&s, "type:dossier proj"), ["Projets"]);
        assert_eq!(names(&s, "ext:mp4 mariage"), ["Mariage Julie.mp4"]);
    }

    #[test]
    fn requete_analysee() {
        let q = parse("ext:.MP4 >=1,5 Go type:vidéo disque:d: été");
        assert_eq!(q.exts, [b"mp4".to_vec()]);
        // Taille sans unité collée : en octets (« Go » reste un mot).
        assert_eq!((q.min_size, q.words.contains(&b"go".to_vec())), (Some(2), true));
        let q = parse(">=1,5go <2tb disque:D type:dossier");
        assert_eq!((q.min_size, q.max_size, q.drives.as_slice(), q.dirs_only), (Some((1.5 * GB as f64) as u64), Some(2 << 40), &['D'][..], true));
        // Filtre illisible : reste un mot à chercher.
        let q = parse("type:inconnu >beaucoup");
        assert_eq!(q.words, [b"type:inconnu".to_vec(), b">beaucoup".to_vec()]);
        assert_eq!(parse_size("10k"), Some(10 * 1024));
        assert_eq!(parse_size("2 Mo"), Some(2 * MB));
        assert_eq!(parse_size("x"), None);
        assert_eq!(fold("Œuvre Été ß"), b"oeuvre ete ss");
    }
}
