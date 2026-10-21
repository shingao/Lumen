//! Commandes système « administrateur » (gouffres système), lancées élevées directement par
//! `ShellExecuteEx` (« runas ») : une demande UAC par commande, le binaire par son chemin complet
//! sous `%SystemRoot%`, des arguments fixes issus de la liste fermée. L'assistant MFT n'est pas
//! concerné : il reste mono-tâche.
//!
//! Limites assumées : un processus élevé ne peut pas être arrêté par Lumen non élevé (annulation
//! et délai maximal ne font que cesser d'attendre), et ses sorties ne sont pas capturables ; le
//! code de sortie l'est, et le gain est mesuré par l'espace libre du volume.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL};

use super::win32_code;
use crate::commands::OfficialCommand;
use crate::exec::{CommandRun, CommandStatus};
use crate::path::WinPath;

const ERROR_CANCELLED: u32 = 1223;

struct Process(HANDLE);

impl Drop for Process {
    fn drop(&mut self) {
        // SAFETY : handle de processus valide, fermé une seule fois.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Arguments sans espace, guillemet ni caractère de shell : la ligne de paramètres est sans
/// ambiguïté (`/Online`, `/for=C:`, base64 du script du point de restauration…).
pub(crate) fn plain_args(args: &[String]) -> Option<String> {
    let ok = args
        .iter()
        .all(|a| !a.is_empty() && a.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | ':' | '=' | '+' | '-' | '.' | '_')));
    ok.then(|| args.join(" "))
}

/// Lance `exe` élevé et attend sa fin ; statut et explication éventuelle.
pub(crate) fn launch(exe: &WinPath, args: &[String], show: bool, timeout: Duration, cancel: &AtomicBool) -> (CommandStatus, Option<String>) {
    let Some(params) = plain_args(args) else {
        return (CommandStatus::Error("argument refusé pour une commande élevée".into()), None);
    };
    let params = HSTRING::from(params);
    let file = HSTRING::from(exe.display_string());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: if show { SW_SHOWNORMAL.0 } else { SW_HIDE.0 },
        ..Default::default()
    };
    // SAFETY : structure et chaînes possédées pendant l'appel.
    if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
        return if win32_code(&e) == ERROR_CANCELLED {
            (CommandStatus::Declined, None)
        } else {
            (CommandStatus::Error(e.message()), None)
        };
    }
    if info.hProcess.is_invalid() {
        return (CommandStatus::Error("commande lancée sans handle de processus".into()), None);
    }
    let process = Process(info.hProcess);
    let started = Instant::now();
    loop {
        // SAFETY : handle de processus valide.
        if unsafe { WaitForSingleObject(process.0, 200) } == WAIT_OBJECT_0 {
            break;
        }
        if cancel.load(Ordering::SeqCst) {
            return (CommandStatus::Cancelled, Some("commande système déjà lancée : elle se termine seule".into()));
        }
        if started.elapsed() >= timeout {
            return (CommandStatus::TimedOut, Some("commande système toujours en cours : Lumen a cessé d'attendre".into()));
        }
    }
    let mut code = 0u32;
    // SAFETY : handle de processus valide, entier possédé.
    if let Err(e) = unsafe { GetExitCodeProcess(process.0, &mut code) } {
        return (CommandStatus::Error(e.message()), None);
    }
    let code = code as i32;
    (if code == 0 { CommandStatus::Succeeded } else { CommandStatus::Failed(code) }, None)
}

/// Commande élevée de la liste fermée : seulement un outil système, seulement « administrateur ».
pub(crate) fn run(command: &OfficialCommand, exe: Option<WinPath>, cancel: &AtomicBool) -> CommandRun {
    let mut run = CommandRun::new(command, CommandStatus::Error(String::new()));
    if !command.needs_elevation() || !command.program().is_system_tool() {
        run.status = CommandStatus::Error("commande non autorisée en élévation".into());
        return run;
    }
    let Some(exe) = exe else {
        run.status = CommandStatus::NotFound;
        return run;
    };
    run.program = Some(exe.display_string());
    let started = Instant::now();
    let (status, note) = launch(&exe, &command.args(), command.shows_window(), Duration::from_secs(command.timeout_secs()), cancel);
    run.duration_ms = started.elapsed().as_millis() as u64;
    let code = match status {
        CommandStatus::Succeeded => Some(0),
        CommandStatus::Failed(c) => Some(c),
        _ => None,
    };
    // Sorties non capturables en élévation : on journalise ce que le code de sortie signifie.
    run.stdout = code.and_then(|c| command.exit_meaning(c)).unwrap_or_default().to_owned();
    run.stderr = note.unwrap_or_default();
    run.status = status;
    run
}
