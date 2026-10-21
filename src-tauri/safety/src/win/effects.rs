//! Effets réels de l'exécution (`crate::exec::Effects`) : Corbeille, suppression définitive par
//! handle, espace libre, commandes officielles.
//!
//! - Suppression définitive : du plus profond au plus haut, chaque élément est ouvert sans suivre
//!   les liens, son identité (file ID, taille, date) est relue sur le handle ouvert et comparée à
//!   celle vérifiée, puis il est marqué pour suppression sur ce même handle. Un élément substitué
//!   entre le contrôle et l'action (jonction, autre fichier) n'est donc jamais supprimé ; un
//!   dossier qui a reçu un nouvel élément n'est pas vide et reste en place.
//! - Commandes : binaire résolu par son chemin complet (`Program::candidates`, fichier ordinaire,
//!   pas un lien), lancé sans console ni shell avec ses arguments en tableau, dans un objet job
//!   (arrêt de tout l'arbre de processus au délai maximal ou à l'annulation), sorties capturées.
#![allow(clippy::disallowed_methods)] // Seul lancement de processus de Lumen : liste fermée, chemin complet.

use std::ffi::c_void;
use std::io::Read;
use std::mem::size_of;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    FileDispositionInfo, FileDispositionInfoEx, GetDiskFreeSpaceExW, SetFileInformationByHandle, DELETE, FILE_DISPOSITION_FLAG_DELETE,
    FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE, FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_DISPOSITION_INFO_EX_FLAGS,
    FILE_READ_ATTRIBUTES,
};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows::Win32::UI::Shell::{FOLDERID_LocalAppData, FOLDERID_Profile, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86, FOLDERID_RoamingAppData, FOLDERID_Windows};

use super::{handle_facts, known_folder, open, probe_error, share_all, Handle, WinProbe};
use crate::check::{Expected, ExpectedNode};
use crate::commands::{path_entries, Candidate, OfficialCommand, ToolDirs};
use crate::exec::{CommandRun, CommandStatus, Effects, Recycled, RemoveError, Unit};
use crate::path::WinPath;
use crate::probe::Probe;

/// Sorties gardées par flux (le reste est lu puis ignoré, pour ne jamais bloquer l'outil).
const OUTPUT_LIMIT: usize = 64 * 1024;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Emplacements connus des outils sur cette machine, PATH filtré (jamais le dossier temporaire).
pub fn tool_dirs() -> ToolDirs {
    let mut program_files = Vec::new();
    for id in [&FOLDERID_ProgramFiles, &FOLDERID_ProgramFilesX86] {
        if let Ok(p) = known_folder(id) {
            if !program_files.contains(&p) {
                program_files.push(p);
            }
        }
    }
    let temp: Vec<WinPath> = super::rule_variables().into_iter().filter(|(n, _)| *n == "TEMP").map(|(_, p)| p).collect();
    ToolDirs {
        system_root: known_folder(&FOLDERID_Windows).ok(),
        program_files,
        local_app_data: known_folder(&FOLDERID_LocalAppData).ok(),
        app_data: known_folder(&FOLDERID_RoamingAppData).ok(),
        user_profile: known_folder(&FOLDERID_Profile).ok(),
        path: path_entries(&std::env::var("PATH").unwrap_or_default(), &temp),
    }
}

#[derive(Debug, Clone)]
pub struct WinEffects {
    pub tools: ToolDirs,
}

impl WinEffects {
    pub fn new() -> WinEffects {
        WinEffects { tools: tool_dirs() }
    }

    /// Premier emplacement qui existe vraiment : fichier ordinaire (ni dossier, ni lien, ni cloud),
    /// renvoyé par son chemin réel.
    pub fn resolve(&self, command: &OfficialCommand) -> Option<(WinPath, Vec<String>)> {
        let real_file = |p: &WinPath| -> Option<WinPath> {
            let a = WinProbe.attributes(p).ok()?;
            if a.directory || a.reparse_point || a.cloud {
                return None;
            }
            WinProbe.canonicalize(p).ok()
        };
        command.program().candidates(&self.tools).into_iter().find_map(|Candidate { exe, script }| {
            let exe = real_file(&exe)?;
            match script {
                Some(s) => Some((exe, vec![real_file(&s)?.display_string()])),
                None => Some((exe, Vec::new())),
            }
        })
    }
}

impl Default for WinEffects {
    fn default() -> Self {
        WinEffects::new()
    }
}

impl Effects for WinEffects {
    fn recycle(&self, units: &[Unit]) -> Vec<Result<Recycled, RemoveError>> {
        let items: Vec<(WinPath, u64)> = units.iter().map(|u| (u.path.clone(), u.bytes)).collect();
        super::recycle::recycle_many(&items)
            .into_iter()
            .map(|r| {
                r.map_err(|e| match e {
                    super::recycle::RecycleError::NoRecycleBin(b) => RemoveError::NoRecycleBin(b.to_string()),
                    other => RemoveError::Failed(other.to_string()),
                })
            })
            .collect()
    }

    fn delete(&self, unit: &Unit) -> Result<(), RemoveError> {
        delete_tree(&unit.path, &unit.expected, &unit.children)
    }

    fn free_bytes(&self, path: &WinPath) -> Option<u64> {
        let root = HSTRING::from(path.volume_root().display_string());
        let mut free = 0u64;
        // SAFETY : chaîne et entier possédés.
        unsafe { GetDiskFreeSpaceExW(&root, None, None, Some(&mut free)) }.ok()?;
        Some(free)
    }

    fn run_elevated(&self, command: &OfficialCommand, cancel: &AtomicBool) -> CommandRun {
        let exe = self.resolve(command).map(|(exe, _)| exe);
        super::elevated::run(command, exe, cancel)
    }

    fn run_command(&self, command: &OfficialCommand, cancel: &AtomicBool) -> CommandRun {
        let Some((exe, mut args)) = self.resolve(command) else {
            return CommandRun::new(command, CommandStatus::NotFound);
        };
        args.extend(command.args());
        let cwd = self.tools.user_profile.clone().or_else(|| self.tools.system_root.clone());
        run_process(command, &exe, &args, cwd.as_ref(), Duration::from_secs(command.timeout_secs()), cancel)
    }
}

/// Supprime l'arbre vérifié, du plus profond au plus haut ; s'arrête à un dossier dont un
/// élément n'a pas pu partir (il n'est pas vide).
fn delete_tree(path: &WinPath, expected: &Expected, children: &[ExpectedNode]) -> Result<(), RemoveError> {
    let mut first_error = None;
    for c in children {
        let result = match path.join(&c.name) {
            Ok(p) if c.expected.directory => delete_tree(&p, &c.expected, &c.children),
            Ok(p) => delete_one(&p, &c.expected),
            Err(e) => Err(RemoveError::Failed(e.to_string())),
        };
        if let Err(e) = result {
            first_error.get_or_insert(e);
        }
    }
    match first_error {
        Some(e) => Err(e),
        None => delete_one(path, expected),
    }
}

fn delete_one(path: &WinPath, expected: &Expected) -> Result<(), RemoveError> {
    let h = open(path, DELETE.0 | FILE_READ_ATTRIBUTES.0, share_all()).map_err(|e| RemoveError::Failed(probe_error(&e).to_string()))?;
    let f = handle_facts(&h).map_err(|e| RemoveError::Failed(e.to_string()))?;
    let same = !f.attributes.reparse_point
        && !f.attributes.cloud
        && f.file_id == expected.file_id
        && f.attributes.directory == expected.directory
        && (expected.directory || (f.size == expected.size && f.modified == expected.modified));
    if !same {
        return Err(RemoveError::Changed);
    }
    mark_for_deletion(&h)
}

fn mark_for_deletion(h: &Handle) -> Result<(), RemoveError> {
    let ex = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_INFO_EX_FLAGS(
            FILE_DISPOSITION_FLAG_DELETE.0 | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0 | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE.0,
        ),
    };
    // SAFETY : handle ouvert avec DELETE, structure possédée de la taille annoncée.
    let r = unsafe {
        SetFileInformationByHandle(h.0, FileDispositionInfoEx, &ex as *const _ as *const c_void, size_of::<FILE_DISPOSITION_INFO_EX>() as u32)
    };
    if r.is_ok() {
        return Ok(());
    }
    // Systèmes de fichiers sans FileDispositionInfoEx (FAT, anciens Windows).
    let classic = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY : idem.
    unsafe {
        SetFileInformationByHandle(h.0, FileDispositionInfo, &classic as *const _ as *const c_void, size_of::<FILE_DISPOSITION_INFO>() as u32)
    }
    .map_err(|e| RemoveError::Failed(probe_error(&e).to_string()))
}

/// Objet job : tous les processus lancés par l'outil sont arrêtés avec lui.
struct Job(HANDLE);

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY : handle de job valide ; KILL_ON_JOB_CLOSE arrête ce qui reste.
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

fn job_for(child: &Child) -> Option<Job> {
    // SAFETY : création et configuration d'un job possédé ; handle du processus valide.
    unsafe {
        let job = Job(CreateJobObjectW(None, PCWSTR::null()).ok()?);
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const c_void,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .ok()?;
        AssignProcessToJobObject(job.0, HANDLE(child.as_raw_handle())).ok()?;
        Some(job)
    }
}

fn capture(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buf = [0u8; 8192];
        let mut truncated = false;
        loop {
            match r.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let room = OUTPUT_LIMIT.saturating_sub(kept.len());
                    kept.extend_from_slice(&buf[..n.min(room)]);
                    truncated |= n > room;
                }
            }
        }
        let mut s = String::from_utf8_lossy(&kept).into_owned();
        if truncated {
            s.push_str("\n[… sortie tronquée]");
        }
        s
    })
}

/// Lance un binaire déjà résolu, sans console ni shell, avec un délai maximal.
pub(crate) fn run_process(
    command: &OfficialCommand,
    exe: &WinPath,
    args: &[String],
    cwd: Option<&WinPath>,
    timeout: Duration,
    cancel: &AtomicBool,
) -> CommandRun {
    let started = Instant::now();
    let mut run = CommandRun::new(command, CommandStatus::Error(String::new()));
    run.program = Some(exe.display_string());
    run.args = args.to_vec();

    let mut cmd = std::process::Command::new(exe.display_string());
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(CREATE_NO_WINDOW);
    cmd.env("NO_COLOR", "1");
    if let Some(dir) = cwd {
        cmd.current_dir(dir.display_string());
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            run.status = CommandStatus::Error(e.to_string());
            return run;
        }
    };
    let job = job_for(&child);
    let out = child.stdout.take().map(capture);
    let err = child.stderr.take().map(capture);

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(e) => {
                run.status = CommandStatus::Error(e.to_string());
                break None;
            }
        }
        let cancelled = cancel.load(Ordering::SeqCst);
        if cancelled || started.elapsed() >= timeout {
            match &job {
                // SAFETY : job valide qui contient le processus et ses enfants.
                Some(j) => unsafe {
                    let _ = TerminateJobObject(j.0, 1);
                },
                None => {
                    let _ = child.kill();
                }
            }
            let _ = child.wait();
            run.status = if cancelled { CommandStatus::Cancelled } else { CommandStatus::TimedOut };
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    drop(job);
    run.stdout = out.and_then(|h| h.join().ok()).unwrap_or_default();
    run.stderr = err.and_then(|h| h.join().ok()).unwrap_or_default();
    if let Some(status) = status {
        run.status = match status.code() {
            Some(0) => CommandStatus::Succeeded,
            Some(code) => CommandStatus::Failed(code),
            None => CommandStatus::Error("arrêt sans code de sortie".into()),
        };
    }
    run.duration_ms = started.elapsed().as_millis() as u64;
    run
}
