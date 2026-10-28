//! Déplacement intelligent (logique pure) : un dossier part sur un **autre disque fixe NTFS
//! local**, une jonction le remplace à son emplacement d'origine, et les applis continuent de le
//! trouver au même chemin.
//!
//! Ordre imposé, sans exception :
//! 1. recontrôle complet au moment de l'action (refus, programmes qui tiennent des fichiers,
//!    espace libre avec 10 % de marge, contenu identique au plan) ;
//! 2. copie fidèle (attributs, dates, ACL, flux alternatifs) ;
//! 3. vérification **BLAKE3 de chaque fichier** (source et copie) ;
//! 4. l'original est mis de côté (renommé à côté, même disque) ;
//! 5. la jonction est créée, puis **testée** (lecture à travers elle, empreintes comparées) ;
//! 6. **seulement après une confirmation explicite**, l'original mis de côté est supprimé
//!    définitivement (jamais à travers un lien).
//!
//! Tout échec avant l'étape 6 déclenche un retour en arrière automatique (jonction retirée,
//! original remis en place, copie supprimée) ; refuser la confirmation aussi. « Annuler le
//! déplacement » recopie depuis la cible, vérifie, retire la jonction, remet le dossier et
//! envoie la copie de la cible à la Corbeille.

use crate::bin::DriveKind;
use crate::denylist::{DenyList, DenyReason};
use crate::path::{fold, WinPath};
use crate::probe::ProbeError;

/// Dossier de destination sur le disque cible : `<X>:\Lumen\Déplacés\<lettre>\<chemin>`.
pub const TARGET_DIR: [&str; 2] = ["Lumen", "Déplacés"];
/// Suffixe de l'original mis de côté pendant le déplacement (même dossier parent).
pub const STAGING_SUFFIX: &str = ".lumen-deplacement";
/// Suffixe de la recopie pendant une annulation.
pub const RETURN_SUFFIX: &str = ".lumen-retour";
/// Marge d'espace libre exigée sur la cible : 10 %.
pub const SPACE_MARGIN_PERCENT: u64 = 10;
/// Fichiers lus à travers la jonction pour la tester.
const JUNCTION_SAMPLES: usize = 3;

/// Élément du dossier à déplacer, lu sans jamais suivre un lien.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    /// Chemin relatif au dossier (`sous\fichier.bin`).
    pub rel: String,
    pub is_dir: bool,
    pub size: u64,
    pub reparse: bool,
    pub cloud: bool,
}

/// Disque, tel que la sonde le voit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeInfo {
    pub kind: DriveKind,
    pub subst: bool,
    pub ntfs: bool,
    pub free: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MoveIoError {
    #[error("{0}")]
    Io(String),
    #[error("contenu différent après la copie : {0}")]
    Mismatch(String),
    #[error("la jonction ne mène pas à la copie")]
    BadJunction,
}

/// Ce que le déplacement doit savoir du système (implémentation réelle : `safety::win`).
pub trait MoveProbe {
    fn volume(&self, drive: char) -> Option<VolumeInfo>;
    /// Le chemin existe (sans suivre un lien), et est un dossier ordinaire / un lien.
    fn kind(&self, path: &WinPath) -> Option<PathKind>;
    /// Contenu du dossier, récursif, sans jamais suivre un lien.
    fn tree(&self, dir: &WinPath) -> Result<Vec<TreeEntry>, ProbeError>;
    /// Programmes qui tiennent ces fichiers ouverts (Restart Manager) : leurs noms.
    fn holders(&self, files: &[WinPath]) -> Vec<String>;
    fn hash(&self, file: &WinPath) -> Result<[u8; 32], ProbeError>;
    /// Cible d'une jonction (`None` : pas une jonction).
    fn junction_target(&self, link: &WinPath) -> Option<WinPath>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    File,
    Dir,
    /// Jonction ou lien symbolique.
    Link,
}

/// Ce que le déplacement fait au système.
pub trait MoveEffects {
    fn create_dir(&self, path: &WinPath, template: &WinPath) -> Result<(), MoveIoError>;
    /// Copie fidèle d'un fichier : contenu, attributs, dates, ACL, flux alternatifs ; jamais
    /// par-dessus un fichier existant.
    fn copy_file(&self, from: &WinPath, to: &WinPath) -> Result<(), MoveIoError>;
    /// Dates, attributs, ACL et flux alternatifs d'un dossier, une fois son contenu copié.
    /// `top` : le dossier déplacé lui-même ; ses droits effectifs (hérités compris) sont
    /// figés sur la copie, qui n'hérite pas de son nouveau parent (les fichiers d'un profil ne
    /// deviennent jamais lisibles par les autres comptes sur le disque cible).
    fn copy_dir_meta(&self, from: &WinPath, to: &WinPath, top: bool) -> Result<(), MoveIoError>;
    fn rename(&self, from: &WinPath, to: &WinPath) -> Result<(), MoveIoError>;
    fn create_junction(&self, link: &WinPath, target: &WinPath) -> Result<(), MoveIoError>;
    /// Retire la jonction (seulement si elle mène à `expected`, quand il est donné ; toujours une
    /// jonction, jamais un dossier ordinaire) ; ne touche jamais sa cible.
    fn remove_junction(&self, link: &WinPath, expected: Option<&WinPath>) -> Result<(), MoveIoError>;
    /// Suppression définitive d'un arbre, jamais à travers un lien.
    fn delete_tree(&self, path: &WinPath) -> Result<(), MoveIoError>;
    /// Corbeille (annulation : la copie de la cible).
    fn recycle(&self, path: &WinPath) -> Result<(), MoveIoError>;
}

/// Ce qui rend un dossier non déplaçable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("seul un dossier se déplace")]
    NotADirectory,
    #[error("dossier protégé : {0}")]
    Denied(DenyReason),
    #[error("emplacement cible protégé : {0}")]
    TargetDenied(DenyReason),
    #[error("déjà déplacé (c'est une jonction)")]
    AlreadyMoved,
    #[error("la cible est sur le même disque : déplacer ne libère de la place qu'ailleurs")]
    SameVolume,
    #[error("disque cible introuvable")]
    TargetUnavailable,
    #[error("la cible doit être un disque fixe local : une jonction ne peut pas mener à un lecteur réseau ni amovible")]
    TargetNotFixed,
    #[error("lecteur SUBST : ce n'est pas un vrai disque")]
    TargetSubst,
    #[error("la cible doit être en NTFS")]
    TargetNotNtfs,
    #[error("le dossier doit être sur un disque NTFS (la jonction y est créée)")]
    SourceNotNtfs,
    #[error("dossier de jeu : déplace-le avec son launcher")]
    GameFolder,
    #[error("contient un disque virtuel (.vhdx) : WSL ou Docker ne le retrouveraient plus")]
    VirtualDisk,
    #[error("dossier synchronisé (OneDrive…) : le déplacer casserait la synchronisation")]
    Synced,
    #[error("contient des liens (jonctions, liens symboliques) : non déplaçable sans risque")]
    ReparsePoints,
    #[error("ouvert par {}", .0.join(", "))]
    InUse(Vec<String>),
    #[error("espace insuffisant sur la cible : {needed} octets nécessaires (marge de 10 % comprise), {free} libres")]
    NotEnoughSpace { needed: u64, free: u64 },
    #[error("le dossier cible existe déjà")]
    TargetExists,
    #[error("un déplacement précédent n'est pas terminé à cet emplacement")]
    StagingExists,
    #[error("dossier vide : rien à gagner")]
    Empty,
    #[error("contenu illisible : {0}")]
    Unreadable(String),
}

/// Contexte de la machine (dossiers connus, jeux, racines synchronisées).
pub struct MoveContext<'a> {
    pub deny: &'a DenyList,
    pub game_dirs: &'a [WinPath],
    /// OneDrive et toute racine synchronisée (gestionnaire de synchronisation de Windows).
    pub synced_roots: &'a [WinPath],
    /// AppData (Local, LocalLow, Roaming) : déplacement permis, mais « À vérifier ».
    pub app_data: &'a [WinPath],
}

/// Déplacement préparé, à confirmer par son empreinte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prepared {
    pub source: WinPath,
    pub target: WinPath,
    pub entries: Vec<TreeEntry>,
    pub bytes: u64,
    pub files: u64,
    /// Dossier d'appli (AppData) : l'appli peut ne plus se mettre à jour correctement.
    pub review: bool,
    pub target_free: u64,
    pub needed: u64,
}

fn drive(p: &WinPath) -> Option<char> {
    match p.prefix() {
        crate::path::Prefix::Drive(d) => Some(*d),
        crate::path::Prefix::Unc { .. } => None,
    }
}

/// Espace exigé sur la cible : la taille plus 10 %.
pub fn needed_space(bytes: u64) -> u64 {
    bytes.saturating_add(bytes.div_ceil(100 / SPACE_MARGIN_PERCENT))
}

/// Emplacement de la copie sur le disque cible.
pub fn target_for(source: &WinPath, target_drive: char) -> Option<WinPath> {
    let letter = drive(source)?;
    let mut t = WinPath::parse(&format!("{}:\\", target_drive.to_ascii_uppercase())).ok()?;
    for c in TARGET_DIR.iter().copied().chain(std::iter::once(&*letter.to_string())) {
        t = t.join(c).ok()?;
    }
    for c in source.components() {
        t = t.join(c).ok()?;
    }
    Some(t)
}

fn sibling(path: &WinPath, suffix: &str) -> Option<WinPath> {
    let name = format!("{}{suffix}", path.file_name()?);
    path.parent()?.join(&name).ok()
}

pub fn staging_of(source: &WinPath) -> Option<WinPath> {
    sibling(source, STAGING_SUFFIX)
}

pub fn return_of(source: &WinPath) -> Option<WinPath> {
    sibling(source, RETURN_SUFFIX)
}

fn is_virtual_disk(rel: &str) -> bool {
    let n = fold(rel);
    n.ends_with(".VHDX") || n.ends_with(".VHD") || n.ends_with(".AVHDX")
}

fn join_rel(base: &WinPath, rel: &str) -> Option<WinPath> {
    rel.split('\\').filter(|c| !c.is_empty()).try_fold(base.clone(), |acc, c| acc.join(c).ok())
}

/// Tous les refus d'un déplacement (vide : déplaçable), et le déplacement préparé.
pub fn prepare(source: &WinPath, target_drive: char, ctx: &MoveContext<'_>, probe: &dyn MoveProbe) -> Result<Prepared, Vec<Refusal>> {
    let mut refusals = Vec::new();
    match probe.kind(source) {
        Some(PathKind::Dir) => {}
        Some(PathKind::Link) => return Err(vec![if probe.junction_target(source).is_some() { Refusal::AlreadyMoved } else { Refusal::ReparsePoints }]),
        _ => return Err(vec![Refusal::NotADirectory]),
    }
    if let Some(r) = ctx.deny.check(source) {
        refusals.push(Refusal::Denied(r));
    }
    // Un parent lien (jonction sur le chemin) : la jonction créée mènerait ailleurs qu'attendu.
    let chain = source.chain();
    if chain[..chain.len().saturating_sub(1)].iter().any(|p| probe.kind(p) == Some(PathKind::Link)) {
        refusals.push(Refusal::ReparsePoints);
    }
    if ctx.game_dirs.iter().any(|g| source.starts_with(g) || g.is_strictly_under(source)) {
        refusals.push(Refusal::GameFolder);
    }
    if ctx.synced_roots.iter().any(|r| source.starts_with(r) || r.is_strictly_under(source)) {
        refusals.push(Refusal::Synced);
    }
    let src_drive = drive(source);
    let target_drive = target_drive.to_ascii_uppercase();
    if src_drive == Some(target_drive) {
        refusals.push(Refusal::SameVolume);
    }
    let src_volume = src_drive.and_then(|d| probe.volume(d));
    if src_volume.is_some_and(|v| !v.ntfs) || src_volume.is_none() {
        refusals.push(Refusal::SourceNotNtfs);
    }
    let target_volume = probe.volume(target_drive);
    match target_volume {
        None => refusals.push(Refusal::TargetUnavailable),
        Some(v) => {
            if v.subst {
                refusals.push(Refusal::TargetSubst);
            } else if v.kind != DriveKind::Fixed {
                refusals.push(Refusal::TargetNotFixed);
            }
            if !v.ntfs {
                refusals.push(Refusal::TargetNotNtfs);
            }
        }
    }
    let target = target_for(source, target_drive);
    match &target {
        Some(t) => {
            if let Some(r) = ctx.deny.check(t) {
                refusals.push(Refusal::TargetDenied(r));
            }
            if probe.kind(t).is_some() {
                refusals.push(Refusal::TargetExists);
            }
        }
        None => refusals.push(Refusal::TargetUnavailable),
    }
    if staging_of(source).is_some_and(|s| probe.kind(&s).is_some()) {
        refusals.push(Refusal::StagingExists);
    }
    let entries = match probe.tree(source) {
        Ok(e) => e,
        Err(e) => {
            refusals.push(Refusal::Unreadable(e.to_string()));
            return Err(refusals);
        }
    };
    if entries.iter().any(|e| e.reparse) && !refusals.contains(&Refusal::ReparsePoints) {
        refusals.push(Refusal::ReparsePoints);
    }
    if entries.iter().any(|e| e.cloud) && !refusals.contains(&Refusal::Synced) {
        refusals.push(Refusal::Synced);
    }
    if entries.iter().any(|e| !e.is_dir && is_virtual_disk(&e.rel)) {
        refusals.push(Refusal::VirtualDisk);
    }
    // Un élément du dossier dans la denylist (sauvegarde, secret…) : il ne partirait pas.
    if let Some(r) = entries.iter().filter_map(|e| join_rel(source, &e.rel)).find_map(|p| ctx.deny.check(&p)) {
        if !refusals.iter().any(|x| matches!(x, Refusal::Denied(_))) {
            refusals.push(Refusal::Denied(r));
        }
    }
    let files: Vec<&TreeEntry> = entries.iter().filter(|e| !e.is_dir).collect();
    let bytes: u64 = files.iter().map(|e| e.size).sum();
    if files.is_empty() {
        refusals.push(Refusal::Empty);
    }
    let needed = needed_space(bytes);
    let target_free = target_volume.map_or(0, |v| v.free);
    if target_volume.is_some() && target_free < needed {
        refusals.push(Refusal::NotEnoughSpace { needed, free: target_free });
    }
    let paths: Vec<WinPath> = files.iter().filter_map(|e| join_rel(source, &e.rel)).collect();
    let holders = probe.holders(&paths);
    if !holders.is_empty() {
        refusals.push(Refusal::InUse(holders));
    }
    let Some(target) = target else { return Err(refusals) };
    if !refusals.is_empty() {
        return Err(refusals);
    }
    let review = ctx.app_data.iter().any(|a| source.is_strictly_under(a));
    Ok(Prepared { source: source.clone(), target, files: files.len() as u64, bytes, entries, review, target_free, needed })
}

impl Prepared {
    /// Empreinte de ce qui sera déplacé (BLAKE3) : la confirmation porte sur elle.
    pub fn fingerprint(&self) -> String {
        let mut h = blake3::Hasher::new();
        h.update(fold(&self.source.display_string()).as_bytes());
        h.update(b"\0");
        h.update(fold(&self.target.display_string()).as_bytes());
        for e in &self.entries {
            h.update(b"\0");
            h.update(fold(&e.rel).as_bytes());
            h.update(&e.size.to_le_bytes());
            h.update(&[u8::from(e.is_dir)]);
        }
        h.finalize().to_hex().to_string()
    }
}

/// Étapes, dans l'ordre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Step {
    Recheck,
    Copy,
    Verify,
    SetAside,
    Junction,
    Test,
    Delete,
}

impl Step {
    pub fn name(self) -> &'static str {
        match self {
            Step::Recheck => "recheck",
            Step::Copy => "copy",
            Step::Verify => "verify",
            Step::SetAside => "setAside",
            Step::Junction => "junction",
            Step::Test => "test",
            Step::Delete => "delete",
        }
    }
}

/// Avancement : étape, octets faits, octets en tout.
pub type Progress<'a> = &'a dyn Fn(Step, u64, u64);

/// Déplacement en attente de la confirmation de suppression de l'original.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Staged {
    pub source: WinPath,
    pub target: WinPath,
    pub staging: WinPath,
    pub bytes: u64,
    pub files: u64,
    /// Contenu vérifié : l'original n'est supprimé que s'il n'a pas changé depuis.
    pub entries: Vec<TreeEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// Confirmation sur un autre plan que celui affiché.
    FingerprintMismatch,
    /// Le recontrôle refuse désormais le déplacement (rien n'a été fait).
    Refused(Vec<Refusal>),
    /// Une étape a échoué ; `rolled_back` : tout est revenu comme avant.
    Step { step: Step, error: String, rolled_back: bool, rollback_error: Option<String> },
}

/// Ce qu'il a fallu faire, pour pouvoir le défaire.
#[derive(Default)]
struct Done {
    target_created: bool,
    set_aside: bool,
    junction: bool,
}

fn rollback(p: &Staged, done: &Done, fx: &dyn MoveEffects) -> Result<(), String> {
    let mut errors = Vec::new();
    if done.junction {
        // Créée à l'instant par ce déplacement : retirée même si elle mène ailleurs (test raté).
        if let Err(e) = fx.remove_junction(&p.source, None) {
            errors.push(format!("jonction : {e}"));
        }
    }
    if done.set_aside && errors.is_empty() {
        if let Err(e) = fx.rename(&p.staging, &p.source) {
            errors.push(format!("remise en place : {e}"));
        }
    }
    // La copie n'est supprimée que si l'original est bien revenu à sa place.
    if done.target_created && errors.is_empty() {
        if let Err(e) = fx.delete_tree(&p.target) {
            errors.push(format!("copie : {e}"));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join(" ; "))
    }
}

/// Même contenu (noms, tailles, dossiers), quel que soit l'ordre de la liste.
fn same_tree(a: &[TreeEntry], b: &[TreeEntry]) -> bool {
    let key = |e: &TreeEntry| (fold(&e.rel), e.size, e.is_dir, e.reparse);
    let mut x: Vec<_> = a.iter().map(key).collect();
    let mut y: Vec<_> = b.iter().map(key).collect();
    x.sort();
    y.sort();
    x == y
}

/// Fichiers lus à travers la jonction pour la tester : les premiers non vides.
fn samples(entries: &[TreeEntry]) -> Vec<&TreeEntry> {
    let mut s: Vec<&TreeEntry> = entries.iter().filter(|e| !e.is_dir && e.size > 0).take(JUNCTION_SAMPLES).collect();
    if s.is_empty() {
        s = entries.iter().filter(|e| !e.is_dir).take(1).collect();
    }
    s
}

/// Étapes 1 à 5 ; en cas d'échec, retour en arrière automatique.
pub fn run(plan: &Prepared, fingerprint: &str, ctx: &MoveContext<'_>, probe: &dyn MoveProbe, fx: &dyn MoveEffects, progress: Progress<'_>) -> Result<Staged, Failure> {
    if fingerprint != plan.fingerprint() {
        return Err(Failure::FingerprintMismatch);
    }
    // 1. Recontrôle complet : mêmes refus que le plan, même contenu.
    progress(Step::Recheck, 0, plan.bytes);
    let fresh = prepare(&plan.source, drive(&plan.target).unwrap_or('?'), ctx, probe).map_err(Failure::Refused)?;
    if fresh.fingerprint() != plan.fingerprint() {
        return Err(Failure::Refused(vec![Refusal::Unreadable("le contenu du dossier a changé depuis la confirmation".into())]));
    }
    let staging = staging_of(&plan.source).ok_or(Failure::Refused(vec![Refusal::StagingExists]))?;
    let staged = Staged { source: plan.source.clone(), target: plan.target.clone(), staging, bytes: plan.bytes, files: plan.files, entries: plan.entries.clone() };
    let mut done = Done::default();
    let fail = |step: Step, error: String, done: &Done| {
        let rb = rollback(&staged, done, fx);
        Failure::Step { step, error, rolled_back: rb.is_ok(), rollback_error: rb.err() }
    };

    // 2. Copie fidèle (dossiers créés d'après leur modèle, puis fichiers, puis dates des dossiers).
    let mut copied = 0u64;
    let mut parents: Vec<WinPath> = Vec::new();
    {
        let mut t = plan.target.volume_root();
        for c in plan.target.components() {
            t = match t.join(c) {
                Ok(n) => n,
                Err(e) => return Err(fail(Step::Copy, e.to_string(), &done)),
            };
            parents.push(t.clone());
        }
    }
    // Dossiers parents de la cible (\Lumen\Déplacés\C\…) : créés s'il le faut, gardés ensuite.
    for p in &parents[..parents.len().saturating_sub(1)] {
        if probe.kind(p).is_none() {
            // Créé entre-temps par un autre déplacement : il suffit qu'il soit un vrai dossier.
            if let Err(e) = fx.create_dir(p, &plan.target.volume_root()) {
                if probe.kind(p) != Some(PathKind::Dir) {
                    return Err(fail(Step::Copy, e.to_string(), &done));
                }
            }
        } else if probe.kind(p) != Some(PathKind::Dir) {
            return Err(fail(Step::Copy, format!("{p} n'est pas un dossier ordinaire"), &done));
        }
    }
    if let Err(e) = fx.create_dir(&plan.target, &plan.source) {
        return Err(fail(Step::Copy, e.to_string(), &done));
    }
    done.target_created = true;
    for e in &plan.entries {
        let (Some(from), Some(to)) = (join_rel(&plan.source, &e.rel), join_rel(&plan.target, &e.rel)) else {
            return Err(fail(Step::Copy, format!("chemin invalide : {}", e.rel), &done));
        };
        let r = if e.is_dir { fx.create_dir(&to, &from) } else { fx.copy_file(&from, &to) };
        if let Err(err) = r {
            return Err(fail(Step::Copy, format!("{} : {err}", e.rel), &done));
        }
        if !e.is_dir {
            copied += e.size;
            progress(Step::Copy, copied, plan.bytes);
        }
    }
    // Dates et ACL des dossiers, des plus profonds au dossier lui-même.
    let mut dirs: Vec<&TreeEntry> = plan.entries.iter().filter(|e| e.is_dir).collect();
    dirs.sort_by_key(|e| std::cmp::Reverse(e.rel.matches('\\').count()));
    for e in dirs {
        if let (Some(from), Some(to)) = (join_rel(&plan.source, &e.rel), join_rel(&plan.target, &e.rel)) {
            if let Err(err) = fx.copy_dir_meta(&from, &to, false) {
                return Err(fail(Step::Copy, format!("{} : {err}", e.rel), &done));
            }
        }
    }
    if let Err(err) = fx.copy_dir_meta(&plan.source, &plan.target, true) {
        return Err(fail(Step::Copy, err.to_string(), &done));
    }

    // 3. Vérification BLAKE3 de chaque fichier.
    let mut verified = 0u64;
    for e in plan.entries.iter().filter(|e| !e.is_dir) {
        let (Some(from), Some(to)) = (join_rel(&plan.source, &e.rel), join_rel(&plan.target, &e.rel)) else { continue };
        match (probe.hash(&from), probe.hash(&to)) {
            (Ok(a), Ok(b)) if a == b => {}
            (Ok(_), Ok(_)) => return Err(fail(Step::Verify, MoveIoError::Mismatch(e.rel.clone()).to_string(), &done)),
            (Err(err), _) | (_, Err(err)) => return Err(fail(Step::Verify, format!("{} : {err}", e.rel), &done)),
        }
        verified += e.size;
        progress(Step::Verify, verified, plan.bytes);
    }

    // 4. L'original mis de côté (même disque : un simple renommage).
    progress(Step::SetAside, plan.bytes, plan.bytes);
    if let Err(err) = fx.rename(&plan.source, &staged.staging) {
        return Err(fail(Step::SetAside, err.to_string(), &done));
    }
    done.set_aside = true;

    // 5. Jonction, puis lecture à travers elle.
    progress(Step::Junction, plan.bytes, plan.bytes);
    if let Err(err) = fx.create_junction(&plan.source, &plan.target) {
        return Err(fail(Step::Junction, err.to_string(), &done));
    }
    done.junction = true;
    progress(Step::Test, plan.bytes, plan.bytes);
    if probe.junction_target(&plan.source).as_ref() != Some(&plan.target) {
        return Err(fail(Step::Test, MoveIoError::BadJunction.to_string(), &done));
    }
    for e in samples(&plan.entries) {
        let (Some(via), Some(orig)) = (join_rel(&plan.source, &e.rel), join_rel(&staged.staging, &e.rel)) else { continue };
        match (probe.hash(&via), probe.hash(&orig)) {
            (Ok(a), Ok(b)) if a == b => {}
            (Ok(_), Ok(_)) => return Err(fail(Step::Test, MoveIoError::Mismatch(e.rel.clone()).to_string(), &done)),
            (Err(err), _) | (_, Err(err)) => return Err(fail(Step::Test, format!("{} : {err}", e.rel), &done)),
        }
    }
    Ok(staged)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finished {
    /// Original supprimé définitivement : l'espace est libéré sur le disque d'origine.
    Done,
    /// Confirmation refusée : retour en arrière fait.
    RolledBack,
    /// La suppression a échoué : la jonction et la copie restent, l'original aussi (rien perdu).
    DeleteFailed(String),
    /// Le retour en arrière a échoué (rare) : à signaler, rien n'est supprimé.
    RollbackFailed(String),
}

/// Étape 6 : `confirmed` = l'utilisateur a confirmé explicitement la suppression définitive de
/// l'original ; sinon retour en arrière.
pub fn finish(staged: &Staged, confirmed: bool, probe: &dyn MoveProbe, fx: &dyn MoveEffects) -> Finished {
    if !confirmed {
        let done = Done { target_created: true, set_aside: true, junction: true };
        return match rollback(staged, &done, fx) {
            Ok(()) => Finished::RolledBack,
            Err(e) => Finished::RollbackFailed(e),
        };
    }
    // Garde-fou final : la jonction mène toujours à la copie, l'original mis de côté est un
    // dossier ordinaire.
    if probe.junction_target(&staged.source).as_ref() != Some(&staged.target) || probe.kind(&staged.staging) != Some(PathKind::Dir) {
        return Finished::DeleteFailed("la jonction ou l'original ont changé : rien n'est supprimé".into());
    }
    if !probe.tree(&staged.staging).is_ok_and(|t| same_tree(&t, &staged.entries)) {
        return Finished::DeleteFailed("l'original a changé depuis la copie : rien n'est supprimé".into());
    }
    match fx.delete_tree(&staged.staging) {
        Ok(()) => Finished::Done,
        Err(e) => Finished::DeleteFailed(e.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Undone {
    /// Dossier revenu à sa place, copie de la cible à la Corbeille.
    Done { recycle_error: Option<String> },
    Refused(String),
    /// Échec : la jonction et la copie restent (rien perdu) ; la recopie partielle est retirée.
    Failed { step: Step, error: String },
}

/// « Annuler le déplacement » : recopie depuis la cible, vérifie, retire la jonction, remet le
/// dossier à sa place, puis envoie la copie de la cible à la Corbeille.
pub fn undo(source: &WinPath, target: &WinPath, probe: &dyn MoveProbe, fx: &dyn MoveEffects, progress: Progress<'_>) -> Undone {
    if probe.junction_target(source).as_ref() != Some(target) {
        return Undone::Refused("l'emplacement d'origine n'est plus la jonction créée par Lumen".into());
    }
    if probe.kind(target) != Some(PathKind::Dir) {
        return Undone::Refused("la copie n'est plus sur le disque cible".into());
    }
    let Some(back) = return_of(source) else { return Undone::Refused("chemin invalide".into()) };
    if probe.kind(&back).is_some() {
        return Undone::Refused("une annulation précédente n'est pas terminée".into());
    }
    let entries = match probe.tree(target) {
        Ok(e) => e,
        Err(e) => return Undone::Refused(e.to_string()),
    };
    if entries.iter().any(|e| e.reparse || e.cloud) {
        return Undone::Refused("la copie contient désormais des liens".into());
    }
    let bytes: u64 = entries.iter().filter(|e| !e.is_dir).map(|e| e.size).sum();
    let free = drive(source).and_then(|d| probe.volume(d)).map_or(0, |v| v.free);
    if free < needed_space(bytes) {
        return Undone::Refused(Refusal::NotEnoughSpace { needed: needed_space(bytes), free }.to_string());
    }
    let paths: Vec<WinPath> = entries.iter().filter(|e| !e.is_dir).filter_map(|e| join_rel(target, &e.rel)).collect();
    let holders = probe.holders(&paths);
    if !holders.is_empty() {
        return Undone::Refused(Refusal::InUse(holders).to_string());
    }
    let cleanup = |step: Step, error: String| {
        let _ = fx.delete_tree(&back);
        Undone::Failed { step, error }
    };
    // Recopie et vérification.
    if let Err(e) = fx.create_dir(&back, target) {
        return Undone::Failed { step: Step::Copy, error: e.to_string() };
    }
    let mut copied = 0u64;
    for e in &entries {
        let (Some(from), Some(to)) = (join_rel(target, &e.rel), join_rel(&back, &e.rel)) else { return cleanup(Step::Copy, e.rel.clone()) };
        let r = if e.is_dir { fx.create_dir(&to, &from) } else { fx.copy_file(&from, &to) };
        if let Err(err) = r {
            return cleanup(Step::Copy, format!("{} : {err}", e.rel));
        }
        copied += e.size;
        progress(Step::Copy, copied, bytes);
    }
    let mut dirs: Vec<&TreeEntry> = entries.iter().filter(|e| e.is_dir).collect();
    dirs.sort_by_key(|e| std::cmp::Reverse(e.rel.matches('\\').count()));
    for e in dirs {
        if let (Some(from), Some(to)) = (join_rel(target, &e.rel), join_rel(&back, &e.rel)) {
            if let Err(err) = fx.copy_dir_meta(&from, &to, false) {
                return cleanup(Step::Copy, err.to_string());
            }
        }
    }
    if let Err(err) = fx.copy_dir_meta(target, &back, true) {
        return cleanup(Step::Copy, err.to_string());
    }
    let mut verified = 0u64;
    for e in entries.iter().filter(|e| !e.is_dir) {
        let (Some(a), Some(b)) = (join_rel(target, &e.rel), join_rel(&back, &e.rel)) else { continue };
        match (probe.hash(&a), probe.hash(&b)) {
            (Ok(x), Ok(y)) if x == y => {}
            _ => return cleanup(Step::Verify, MoveIoError::Mismatch(e.rel.clone()).to_string()),
        }
        verified += e.size;
        progress(Step::Verify, verified, bytes);
    }
    // Jonction retirée, dossier remis à sa place ; en cas d'échec, la jonction est recréée.
    progress(Step::Junction, bytes, bytes);
    if let Err(e) = fx.remove_junction(source, Some(target)) {
        return cleanup(Step::Junction, e.to_string());
    }
    if let Err(e) = fx.rename(&back, source) {
        let _ = fx.create_junction(source, target);
        return cleanup(Step::SetAside, e.to_string());
    }
    // Vérification finale : un dossier ordinaire, le même contenu.
    progress(Step::Test, bytes, bytes);
    let ok = probe.kind(source) == Some(PathKind::Dir) && probe.tree(source).is_ok_and(|t| same_tree(&t, &entries));
    if !ok {
        return Undone::Failed { step: Step::Test, error: "le dossier remis en place ne correspond pas à la copie (la copie est gardée sur le disque cible)".into() };
    }
    progress(Step::Delete, bytes, bytes);
    Undone::Done { recycle_error: fx.recycle(target).err().map(|e| e.to_string()) }
}

/// Reprise d'un déplacement interrompu (Lumen fermé ou arrêté en cours de route).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recovered {
    /// Tout est revenu comme avant le déplacement.
    RolledBack,
    /// Le déplacement était allé au bout (original déjà supprimé, jonction en place).
    Done,
    /// État inattendu : rien n'est touché, à signaler.
    NeedsAttention(String),
}

/// Remet en état un déplacement resté « en cours » : retour en arrière tant que l'original
/// existe (mis de côté ou encore à sa place), jamais de suppression de l'original.
pub fn recover(source: &WinPath, target: &WinPath, probe: &dyn MoveProbe, fx: &dyn MoveEffects) -> Recovered {
    let Some(staging) = staging_of(source) else { return Recovered::NeedsAttention("chemin invalide".into()) };
    let junction = probe.junction_target(source);
    let aside = probe.kind(&staging) == Some(PathKind::Dir);
    let here = probe.kind(source);
    let drop_copy = |rb: Recovered| {
        if probe.kind(target) == Some(PathKind::Dir) {
            if let Err(e) = fx.delete_tree(target) {
                return Recovered::NeedsAttention(format!("copie sur le disque cible : {e}"));
            }
        }
        rb
    };
    match (junction.is_some(), aside, here) {
        // Jonction posée, original mis de côté : on défait les deux.
        (true, true, _) => {
            if let Err(e) = fx.remove_junction(source, None) {
                return Recovered::NeedsAttention(format!("jonction : {e}"));
            }
            if let Err(e) = fx.rename(&staging, source) {
                return Recovered::NeedsAttention(format!("remise en place : {e}"));
            }
            drop_copy(Recovered::RolledBack)
        }
        // Original mis de côté, pas encore de jonction.
        (false, true, None) => match fx.rename(&staging, source) {
            Ok(()) => drop_copy(Recovered::RolledBack),
            Err(e) => Recovered::NeedsAttention(format!("remise en place : {e}")),
        },
        // Interrompu pendant la copie : l'original n'a pas bougé, la copie partielle part.
        (false, false, Some(PathKind::Dir)) => drop_copy(Recovered::RolledBack),
        // Jonction vers la copie, original supprimé : le déplacement était fini.
        (true, false, _) if junction.as_ref() == Some(target) && probe.kind(target) == Some(PathKind::Dir) => Recovered::Done,
        _ => Recovered::NeedsAttention("état inattendu : rien n'est touché".into()),
    }
}

/// État d'un déplacement fait par Lumen, vérifié au démarrage et après chaque analyse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Ok,
    /// Le disque cible n'est pas branché.
    TargetVolumeAbsent,
    /// Le disque est là, mais la copie a disparu.
    TargetMissing,
    /// La jonction n'est plus à l'emplacement d'origine (supprimée ou remplacée).
    JunctionMissing,
}

pub fn health(source: &WinPath, target: &WinPath, probe: &dyn MoveProbe) -> Health {
    let Some(d) = drive(target) else { return Health::TargetVolumeAbsent };
    if probe.volume(d).is_none() {
        return Health::TargetVolumeAbsent;
    }
    if probe.kind(target) != Some(PathKind::Dir) {
        return Health::TargetMissing;
    }
    if probe.junction_target(source).as_ref() != Some(target) {
        return Health::JunctionMissing;
    }
    Health::Ok
}

pub mod raw;

#[cfg(test)]
pub(crate) mod memory;

#[cfg(test)]
mod tests;
