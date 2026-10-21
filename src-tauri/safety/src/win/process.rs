//! Programmes en cours (garde des jeux et launchers) et ouverture d'un launcher : adresse Steam
//! de la liste fermée, ou exécutable de launcher connu sous Program Files, par `ShellExecuteEx`
//! (jamais élevé, aucun argument).

use windows::core::{w, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::guard::RunningProcess;
use crate::launch::{LaunchRefusal, LauncherOpen};
use crate::path::WinPath;

struct Snapshot(HANDLE);

impl Drop for Snapshot {
    fn drop(&mut self) {
        // SAFETY : poignée ouverte par CreateToolhelp32Snapshot, fermée une seule fois.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn text(buf: &[u16]) -> String {
    String::from_utf16_lossy(&buf[..buf.iter().position(|&c| c == 0).unwrap_or(buf.len())])
}

fn image_path(pid: u32) -> Option<WinPath> {
    // SAFETY : droits minimaux (lecture du nom seulement) ; poignée fermée juste après.
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = vec![0u16; 32_768];
    let mut len = buf.len() as u32;
    // SAFETY : tampon de la taille annoncée.
    let ok = unsafe { QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len) };
    // SAFETY : poignée ouverte ci-dessus.
    let _ = unsafe { CloseHandle(h) };
    ok.ok()?;
    WinPath::parse(&String::from_utf16_lossy(&buf[..(len as usize).min(buf.len())])).ok()
}

/// Programmes en cours : nom de l'exécutable, et chemin complet quand Windows le donne.
pub fn running_processes() -> Vec<RunningProcess> {
    // SAFETY : sans paramètre variable ; poignée fermée par Snapshot.
    let Ok(h) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else { return Vec::new() };
    let snap = Snapshot(h);
    let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
    let mut out = Vec::new();
    // SAFETY : structure possédée, taille annoncée.
    let mut ok = unsafe { Process32FirstW(snap.0, &mut entry) }.is_ok();
    while ok {
        out.push(RunningProcess { name: text(&entry.szExeFile), path: image_path(entry.th32ProcessID) });
        // SAFETY : idem.
        ok = unsafe { Process32NextW(snap.0, &mut entry) }.is_ok();
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OpenError {
    #[error("{0}")]
    Refused(#[from] LaunchRefusal),
    #[error("launcher introuvable")]
    Missing,
    #[error("aucune appli n'ouvre cette adresse (Steam est-il installé ?)")]
    NoHandler,
    #[error("ouverture impossible : {0}")]
    Failed(String),
}

/// Ouvre le launcher (désinstaller ou déplacer un jeu se fait toujours chez lui).
pub fn open_launcher(target: &LauncherOpen) -> Result<(), OpenError> {
    let layout = super::system_layout().map_err(|e| OpenError::Failed(e.to_string()))?;
    target.validate(&layout.program_files)?;
    let file = match target {
        LauncherOpen::App(exe) => {
            // Le fichier existe, n'est pas un dossier, et ni lui ni ses parents ne sont des liens.
            let probe = super::WinProbe;
            let attrs = |p: &WinPath| crate::probe::Probe::attributes(&probe, p);
            match attrs(exe) {
                Ok(a) if !a.directory && !a.reparse_point => {}
                _ => return Err(OpenError::Missing),
            }
            if exe.chain().iter().any(|p| attrs(p).is_ok_and(|a| a.reparse_point)) {
                return Err(OpenError::Refused(LaunchRefusal::OutsideProgramFiles));
            }
            HSTRING::from(exe.display_string())
        }
        other => HSTRING::from(other.uri().unwrap_or_default()),
    };
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC,
        lpVerb: w!("open"),
        lpFile: PCWSTR(file.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY : structure et chaînes possédées pendant l'appel.
    match unsafe { ShellExecuteExW(&mut info) } {
        Ok(()) => Ok(()),
        // ERROR_NO_ASSOCIATION : aucune appli enregistrée pour steam://.
        Err(e) if super::win32_code(&e) == 1155 => Err(OpenError::NoHandler),
        Err(e) => Err(OpenError::Failed(e.message())),
    }
}
