//! Assistant élevé : l'interface ne tourne jamais en administrateur. Quand l'utilisateur demande
//! le « Scan rapide (admin) », Lumen se relance lui-même avec `runas` (une demande UAC), avec pour
//! seule mission de lire la MFT d'un volume et de renvoyer le résultat par un tube nommé.
//!
//! L'assistant ne prend qu'une lettre de lecteur, un jeton aléatoire et un dossier à analyser ;
//! il ne fait que lire des métadonnées. Le résultat reçu est validé avant usage (`model::wire`) et
//! `safety` revérifie de toute façon chaque élément sur le disque avant toute action.

use std::collections::hash_map::RandomState;
use std::fs::{File, OpenOptions};
use std::hash::{BuildHasher, Hasher};
use std::io::{BufReader, BufWriter, Read, Write};
use std::os::windows::io::FromRawHandle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use windows::core::{w, HSTRING};
use windows::Win32::Foundation::{HANDLE, WAIT_TIMEOUT};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::Win32::System::Threading::{TerminateProcess, WaitForSingleObject};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

use super::{mft, win32_code, Handle};
use crate::model::{wire, Scan};
use crate::walk::{Progress, ScanError};

/// Argument qui fait démarrer l'exécutable en assistant, au lieu de l'interface.
pub const HELPER_ARG: &str = "--lumen-mft-scan";
const ERROR_CANCELLED: u32 = 1223;

const FRAME_PROGRESS: u8 = 1;
const FRAME_RESULT: u8 = 2;
const FRAME_ERROR: u8 = 3;

fn pipe_name(nonce: &str) -> String {
    format!("\\\\.\\pipe\\lumen-mft-{nonce}")
}

fn random_nonce() -> String {
    let a = RandomState::new().build_hasher().finish();
    let mut h = RandomState::new().build_hasher();
    h.write_u64(a);
    format!("{a:016x}{:016x}", h.finish())
}

/// Arguments de l'assistant : lecteur, jeton, dossier. Tout le reste est refusé.
pub fn parse_args(args: &[String]) -> Option<(char, String, String)> {
    if args.len() != 5 || args[1] != HELPER_ARG {
        return None;
    }
    let mut d = args[2].chars();
    let drive = match (d.next(), d.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => c.to_ascii_uppercase(),
        _ => return None,
    };
    let nonce = args[3].clone();
    if nonce.len() != 32 || !nonce.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut root = args[4].clone();
    if root.len() == 2 {
        root.push('\\');
    }
    let prefix = format!("{drive}:\\");
    let ok = root.to_uppercase().starts_with(&prefix)
        && root.len() < 32_000
        && !root.chars().any(|c| (c as u32) < 0x20 || matches!(c, '"' | '<' | '>' | '|' | '?' | '*'))
        && !root.split('\\').any(|p| p == "..");
    ok.then_some((drive, nonce, root))
}

fn write_frame(out: &Mutex<BufWriter<File>>, tag: u8, payload: &[u8]) {
    if let Ok(mut w) = out.lock() {
        let _ = w.write_all(&[tag]);
        let _ = w.write_all(&(payload.len() as u32).to_le_bytes());
        let _ = w.write_all(payload);
        let _ = w.flush();
    }
}

fn phase_code(phase: &str) -> u8 {
    match phase {
        "mft-read" => 0,
        "mft-build" => 1,
        _ => 2,
    }
}

fn phase_name(code: u8) -> &'static str {
    match code {
        0 => "mft-read",
        1 => "mft-build",
        _ => "mft",
    }
}

/// Point d'entrée de l'assistant (processus élevé). Renvoie le code de sortie.
pub fn run_helper(args: &[String]) -> i32 {
    let Some((_drive, nonce, root)) = parse_args(args) else { return 2 };
    let Ok(pipe) = OpenOptions::new().write(true).open(pipe_name(&nonce)) else { return 3 };
    let out = Mutex::new(BufWriter::new(pipe));
    let cancel = AtomicBool::new(false);
    let result = mft::scan_mft(&root, &cancel, &|p: &Progress| {
        let mut b = Vec::with_capacity(33);
        for v in [p.files, p.dirs, p.allocated, p.elapsed_ms] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(phase_code(p.phase));
        write_frame(&out, FRAME_PROGRESS, &b);
    });
    match result {
        Ok(scan) => {
            let mut b = Vec::new();
            if wire::encode(&scan, &mut b).is_ok() {
                write_frame(&out, FRAME_RESULT, &b);
            }
            0
        }
        Err(e) => {
            write_frame(&out, FRAME_ERROR, e.to_string().as_bytes());
            1
        }
    }
}

fn read_frames(pipe: File, on_progress: &(dyn Fn(&Progress) + Sync)) -> Result<Scan, ScanError> {
    let mut r = BufReader::new(pipe);
    loop {
        let mut head = [0u8; 5];
        r.read_exact(&mut head).map_err(|_| ScanError::Mft("l'assistant s'est arrêté sans résultat".into()))?;
        // Longueur sur 32 bits : la trame de résultat est lue au plus jusqu'à cette borne.
        let len = u32::from_le_bytes([head[1], head[2], head[3], head[4]]);
        match head[0] {
            FRAME_PROGRESS if len == 33 => {
                let mut b = [0u8; 33];
                r.read_exact(&mut b).map_err(|e| ScanError::Mft(e.to_string()))?;
                let v = |i: usize| u64::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3], b[i + 4], b[i + 5], b[i + 6], b[i + 7]]);
                on_progress(&Progress {
                    files: v(0),
                    dirs: v(8),
                    allocated: v(16),
                    errors: 0,
                    elapsed_ms: v(24),
                    tops: Vec::new(),
                    phase: phase_name(b[32]),
                });
            }
            FRAME_RESULT => {
                let mut body = (&mut r).take(u64::from(len));
                let scan = wire::decode(&mut body).map_err(|e| ScanError::Mft(format!("résultat invalide : {e}")))?;
                return Ok(scan);
            }
            FRAME_ERROR => {
                let mut b = vec![0u8; len.min(64 * 1024) as usize];
                r.read_exact(&mut b).map_err(|e| ScanError::Mft(e.to_string()))?;
                return Err(ScanError::Mft(String::from_utf8_lossy(&b).into_owned()));
            }
            _ => return Err(ScanError::Mft("trame inconnue".into())),
        }
    }
}

/// Lance l'assistant élevé (une demande UAC) et reçoit son résultat.
pub fn scan_mft_elevated(root: &str, cancel: &AtomicBool, on_progress: &(dyn Fn(&Progress) + Sync)) -> Result<Scan, ScanError> {
    let drive = root.chars().next().filter(char::is_ascii_alphabetic).ok_or_else(|| ScanError::Mft("racine invalide".into()))?;
    let nonce = random_nonce();
    let name = pipe_name(&nonce);
    // SAFETY : chaîne possédée ; un seul client, jamais à distance, première instance exigée.
    let pipe = unsafe {
        CreateNamedPipeW(
            &HSTRING::from(name.as_str()),
            PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            0,
            4 * 1024 * 1024,
            0,
            None,
        )
    };
    if pipe.is_invalid() {
        return Err(ScanError::Mft("tube de l'assistant impossible à créer".into()));
    }
    // Le handle passe dans le fil de lecture sous forme d'entier (HANDLE n'est pas Send).
    let raw_pipe = pipe.0 as isize;
    // SAFETY : handle de tube valide, désormais possédé par le File (fermé à sa destruction).
    let pipe_file = unsafe { File::from_raw_handle(pipe.0) };

    let exe = std::env::current_exe().map_err(|e| ScanError::Mft(e.to_string()))?;
    let root_arg = root.trim_end_matches('\\');
    let params = HSTRING::from(format!("{HELPER_ARG} {drive} {nonce} \"{root_arg}\""));
    let file = HSTRING::from(exe.as_os_str());
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: windows::core::PCWSTR(file.as_ptr()),
        lpParameters: windows::core::PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    // SAFETY : structure et chaînes possédées pendant l'appel.
    if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
        return Err(if win32_code(&e) == ERROR_CANCELLED { ScanError::NotElevated } else { ScanError::Mft(e.message()) });
    }
    let process = Handle(info.hProcess);

    std::thread::scope(|s| {
        let reader = s.spawn(move || {
            // SAFETY : handle de tube valide (possédé par pipe_file, vivant dans ce fil).
            let _ = unsafe { ConnectNamedPipe(HANDLE(raw_pipe as *mut std::ffi::c_void), None) };
            read_frames(pipe_file, on_progress)
        });
        loop {
            // SAFETY : handle de processus valide.
            let wait = unsafe { WaitForSingleObject(process.0, 100) };
            if cancel.load(Ordering::Relaxed) {
                // SAFETY : handle de processus valide.
                let _ = unsafe { TerminateProcess(process.0, 1) };
            }
            if wait != WAIT_TIMEOUT {
                break;
            }
        }
        // Assistant terminé avant de se connecter : on débloque l'attente du tube.
        if !reader.is_finished() {
            let _ = OpenOptions::new().write(true).open(&name);
        }
        let result = reader.join().map_err(|_| ScanError::Mft("fil de lecture arrêté".into()))?;
        if cancel.load(Ordering::Relaxed) {
            return Err(ScanError::Cancelled);
        }
        result
    })
}

#[cfg(test)]
mod unit {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn arguments_de_l_assistant_valides() {
        let nonce = "0123456789abcdef0123456789abcdef";
        assert_eq!(parse_args(&args(&["lumen.exe", HELPER_ARG, "c", nonce, "C:"])), Some(('C', nonce.into(), "C:\\".into())));
        assert!(parse_args(&args(&["lumen.exe", HELPER_ARG, "C", nonce, "C:\\Users\\Arthur"])).is_some());
        for bad in [
            ["lumen.exe", HELPER_ARG, "CD", nonce, "C:"],
            ["lumen.exe", HELPER_ARG, "C", "court", "C:"],
            ["lumen.exe", HELPER_ARG, "C", nonce, "D:\\x"],
            ["lumen.exe", HELPER_ARG, "C", nonce, "C:\\x\\..\\..\\Windows"],
            ["lumen.exe", HELPER_ARG, "C", nonce, "C:\\a\" --autre"],
            ["lumen.exe", "--autre", "C", nonce, "C:"],
        ] {
            assert!(parse_args(&args(&bad)).is_none(), "{bad:?}");
        }
        assert_eq!(random_nonce().len(), 32);
    }
}
