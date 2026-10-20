//! Lecture directe de la MFT d'un volume NTFS (droits administrateur) : beaucoup plus rapide que
//! le parcours, et toujours sans ouvrir aucun fichier. Le résultat a exactement la même forme
//! (et, sur les mêmes fichiers, les mêmes totaux) que le parcours : même classement, même règle
//! de taille allouée, mêmes liens physiques comptés une fois.

use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Ioctl::{FSCTL_GET_NTFS_VOLUME_DATA, NTFS_VOLUME_DATA_BUFFER};
use windows::Win32::System::IO::DeviceIoControl;

use crate::classify::{classify, ATTR_DIRECTORY, ATTR_REPARSE, TAG_WOF};
use crate::model::{Builder, Kind, Method, RawEntry, Scan, Stats};
use crate::walk::{Progress, ScanError, PROGRESS_INTERVAL};

const ROOT_RECORD: u64 = 5;
/// Enregistrements 0 à 23 : fichiers de métadonnées NTFS ($MFT, $LogFile, $Extend…), invisibles
/// pour l'Explorateur comme pour le parcours.
const FIRST_USER_RECORD: u64 = 24;
const CHUNK: usize = 16 * 1024 * 1024;

fn err(msg: impl Into<String>) -> ScanError {
    ScanError::Mft(msg.into())
}

fn u16_at(b: &[u8], i: usize) -> u16 {
    b.get(i..i + 2).map_or(0, |s| u16::from_le_bytes([s[0], s[1]]))
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    b.get(i..i + 4).map_or(0, |s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn u64_at(b: &[u8], i: usize) -> u64 {
    b.get(i..i + 8).map_or(0, |s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
}

/// Applique le tableau de correction (« update sequence ») d'un enregistrement ; faux s'il est abîmé.
fn apply_fixup(rec: &mut [u8]) -> bool {
    let usa_off = u16_at(rec, 4) as usize;
    let usa_count = u16_at(rec, 6) as usize;
    if usa_count < 2 || usa_off + usa_count * 2 > rec.len() {
        return false;
    }
    let check = [rec[usa_off], rec[usa_off + 1]];
    for i in 1..usa_count {
        let end = i * 512;
        if end > rec.len() {
            break;
        }
        if rec[end - 2..end] != check {
            return false;
        }
        rec[end - 2] = rec[usa_off + i * 2];
        rec[end - 1] = rec[usa_off + i * 2 + 1];
    }
    true
}

/// Extents d'un attribut non résident : (LCN, nombre de clusters) ; LCN absent = trou.
fn data_runs(b: &[u8]) -> Vec<(Option<i64>, u64)> {
    let mut runs = Vec::new();
    let mut i = 0;
    let mut lcn: i64 = 0;
    while i < b.len() {
        let header = b[i];
        if header == 0 {
            break;
        }
        let len_size = (header & 0x0F) as usize;
        let off_size = (header >> 4) as usize;
        i += 1;
        if len_size == 0 || len_size > 8 || off_size > 8 || i + len_size + off_size > b.len() {
            break;
        }
        let mut length: u64 = 0;
        for k in 0..len_size {
            length |= u64::from(b[i + k]) << (8 * k);
        }
        i += len_size;
        if off_size == 0 {
            runs.push((None, length));
        } else {
            let mut delta: i64 = 0;
            for k in 0..off_size {
                delta |= i64::from(b[i + k]) << (8 * k);
            }
            // Extension du signe.
            let shift = 64 - 8 * off_size as u32;
            delta = (delta << shift) >> shift;
            lcn += delta;
            runs.push((Some(lcn), length));
        }
        i += off_size;
    }
    runs
}

/// Ce qu'on retient d'un enregistrement (de base, enrichi par ses enregistrements d'extension).
#[derive(Clone, Copy, Default)]
struct Rec {
    in_use: bool,
    dir: bool,
    seq: u16,
    attributes: u32,
    modified: i64,
    accessed: i64,
    logical: u64,
    allocated: u64,
    wof_allocated: Option<u64>,
    tag: u32,
}

struct Name {
    record: u32,
    parent: u32,
    parent_seq: u16,
    off: u32,
    len: u32,
}

#[derive(Default)]
struct Table {
    recs: Vec<Rec>,
    names: Vec<Name>,
    text: String,
}

/// Taille sur disque d'un attribut non résident : clusters réellement alloués s'il est compressé
/// ou clairsemé, sinon taille allouée.
fn nonresident_allocated(attr: &[u8]) -> u64 {
    let flags = u16_at(attr, 12);
    let attr_len = u32_at(attr, 4) as usize;
    if flags & (0x0001 | 0x8000) != 0 && attr_len >= 72 {
        u64_at(attr, 64)
    } else {
        u64_at(attr, 40)
    }
}

impl Table {
    fn rec_mut(&mut self, index: usize) -> &mut Rec {
        if self.recs.len() <= index {
            self.recs.resize(index + 1, Rec::default());
        }
        &mut self.recs[index]
    }

    fn parse_record(&mut self, number: u64, rec: &[u8]) {
        if &rec[0..4] != b"FILE" {
            return;
        }
        let flags = u16_at(rec, 22);
        if flags & 1 == 0 {
            return;
        }
        let base = u64_at(rec, 32) & 0x0000_FFFF_FFFF_FFFF;
        let is_base = base == 0;
        let target = if is_base { number } else { base } as usize;
        let used = (u32_at(rec, 24) as usize).min(rec.len());
        if is_base {
            let seq = u16_at(rec, 16);
            let r = self.rec_mut(target);
            r.in_use = true;
            r.dir = flags & 2 != 0;
            r.seq = seq;
        }
        let mut off = u16_at(rec, 20) as usize;
        while off + 16 <= used {
            let a = &rec[off..used];
            let kind = u32_at(a, 0);
            let len = u32_at(a, 4) as usize;
            if kind == 0xFFFF_FFFF || len < 16 || len > a.len() {
                break;
            }
            let attr = &a[..len];
            let nonresident = attr[8] != 0;
            let name_len = attr[9] as usize;
            let value = if nonresident {
                &[][..]
            } else {
                let voff = u16_at(attr, 20) as usize;
                let vlen = u32_at(attr, 16) as usize;
                attr.get(voff..voff + vlen).unwrap_or(&[])
            };
            match kind {
                0x10 if !nonresident && value.len() >= 36 => {
                    let r = self.rec_mut(target);
                    r.modified = u64_at(value, 8) as i64;
                    r.accessed = u64_at(value, 24) as i64;
                    r.attributes = u32_at(value, 32);
                }
                0x30 if !nonresident && value.len() >= 66 => {
                    let namespace = value[65];
                    let chars = value[64] as usize;
                    // Espace de noms 2 : nom court 8.3 seul, doublon d'un nom long.
                    if namespace != 2 && value.len() >= 66 + chars * 2 {
                        let parent_ref = u64_at(value, 0);
                        let units: Vec<u16> =
                            value[66..66 + chars * 2].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                        let name = String::from_utf16_lossy(&units);
                        let reparse = u32_at(value, 60);
                        let off = self.text.len() as u32;
                        self.text.push_str(&name);
                        self.names.push(Name {
                            record: target as u32,
                            parent: (parent_ref & 0x0000_FFFF_FFFF_FFFF) as u32,
                            parent_seq: (parent_ref >> 48) as u16,
                            off,
                            len: name.len() as u32,
                        });
                        let r = self.rec_mut(target);
                        if r.tag == 0 && reparse != 0 {
                            r.tag = reparse;
                        }
                    }
                }
                0x80 => {
                    let stream = if name_len == 0 {
                        None
                    } else {
                        let noff = u16_at(attr, 10) as usize;
                        let units: Vec<u16> =
                            attr.get(noff..noff + name_len * 2).unwrap_or(&[]).chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                        Some(String::from_utf16_lossy(&units))
                    };
                    let (logical, allocated, first) = if nonresident {
                        (u64_at(attr, 48), nonresident_allocated(attr), u64_at(attr, 16) == 0)
                    } else {
                        // Données résidentes : dans l'enregistrement MFT, aucun cluster alloué.
                        (u32_at(attr, 16) as u64, 0, true)
                    };
                    if first {
                        let r = self.rec_mut(target);
                        match stream.as_deref() {
                            None => {
                                r.logical = logical;
                                r.allocated = allocated;
                            }
                            Some("WofCompressedData") => r.wof_allocated = Some(allocated),
                            Some(_) => {}
                        }
                    }
                }
                0xC0 if !nonresident && value.len() >= 4 => {
                    let tag = u32_at(value, 0);
                    self.rec_mut(target).tag = tag;
                }
                _ => {}
            }
            off += len;
        }
    }

    fn name(&self, n: &Name) -> &str {
        &self.text[n.off as usize..(n.off + n.len) as usize]
    }

    fn entry(&self, name: String, record: usize) -> RawEntry {
        let r = self.recs[record];
        let attributes = r.attributes | if r.dir { ATTR_DIRECTORY } else { 0 };
        let tag = if attributes & ATTR_REPARSE != 0 { r.tag } else { 0 };
        let kind = classify(attributes, tag);
        let allocated = match kind {
            Kind::File if tag == TAG_WOF => r.wof_allocated.unwrap_or(r.allocated),
            Kind::File => r.allocated,
            _ => 0,
        };
        RawEntry {
            name,
            kind,
            attributes,
            file_id: (u128::from(r.seq) << 48) | record as u128,
            logical: if matches!(kind, Kind::File | Kind::CloudFile) { r.logical } else { 0 },
            allocated,
            modified: r.modified,
            accessed: r.accessed,
        }
    }
}

struct Volume {
    file: File,
    record_size: usize,
    cluster: u64,
    mft_start: u64,
    mft_valid: u64,
    serial: u64,
}

fn open_volume(drive: char) -> Result<Volume, ScanError> {
    let path = format!("\\\\.\\{drive}:");
    // Écrit sur disque les métadonnées encore en cache, pour lire une MFT à jour.
    if let Ok(f) = OpenOptions::new().read(true).write(true).share_mode(3).open(&path) {
        let _ = f.sync_all();
    }
    let file = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&path)
        .map_err(|e| if e.raw_os_error() == Some(5) { ScanError::NotElevated } else { err(format!("{path} : {e}")) })?;
    let mut data = NTFS_VOLUME_DATA_BUFFER::default();
    let mut returned = 0u32;
    // SAFETY : handle valide pendant l'appel, structure de sortie possédée et de taille annoncée.
    unsafe {
        DeviceIoControl(
            HANDLE(file.as_raw_handle()),
            FSCTL_GET_NTFS_VOLUME_DATA,
            None,
            0,
            Some(&mut data as *mut NTFS_VOLUME_DATA_BUFFER as *mut c_void),
            std::mem::size_of::<NTFS_VOLUME_DATA_BUFFER>() as u32,
            Some(&mut returned),
            None,
        )
    }
    .map_err(|_| err("volume non NTFS"))?;
    Ok(Volume {
        file,
        record_size: data.BytesPerFileRecordSegment as usize,
        cluster: u64::from(data.BytesPerCluster),
        mft_start: data.MftStartLcn as u64,
        mft_valid: data.MftValidDataLength as u64,
        serial: data.VolumeSerialNumber as u64,
    })
}

fn read_at(file: &mut File, offset: u64, buf: &mut [u8]) -> Result<(), ScanError> {
    file.seek(SeekFrom::Start(offset)).map_err(|e| err(e.to_string()))?;
    file.read_exact(buf).map_err(|e| err(e.to_string()))
}

/// Extents de la MFT elle-même, lus dans son enregistrement 0.
fn mft_extents(v: &mut Volume) -> Result<Vec<(u64, u64)>, ScanError> {
    let mut rec = vec![0u8; v.record_size];
    read_at(&mut v.file, v.mft_start * v.cluster, &mut rec)?;
    if &rec[0..4] != b"FILE" || !apply_fixup(&mut rec) {
        return Err(err("enregistrement 0 de la MFT illisible"));
    }
    let used = (u32_at(&rec, 24) as usize).min(rec.len());
    let mut off = u16_at(&rec, 20) as usize;
    while off + 16 <= used {
        let a = &rec[off..used];
        let kind = u32_at(a, 0);
        let len = u32_at(a, 4) as usize;
        if kind == 0xFFFF_FFFF || len < 16 || len > a.len() {
            break;
        }
        if kind == 0x80 && a[8] != 0 && a[9] == 0 {
            let runs_off = u16_at(a, 32) as usize;
            let runs = data_runs(a.get(runs_off..len).unwrap_or(&[]));
            let extents: Vec<(u64, u64)> = runs.into_iter().filter_map(|(lcn, n)| lcn.map(|l| (l as u64, n))).collect();
            if extents.is_empty() {
                break;
            }
            return Ok(extents);
        }
        off += len;
    }
    // MFT très fragmentée (liste d'attributs) : non gérée, le parcours prend le relais.
    Err(err("MFT fragmentée : liste d'attributs non prise en charge"))
}

fn read_table(v: &mut Volume, cancel: &AtomicBool, progress: &mut dyn FnMut(u64, u64)) -> Result<Table, ScanError> {
    let extents = mft_extents(v)?;
    let total_records = v.mft_valid / v.record_size as u64;
    let mut table = Table { recs: Vec::with_capacity(total_records as usize), ..Table::default() };
    let mut number: u64 = 0;
    let mut buf = vec![0u8; CHUNK];
    'outer: for (lcn, clusters) in extents {
        let mut offset = lcn * v.cluster;
        let mut remaining = clusters * v.cluster;
        while remaining > 0 {
            if cancel.load(Ordering::Relaxed) {
                return Err(ScanError::Cancelled);
            }
            let n = (remaining as usize).min(CHUNK);
            read_at(&mut v.file, offset, &mut buf[..n])?;
            for rec in buf[..n].chunks_exact_mut(v.record_size) {
                if number >= total_records {
                    break 'outer;
                }
                if apply_fixup(rec) {
                    table.parse_record(number, rec);
                }
                number += 1;
            }
            offset += n as u64;
            remaining -= n as u64;
            progress(number, total_records);
        }
    }
    Ok(table)
}

/// Scanne `root` (racine d'un volume NTFS ou dossier) en lisant la MFT.
pub fn scan_mft(root: &str, cancel: &AtomicBool, on_progress: &(dyn Fn(&Progress) + Sync)) -> Result<Scan, ScanError> {
    let started = Instant::now();
    // La MFT ne connaît que les noms longs (les noms courts 8.3 sont ignorés) : on résout d'abord.
    let root_owned = super::long_path(root);
    let root = root_owned.as_str();
    let drive = root.chars().next().filter(|c| c.is_ascii_alphabetic() && root[1..].starts_with(":\\")).ok_or_else(|| err("racine sans lettre de lecteur"))?;
    let mut volume = open_volume(drive.to_ascii_uppercase())?;
    let mut last = Instant::now();
    let table = read_table(&mut volume, cancel, &mut |done, total| {
        if last.elapsed() >= PROGRESS_INTERVAL {
            last = Instant::now();
            on_progress(&Progress {
                files: done,
                dirs: 0,
                allocated: 0,
                errors: 0,
                elapsed_ms: started.elapsed().as_millis() as u64,
                tops: Vec::new(),
                phase: if total > 0 { "mft-read" } else { "mft" },
            });
        }
    })?;

    // Index des noms par dossier parent.
    let mut by_parent: Vec<u32> = (0..table.names.len() as u32).collect();
    by_parent.sort_unstable_by_key(|&i| table.names[i as usize].parent);
    let children_of = |record: u32| -> &[u32] {
        let lo = by_parent.partition_point(|&i| table.names[i as usize].parent < record);
        let hi = by_parent.partition_point(|&i| table.names[i as usize].parent <= record);
        &by_parent[lo..hi]
    };
    let valid_child = |n: &Name| -> bool {
        let rec = n.record as usize;
        u64::from(n.record) >= FIRST_USER_RECORD
            && table.recs.get(rec).is_some_and(|r| r.in_use)
            && table.recs.get(n.parent as usize).is_some_and(|p| p.in_use && p.seq == n.parent_seq)
    };

    // Dossier de départ : la racine du volume, ou le dossier demandé, nom par nom.
    let mut record = ROOT_RECORD as u32;
    for part in root[3..].split('\\').filter(|p| !p.is_empty()) {
        record = children_of(record)
            .iter()
            .map(|&i| &table.names[i as usize])
            .find(|n| valid_child(n) && table.name(n).to_lowercase() == part.to_lowercase())
            .map(|n| n.record)
            .ok_or_else(|| err(format!("{part} introuvable dans la MFT")))?;
    }

    let root_name = root.trim_end_matches('\\').rsplit('\\').next().unwrap_or(root).to_owned();
    let mut root_entry = table.entry(root_name, record as usize);
    root_entry.kind = Kind::Dir;
    let mut builder = Builder::new(&root_entry);
    let mut queue = std::collections::VecDeque::from([(record, 0u32)]);
    let mut done_dirs = 0u64;
    while let Some((rec, node)) = queue.pop_front() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ScanError::Cancelled);
        }
        let entries: Vec<(RawEntry, u32)> = children_of(rec)
            .iter()
            .map(|&i| &table.names[i as usize])
            .filter(|n| valid_child(n))
            .map(|n| (table.entry(table.name(n).to_owned(), n.record as usize), n.record))
            .collect();
        let raw: Vec<RawEntry> = entries.iter().map(|(e, _)| e.clone()).collect();
        let range = builder.add_children(node, &raw);
        for (k, (e, child)) in entries.into_iter().enumerate() {
            if e.kind == Kind::Dir {
                queue.push_back((child, range.start + k as u32));
            }
        }
        done_dirs += 1;
        if last.elapsed() >= PROGRESS_INTERVAL {
            last = Instant::now();
            on_progress(&Progress {
                files: builder.len() as u64,
                dirs: done_dirs,
                allocated: 0,
                errors: 0,
                elapsed_ms: started.elapsed().as_millis() as u64,
                tops: Vec::new(),
                phase: "mft-build",
            });
        }
    }
    drop(table);
    let mut scan = builder.finish(root.to_owned(), volume.serial, Method::Mft, super::last_access_reliable(), Stats::default());
    scan.stats.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(scan)
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn decodage_des_extents() {
        // 0x21 : longueur sur 1 octet (0x18 clusters), décalage sur 2 octets (0x5634).
        // 0x11 : longueur 0x10, décalage −2 (0xFE). 0x01 : trou de 8 clusters.
        let runs = data_runs(&[0x21, 0x18, 0x34, 0x56, 0x11, 0x10, 0xFE, 0x01, 0x08, 0x00]);
        assert_eq!(runs, vec![(Some(0x5634), 0x18), (Some(0x5632), 0x10), (None, 8)]);
    }

    #[test]
    fn correction_des_secteurs() {
        let mut rec = vec![0u8; 1024];
        rec[0..4].copy_from_slice(b"FILE");
        rec[4] = 48; // décalage du tableau
        rec[6] = 3; // 1 valeur de contrôle + 2 secteurs
        rec[48] = 0xAB;
        rec[49] = 0xCD;
        rec[50] = 0x11;
        rec[51] = 0x22;
        rec[52] = 0x33;
        rec[53] = 0x44;
        rec[510] = 0xAB;
        rec[511] = 0xCD;
        rec[1022] = 0xAB;
        rec[1023] = 0xCD;
        assert!(apply_fixup(&mut rec));
        assert_eq!(&rec[510..512], &[0x11, 0x22]);
        assert_eq!(&rec[1022..1024], &[0x33, 0x44]);
        rec[510] = 0;
        rec[1022] = 0;
        assert!(!apply_fixup(&mut rec), "secteur abîmé détecté");
    }
}
