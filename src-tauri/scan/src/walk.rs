//! Parcours parallèle (pool rayon dédié, un travail par dossier). Une source ([`Source`]) lit la
//! liste d'un dossier — métadonnées seulement — et le parcours construit l'arène, compte les
//! erreurs sans jamais s'arrêter, publie la progression (~10 fois par seconde) et s'annule proprement.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::model::{Builder, Kind, Method, NodeId, RawEntry, Scan, Stats};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirError {
    AccessDenied,
    /// Partage refusé (ouvert en exclusivité par un autre programme).
    Locked,
    NotFound,
    Other(i32),
}

/// Lecture des métadonnées d'un dossier. Ne doit jamais ouvrir le contenu d'un fichier.
pub trait Source: Sync {
    fn root(&self, path: &str) -> Result<RawEntry, DirError>;
    fn read_dir(&self, path: &str, out: &mut Vec<RawEntry>) -> Result<(), DirError>;
    fn join(&self, dir: &str, name: &str) -> String;
    fn volume_serial(&self, root: &str) -> u64;
    fn last_access_reliable(&self, root: &str) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScanError {
    #[error("analyse annulée")]
    Cancelled,
    #[error("dossier à analyser illisible : {0:?}")]
    Root(DirError),
    #[error("lecture de la MFT impossible : {0}")]
    Mft(String),
    /// Forme de MFT non gérée (MFT très fragmentée) : le parcours prend le relais.
    #[error("MFT non lisible directement : {0}")]
    MftUnsupported(String),
    #[error("l'analyse rapide demande les droits administrateur")]
    NotElevated,
}

/// Un dossier de premier niveau, avec ce qui a déjà été compté dessous (pour la carte en direct).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopProgress {
    pub name: String,
    pub allocated: u64,
    pub files: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    pub files: u64,
    pub dirs: u64,
    pub allocated: u64,
    pub errors: u64,
    pub elapsed_ms: u64,
    pub tops: Vec<TopProgress>,
    /// Phase en cours (« walk », « mft-read », « mft-build », « aggregate »).
    pub phase: &'static str,
}

#[derive(Default)]
struct Counters {
    files: AtomicU64,
    dirs: AtomicU64,
    allocated: AtomicU64,
    access_denied: AtomicU64,
    locked: AtomicU64,
    other: AtomicU64,
}

struct Top {
    name: String,
    allocated: AtomicU64,
    files: AtomicU64,
}

struct Shared<'a, S: Source> {
    source: &'a S,
    builder: Mutex<Builder>,
    counters: Counters,
    tops: Mutex<Vec<Top>>,
    samples: Mutex<Vec<String>>,
    cancel: &'a AtomicBool,
}

const ERROR_SAMPLES: usize = 50;

impl<S: Source> Shared<'_, S> {
    fn count_error(&self, path: &str, e: DirError) {
        let c = match e {
            DirError::AccessDenied => &self.counters.access_denied,
            DirError::Locked => &self.counters.locked,
            DirError::NotFound | DirError::Other(_) => &self.counters.other,
        };
        c.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut s) = self.samples.lock() {
            if s.len() < ERROR_SAMPLES {
                s.push(path.to_owned());
            }
        }
    }

    fn visit<'s>(&'s self, scope: &rayon::Scope<'s>, dir: NodeId, path: String, top: Option<usize>) {
        if self.cancel.load(Ordering::Relaxed) {
            return;
        }
        let mut entries = Vec::new();
        if let Err(e) = self.source.read_dir(&path, &mut entries) {
            self.count_error(&path, e);
            if let Ok(mut b) = self.builder.lock() {
                b.mark_unreadable(dir);
            }
            return;
        }
        self.counters.dirs.fetch_add(1, Ordering::Relaxed);
        let (mut files, mut allocated) = (0u64, 0u64);
        for e in &entries {
            if e.kind == Kind::File {
                files += 1;
                allocated += e.allocated;
            }
        }
        self.counters.files.fetch_add(files, Ordering::Relaxed);
        self.counters.allocated.fetch_add(allocated, Ordering::Relaxed);

        let range = match self.builder.lock() {
            Ok(mut b) => b.add_children(dir, &entries),
            Err(_) => return,
        };

        // Racine : chaque dossier de premier niveau a son compteur, pour la carte en direct.
        let top_base = if top.is_none() {
            if let Ok(mut t) = self.tops.lock() {
                let base = t.len();
                t.extend(entries.iter().map(|e| Top {
                    name: e.name.clone(),
                    allocated: AtomicU64::new(if e.kind == Kind::File { e.allocated } else { 0 }),
                    files: AtomicU64::new(u64::from(e.kind == Kind::File)),
                }));
                Some(base)
            } else {
                None
            }
        } else {
            if let Some(t) = top {
                if let Ok(tops) = self.tops.lock() {
                    if let Some(entry) = tops.get(t) {
                        entry.allocated.fetch_add(allocated, Ordering::Relaxed);
                        entry.files.fetch_add(files, Ordering::Relaxed);
                    }
                }
            }
            None
        };

        for (k, e) in entries.into_iter().enumerate() {
            if e.kind == Kind::Dir {
                let child = range.start + k as NodeId;
                let child_path = self.source.join(&path, &e.name);
                let child_top = top.or(top_base.map(|b| b + k));
                scope.spawn(move |s| self.visit(s, child, child_path, child_top));
            }
        }
    }

    fn snapshot(&self, started: Instant, phase: &'static str) -> Progress {
        let c = &self.counters;
        let tops = self
            .tops
            .lock()
            .map(|t| {
                let mut v: Vec<TopProgress> = t
                    .iter()
                    .map(|e| TopProgress {
                        name: e.name.clone(),
                        allocated: e.allocated.load(Ordering::Relaxed),
                        files: e.files.load(Ordering::Relaxed),
                    })
                    .collect();
                v.sort_by_key(|t| std::cmp::Reverse(t.allocated));
                v.truncate(24);
                v
            })
            .unwrap_or_default();
        Progress {
            files: c.files.load(Ordering::Relaxed),
            dirs: c.dirs.load(Ordering::Relaxed),
            allocated: c.allocated.load(Ordering::Relaxed),
            errors: c.access_denied.load(Ordering::Relaxed) + c.locked.load(Ordering::Relaxed) + c.other.load(Ordering::Relaxed),
            elapsed_ms: started.elapsed().as_millis() as u64,
            tops,
            phase,
        }
    }
}

/// Intervalle entre deux événements de progression (≈ 10 par seconde).
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Nombre de fils : le parcours attend surtout le disque, on dépasse donc le nombre de cœurs.
fn threads() -> usize {
    std::thread::available_parallelism().map_or(8, |n| n.get() * 2).clamp(4, 32)
}

/// Parcourt `root` avec `source`. `on_progress` est appelé au plus ~10 fois par seconde.
pub fn walk<S: Source>(
    source: &S,
    root: &str,
    cancel: &AtomicBool,
    on_progress: &(dyn Fn(&Progress) + Sync),
) -> Result<Scan, ScanError> {
    let started = Instant::now();
    let root_entry = source.root(root).map_err(ScanError::Root)?;
    let shared = Shared {
        source,
        builder: Mutex::new(Builder::new(&root_entry)),
        counters: Counters::default(),
        tops: Mutex::new(Vec::new()),
        samples: Mutex::new(Vec::new()),
        cancel,
    };
    let done = AtomicBool::new(false);

    std::thread::scope(|ts| {
        ts.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                std::thread::sleep(PROGRESS_INTERVAL);
                if !done.load(Ordering::Relaxed) {
                    on_progress(&shared.snapshot(started, "walk"));
                }
            }
        });
        match rayon::ThreadPoolBuilder::new().num_threads(threads()).thread_name(|i| format!("lumen-scan-{i}")).build() {
            Ok(pool) => pool.scope(|s| shared.visit(s, 0, root.to_owned(), None)),
            Err(_) => rayon::scope(|s| shared.visit(s, 0, root.to_owned(), None)),
        }
        done.store(true, Ordering::Relaxed);
    });

    if cancel.load(Ordering::Relaxed) {
        return Err(ScanError::Cancelled);
    }
    on_progress(&shared.snapshot(started, "aggregate"));
    let c = &shared.counters;
    let stats = Stats {
        access_denied: c.access_denied.load(Ordering::Relaxed),
        locked: c.locked.load(Ordering::Relaxed),
        other_errors: c.other.load(Ordering::Relaxed),
        error_samples: shared.samples.into_inner().unwrap_or_default(),
        ..Stats::default()
    };
    let builder = shared.builder.into_inner().map_err(|_| ScanError::Mft("arène empoisonnée".into()))?;
    let mut scan = builder.finish(
        root.to_owned(),
        source.volume_serial(root),
        Method::Walk,
        source.last_access_reliable(root),
        stats,
    );
    scan.stats.elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(scan)
}
