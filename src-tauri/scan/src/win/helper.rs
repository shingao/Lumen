//! Assistant élevé du « Scan rapide (admin) » : muet et éphémère.
//!
//! L'interface ne tourne jamais en administrateur. Pour lire la MFT, Lumen se relance avec `runas`
//! (une demande UAC) : ce processus élevé est l'assistant. Un processus élevé qui écoute sur un
//! canal serait une porte d'élévation de privilèges ; l'assistant :
//! - ne fait qu'une chose : lire la MFT d'un volume, désigné par sa seule lettre (disque fixe NTFS,
//!   jamais un chemin), envoyer le résultat et quitter. Le dossier demandé est extrait ensuite par
//!   Lumen, sans privilège ([`Scan::subtree`]) ;
//! - n'écoute rien : Lumen (non élevé) crée le tube, l'assistant s'y connecte en client, au niveau
//!   d'emprunt « identification » (le serveur ne peut jamais agir en son nom) ;
//! - ne lit sur le tube que 32 octets, comparés au jeton reçu en ligne de commande : tout autre
//!   contenu (une « commande ») est refusé et l'assistant quitte sans rien faire ;
//! - vérifie que le serveur du tube est bien le processus Lumen qui l'a lancé (PID et exécutable) ;
//! - quitte seul si l'échange n'est pas fait sous 10 s, si Lumen se ferme, et après le scan.
//!
//! Côté Lumen : exécutable vérifié avant le lancement ([`super::trust`]) ; tube au nom aléatoire,
//! première et unique instance, clients distants refusés, DACL réservée à l'utilisateur courant ;
//! le client doit être le processus lancé (PID du handle rendu par `ShellExecuteEx`) et présenter
//! son jeton à usage unique, sinon tout s'arrête. Le résultat est validé au décodage
//! (`model::wire`) et `safety` revérifie de toute façon chaque élément avant toute action.

use std::ffi::c_void;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use windows::core::{w, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{LocalFree, GENERIC_READ, GENERIC_WRITE, HANDLE, HLOCAL, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
use windows::Win32::Security::{GetTokenInformation, TokenUser, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, GetDriveTypeW, GetVolumeInformationW, QueryDosDeviceW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_SHARE_NONE,
    OPEN_EXISTING, PIPE_ACCESS_DUPLEX, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId, GetNamedPipeServerProcessId,
    PeekNamedPipe, SetNamedPipeHandleState, PIPE_NOWAIT, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_WAIT,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetExitCodeProcess, GetProcessId, OpenProcess, OpenProcessToken,
    QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

use super::{long_path, mft, trust, win32_code, Handle};
use crate::model::{wire, Scan};
use crate::walk::{Progress, ScanError};

/// Argument qui fait démarrer l'exécutable en assistant, au lieu de l'interface.
pub const HELPER_ARG: &str = "--lumen-mft-scan";
/// Délai maximal entre le lancement de l'assistant et la fin de l'échange des jetons.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
const TOKEN_LEN: usize = 32;
const ERROR_CANCELLED: u32 = 1223;
const ERROR_FILE_NOT_FOUND: u32 = 2;
const ERROR_PIPE_BUSY: u32 = 231;
const ERROR_PIPE_CONNECTED: u32 = 535;
const ERROR_PIPE_LISTENING: u32 = 536;

const FRAME_PROGRESS: u8 = 1;
const FRAME_RESULT: u8 = 2;
const FRAME_ERROR: u8 = 3;
/// Première octet d'une trame d'erreur : erreur ordinaire, ou MFT dont la forme n'est pas gérée.
const ERROR_KIND_OTHER: u8 = 0;
const ERROR_KIND_UNSUPPORTED: u8 = 1;

/// Codes de sortie de l'assistant.
pub mod exit {
    pub const OK: i32 = 0;
    pub const SCAN: i32 = 1;
    pub const ARGS: i32 = 2;
    pub const VOLUME: i32 = 3;
    pub const TIMEOUT: i32 = 4;
    pub const PEER: i32 = 5;
    pub const HANDSHAKE: i32 = 6;
    pub const PARENT: i32 = 7;
}

/// Tout ce que l'assistant reçoit : une lettre, le tube, deux jetons, le PID de Lumen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperArgs {
    pub drive: char,
    pub pipe: String,
    pub client_token: [u8; TOKEN_LEN],
    pub server_token: [u8; TOKEN_LEN],
    pub parent: u32,
}

impl HelperArgs {
    pub fn to_args(&self) -> Vec<String> {
        vec![
            HELPER_ARG.to_owned(),
            self.drive.to_string(),
            self.pipe.clone(),
            hex(&self.client_token),
            hex(&self.server_token),
            self.parent.to_string(),
        ]
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex<const N: usize>(s: &str) -> Option<[u8; N]> {
    if s.len() != N * 2 || !s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) {
        return None;
    }
    let mut out = [0u8; N];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

/// Octets aléatoires du générateur cryptographique du système.
fn random_bytes<const N: usize>() -> Result<[u8; N], ScanError> {
    let mut b = [0u8; N];
    // SAFETY : tampon possédé.
    let status = unsafe { BCryptGenRandom(None, &mut b, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    if status.is_err() {
        return Err(ScanError::Mft("générateur aléatoire indisponible".into()));
    }
    Ok(b)
}

/// Comparaison en temps constant.
fn same_token(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn pipe_name(id: &str) -> String {
    format!("\\\\.\\pipe\\lumen-mft-{id}")
}

/// Arguments de l'assistant, tous obligatoires et au format exact ; tout le reste est refusé.
pub fn parse_args(args: &[String]) -> Option<HelperArgs> {
    if args.len() != 7 || args[1] != HELPER_ARG {
        return None;
    }
    let mut d = args[2].chars();
    let drive = match (d.next(), d.next()) {
        (Some(c), None) if c.is_ascii_uppercase() => c,
        _ => return None,
    };
    unhex::<16>(&args[3])?;
    let parent = &args[6];
    if parent.is_empty() || parent.len() > 10 || !parent.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let parent: u32 = parent.parse().ok().filter(|&p| p != 0)?;
    Some(HelperArgs { drive, pipe: args[3].clone(), client_token: unhex(&args[4])?, server_token: unhex(&args[5])?, parent })
}

/// Le volume est-il un disque fixe NTFS (ni réseau, ni amovible, ni lecteur SUBST) ?
pub fn validate_volume(drive: char) -> Result<(), String> {
    if !drive.is_ascii_uppercase() {
        return Err(format!("lettre de lecteur invalide : {drive}"));
    }
    let root = HSTRING::from(format!("{drive}:\\"));
    // SAFETY : chaîne possédée.
    if unsafe { GetDriveTypeW(&root) } != 3 {
        return Err(format!("{drive}: n'est pas un disque fixe"));
    }
    let mut target = [0u16; 1024];
    // SAFETY : chaîne et tampon possédés.
    let n = unsafe { QueryDosDeviceW(&HSTRING::from(format!("{drive}:")), Some(&mut target)) } as usize;
    if n == 0 || String::from_utf16_lossy(&target[..n.min(target.len())]).starts_with("\\??\\") {
        return Err(format!("{drive}: est un lecteur SUBST ou introuvable"));
    }
    let mut fs = [0u16; 64];
    // SAFETY : chaîne et tampon possédés.
    unsafe { GetVolumeInformationW(&root, None, None, None, None, Some(&mut fs)) }.map_err(|e| format!("{drive}: {}", e.message()))?;
    let fs = String::from_utf16_lossy(&fs[..fs.iter().position(|&c| c == 0).unwrap_or(fs.len())]);
    if fs != "NTFS" {
        return Err(format!("{drive}: n'est pas en NTFS ({fs})"));
    }
    Ok(())
}

/// DACL du tube : l'utilisateur courant seulement.
struct Descriptor(PSECURITY_DESCRIPTOR);

impl Drop for Descriptor {
    fn drop(&mut self) {
        // SAFETY : descripteur alloué par ConvertStringSecurityDescriptorToSecurityDescriptorW.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(self.0 .0)));
        }
    }
}

fn user_sid() -> Result<String, ScanError> {
    let fail = |e: windows::core::Error| ScanError::Mft(format!("identité de l'utilisateur : {}", e.message()));
    let mut token = HANDLE::default();
    // SAFETY : pseudo-handle du processus courant ; jeton fermé par Handle.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.map_err(fail)?;
    let token = Handle(token);
    let mut buf = vec![0u64; 64];
    let mut size = 0u32;
    // SAFETY : tampon possédé (aligné sur 8 octets), taille annoncée.
    unsafe { GetTokenInformation(token.0, TokenUser, Some(buf.as_mut_ptr() as *mut c_void), (buf.len() * 8) as u32, &mut size) }.map_err(fail)?;
    // SAFETY : le tampon contient une structure TOKEN_USER valide.
    let sid = unsafe { (*(buf.as_ptr() as *const TOKEN_USER)).User.Sid };
    let mut text = PWSTR::null();
    // SAFETY : SID valide ; chaîne libérée par LocalFree.
    unsafe { ConvertSidToStringSidW(sid, &mut text) }.map_err(fail)?;
    // SAFETY : chaîne terminée par un zéro, allouée par le système.
    let s = unsafe { text.to_string() }.map_err(|_| ScanError::Mft("SID illisible".into()));
    // SAFETY : chaîne allouée par ConvertSidToStringSidW.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(text.0 as *mut c_void)));
    }
    s
}

fn pipe_security() -> Result<Descriptor, ScanError> {
    let sddl = HSTRING::from(format!("D:P(A;;GA;;;{})", user_sid()?));
    let mut sd = PSECURITY_DESCRIPTOR::default();
    // SAFETY : chaîne possédée ; descripteur libéré par Descriptor.
    unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(&sddl, SDDL_REVISION_1, &mut sd, None) }
        .map_err(|e| ScanError::Mft(format!("droits du tube : {}", e.message())))?;
    Ok(Descriptor(sd))
}

/// Pourquoi l'échange avec l'assistant a échoué.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Handshake {
    Timeout,
    /// L'assistant s'est arrêté avant l'échange (code de sortie).
    HelperExited(u32),
    /// Un autre processus que l'assistant lancé s'est connecté.
    ForeignClient(u32),
    BadToken,
    Cancelled,
    Io(String),
}

impl From<Handshake> for ScanError {
    fn from(h: Handshake) -> ScanError {
        match h {
            Handshake::Cancelled => ScanError::Cancelled,
            Handshake::Timeout => ScanError::Mft("l'assistant administrateur n'a pas répondu à temps".into()),
            Handshake::HelperExited(code) => ScanError::Mft(match code as i32 {
                exit::VOLUME => "volume refusé par l'assistant (disque fixe NTFS seulement)".into(),
                exit::PEER | exit::HANDSHAKE => "l'assistant a refusé le canal de Lumen".into(),
                c => format!("l'assistant s'est arrêté (code {c})"),
            }),
            Handshake::ForeignClient(pid) => {
                ScanError::Mft(format!("un autre programme (PID {pid}) a tenté de se connecter à l'assistant : scan rapide interrompu"))
            }
            Handshake::BadToken => ScanError::Mft("jeton de l'assistant invalide : scan rapide interrompu".into()),
            Handshake::Io(e) => ScanError::Mft(e),
        }
    }
}

/// Octets disponibles dans le tube, sans les lire ; `None` si le tube est rompu.
fn available(pipe: HANDLE) -> Option<u32> {
    let mut avail = 0u32;
    // SAFETY : handle de tube valide, entier possédé.
    unsafe { PeekNamedPipe(pipe, None, 0, None, Some(&mut avail), None) }.ok()?;
    Some(avail)
}

/// Lit exactement `TOKEN_LEN` octets avant `deadline`.
fn read_token(pipe: &mut File, deadline: Instant, alive: &dyn Fn() -> bool) -> Result<[u8; TOKEN_LEN], Handshake> {
    let h = HANDLE(pipe.as_raw_handle());
    loop {
        match available(h) {
            Some(n) if n as usize >= TOKEN_LEN => break,
            Some(_) => {}
            None => return Err(Handshake::Io("tube fermé pendant l'échange".into())),
        }
        if Instant::now() >= deadline || !alive() {
            return Err(Handshake::Timeout);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut token = [0u8; TOKEN_LEN];
    pipe.read_exact(&mut token).map_err(|e| Handshake::Io(e.to_string()))?;
    Ok(token)
}

/// Tube créé par Lumen (non élevé) pour un seul lancement de l'assistant.
pub struct Channel {
    pub name: String,
    pipe: Handle,
    client_token: [u8; TOKEN_LEN],
    server_token: [u8; TOKEN_LEN],
}

impl Channel {
    pub fn create() -> Result<Channel, ScanError> {
        let id: [u8; 16] = random_bytes()?;
        let name = pipe_name(&hex(&id));
        let sd = pipe_security()?;
        let sa = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd.0 .0,
            bInheritHandle: false.into(),
        };
        // SAFETY : chaîne et attributs possédés pendant l'appel. Une seule instance, la première,
        // jamais à distance ; mode non bloquant le temps de l'échange (délais maîtrisés).
        let h = unsafe {
            CreateNamedPipeW(
                &HSTRING::from(name.as_str()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_NOWAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                64 * 1024,
                4 * 1024 * 1024,
                0,
                Some(&sa),
            )
        };
        if h.is_invalid() {
            return Err(ScanError::Mft("tube de l'assistant impossible à créer".into()));
        }
        Ok(Channel { name, pipe: Handle(h), client_token: random_bytes()?, server_token: random_bytes()? })
    }

    pub fn helper_args(&self, drive: char) -> HelperArgs {
        let pipe = self.name.rsplit('-').next().unwrap_or_default().to_owned();
        // SAFETY : sans paramètre.
        let parent = unsafe { GetCurrentProcessId() };
        HelperArgs { drive, pipe, client_token: self.client_token, server_token: self.server_token, parent }
    }

    /// Attend qu'un client se connecte ; renvoie son PID.
    pub(crate) fn wait_client(&self, deadline: Instant, alive: &dyn Fn() -> bool, cancel: &AtomicBool) -> Result<u32, Handshake> {
        loop {
            // SAFETY : handle de tube valide (mode non bloquant : retour immédiat).
            match unsafe { ConnectNamedPipe(self.pipe.0, None) } {
                Err(e) if win32_code(&e) == ERROR_PIPE_CONNECTED => break,
                Err(e) if win32_code(&e) != ERROR_PIPE_LISTENING => return Err(Handshake::Io(e.message())),
                _ => {}
            }
            if cancel.load(Ordering::Relaxed) {
                return Err(Handshake::Cancelled);
            }
            if !alive() {
                return Err(Handshake::HelperExited(0));
            }
            if Instant::now() >= deadline {
                return Err(Handshake::Timeout);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let mut pid = 0u32;
        // SAFETY : handle de tube connecté, entier possédé.
        unsafe { GetNamedPipeClientProcessId(self.pipe.0, &mut pid) }.map_err(|e| Handshake::Io(e.message()))?;
        Ok(pid)
    }

    /// Vérifie le client (processus lancé, jeton) et répond avec le jeton de Lumen.
    pub fn accept(self, expected_pid: u32, alive: &dyn Fn() -> bool, cancel: &AtomicBool, timeout: Duration) -> Result<File, Handshake> {
        let deadline = Instant::now() + timeout;
        let pid = self.wait_client(deadline, alive, cancel)?;
        if pid != expected_pid {
            // SAFETY : handle de tube valide.
            let _ = unsafe { DisconnectNamedPipe(self.pipe.0) };
            return Err(Handshake::ForeignClient(pid));
        }
        let (mut pipe, client_token, server_token) = self.into_file()?;
        let token = read_token(&mut pipe, deadline, alive)?;
        if !same_token(&token, &client_token) {
            return Err(Handshake::BadToken);
        }
        pipe.write_all(&server_token).map_err(|e| Handshake::Io(e.to_string()))?;
        Ok(pipe)
    }

    /// Repasse le tube en mode bloquant et le confie à un `File`.
    pub(crate) fn into_file(self) -> Result<(File, [u8; TOKEN_LEN], [u8; TOKEN_LEN]), Handshake> {
        let mode = PIPE_READMODE_BYTE | PIPE_WAIT;
        // SAFETY : handle de tube valide, mode possédé.
        unsafe { SetNamedPipeHandleState(self.pipe.0, Some(&mode), None, None) }.map_err(|e| Handshake::Io(e.message()))?;
        let Channel { pipe, client_token, server_token, .. } = self;
        let raw = pipe.0;
        std::mem::forget(pipe);
        // SAFETY : handle de tube valide, désormais possédé par le File (fermé à sa destruction).
        Ok((unsafe { File::from_raw_handle(raw.0) }, client_token, server_token))
    }

    #[cfg(test)]
    pub(crate) fn tokens(&self) -> ([u8; TOKEN_LEN], [u8; TOKEN_LEN]) {
        (self.client_token, self.server_token)
    }
}

/// Chemin de l'exécutable d'un processus, en noms longs et minuscules.
fn process_image(pid: u32) -> Option<String> {
    // SAFETY : accès minimal ; handle fermé par Handle.
    let h = Handle(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?);
    let mut buf = vec![0u16; 32_768];
    let mut size = buf.len() as u32;
    // SAFETY : handle valide, tampon possédé de la taille annoncée.
    unsafe { QueryFullProcessImageNameW(h.0, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut size) }.ok()?;
    Some(long_path(&String::from_utf16_lossy(&buf[..size as usize])).to_lowercase())
}

/// Côté assistant : se connecte au tube de Lumen, vérifie le serveur et échange les jetons.
fn connect_to_lumen(args: &HelperArgs, deadline: Instant) -> Result<File, i32> {
    let name = HSTRING::from(pipe_name(&args.pipe));
    let mut pipe = loop {
        // SAFETY : chaîne possédée. Niveau d'emprunt « identification » : le serveur du tube ne
        // peut pas agir avec les droits de l'assistant.
        let r = unsafe {
            CreateFileW(
                &name,
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                None,
            )
        };
        match r {
            // SAFETY : handle valide, désormais possédé par le File.
            Ok(h) => break unsafe { File::from_raw_handle(h.0) },
            Err(e) if matches!(win32_code(&e), ERROR_FILE_NOT_FOUND | ERROR_PIPE_BUSY) && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) if Instant::now() >= deadline => return Err(exit::TIMEOUT),
            Err(_) => return Err(exit::PEER),
        }
    };
    let mut server = 0u32;
    // SAFETY : handle de tube valide, entier possédé.
    unsafe { GetNamedPipeServerProcessId(HANDLE(pipe.as_raw_handle()), &mut server) }.map_err(|_| exit::PEER)?;
    // Le serveur doit être Lumen lui-même : le PID reçu, et le même exécutable que l'assistant.
    let own = std::env::current_exe().ok().and_then(|p| p.to_str().map(|s| long_path(s).to_lowercase()));
    if server != args.parent || own.is_none() || process_image(server) != own {
        return Err(exit::PEER);
    }
    pipe.write_all(&args.client_token).map_err(|_| exit::PEER)?;
    let token = read_token(&mut pipe, deadline, &|| true).map_err(|e| if e == Handshake::Timeout { exit::TIMEOUT } else { exit::PEER })?;
    if !same_token(&token, &args.server_token) {
        // Aucune commande n'existe : tout ce qui n'est pas le jeton attendu est refusé.
        return Err(exit::HANDSHAKE);
    }
    Ok(pipe)
}

/// Surveille l'assistant : arrêt si l'échange n'est pas fait à temps ou si Lumen se ferme.
fn watchdog(started: Instant, ready: Arc<AtomicBool>, parent: isize) {
    let parent = HANDLE(parent as *mut c_void);
    loop {
        std::thread::sleep(Duration::from_millis(100));
        if !ready.load(Ordering::Relaxed) && started.elapsed() >= HANDSHAKE_TIMEOUT {
            std::process::exit(exit::TIMEOUT);
        }
        // SAFETY : handle de processus valide pour toute la vie de l'assistant.
        if unsafe { WaitForSingleObject(parent, 0) } == WAIT_OBJECT_0 {
            std::process::exit(exit::PARENT);
        }
    }
}

fn write_frame(out: &Mutex<BufWriter<File>>, tag: u8, payload: &[u8]) -> bool {
    let Ok(mut w) = out.lock() else { return false };
    w.write_all(&[tag]).is_ok()
        && w.write_all(&(payload.len() as u32).to_le_bytes()).is_ok()
        && w.write_all(payload).is_ok()
        && w.flush().is_ok()
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
pub fn run_helper(argv: &[String]) -> i32 {
    let started = Instant::now();
    let Some(args) = parse_args(argv) else { return exit::ARGS };
    if validate_volume(args.drive).is_err() {
        return exit::VOLUME;
    }
    // SAFETY : droit d'attente seulement ; handle gardé ouvert jusqu'à la fin du processus.
    let Ok(parent) = (unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, args.parent) }) else { return exit::PEER };
    let ready = Arc::new(AtomicBool::new(false));
    {
        let ready = ready.clone();
        let parent = parent.0 as isize;
        std::thread::spawn(move || watchdog(started, ready, parent));
    }
    let pipe = match connect_to_lumen(&args, started + HANDSHAKE_TIMEOUT) {
        Ok(p) => p,
        Err(code) => return code,
    };
    ready.store(true, Ordering::Relaxed);

    // Plus rien n'est lu sur le tube : l'assistant ne fait qu'écrire.
    let out = Mutex::new(BufWriter::new(pipe));
    let broken = AtomicBool::new(false);
    let root = format!("{}:\\", args.drive);
    let result = mft::scan_mft(&root, &broken, &|p: &Progress| {
        let mut b = Vec::with_capacity(33);
        for v in [p.files, p.dirs, p.allocated, p.elapsed_ms] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(phase_code(p.phase));
        if !write_frame(&out, FRAME_PROGRESS, &b) {
            // Lumen ne lit plus : on s'arrête.
            broken.store(true, Ordering::Relaxed);
        }
    });
    match result {
        Ok(scan) => {
            let mut b = Vec::new();
            if wire::encode(&scan, &mut b).is_ok() && write_frame(&out, FRAME_RESULT, &b) {
                exit::OK
            } else {
                exit::SCAN
            }
        }
        Err(e) => {
            let kind = if matches!(e, ScanError::MftUnsupported(_)) { ERROR_KIND_UNSUPPORTED } else { ERROR_KIND_OTHER };
            let mut b = vec![kind];
            b.extend_from_slice(e.to_string().as_bytes());
            write_frame(&out, FRAME_ERROR, &b);
            exit::SCAN
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
                return wire::decode(&mut body).map_err(|e| ScanError::Mft(format!("résultat invalide : {e}")));
            }
            FRAME_ERROR if len >= 1 => {
                let mut b = vec![0u8; len.min(64 * 1024) as usize];
                r.read_exact(&mut b).map_err(|e| ScanError::Mft(e.to_string()))?;
                let text = String::from_utf8_lossy(&b[1..]).into_owned();
                return Err(if b[0] == ERROR_KIND_UNSUPPORTED { ScanError::MftUnsupported(text) } else { ScanError::Mft(text) });
            }
            _ => return Err(ScanError::Mft("trame inconnue".into())),
        }
    }
}

fn exit_code(process: &Handle) -> u32 {
    let mut code = 0u32;
    // SAFETY : handle de processus valide, entier possédé.
    let _ = unsafe { GetExitCodeProcess(process.0, &mut code) };
    code
}

/// Lance l'assistant (par `launch`), vérifie le canal et reçoit le résultat. L'assistant ne survit
/// jamais à un échec : il est arrêté au moindre écart.
pub(crate) fn run_with(
    drive: char,
    cancel: &AtomicBool,
    on_progress: &(dyn Fn(&Progress) + Sync),
    launch: impl FnOnce(&[String]) -> Result<Handle, ScanError>,
) -> Result<Scan, ScanError> {
    let channel = Channel::create()?;
    let args = channel.helper_args(drive);
    let process = launch(&args.to_args())?;
    // SAFETY : handle de processus valide.
    let pid = unsafe { GetProcessId(process.0) };
    // SAFETY : handle de processus valide.
    let alive = || unsafe { WaitForSingleObject(process.0, 0) } == WAIT_TIMEOUT;
    let result = match channel.accept(pid, &alive, cancel, HANDSHAKE_TIMEOUT) {
        Ok(pipe) => receive(pipe, &process, cancel, on_progress),
        Err(Handshake::HelperExited(_)) => Err(Handshake::HelperExited(exit_code(&process)).into()),
        Err(e) => Err(e.into()),
    };
    if result.is_err() {
        // SAFETY : handle de processus valide.
        let _ = unsafe { TerminateProcess(process.0, 1) };
    }
    result
}

fn receive(pipe: File, process: &Handle, cancel: &AtomicBool, on_progress: &(dyn Fn(&Progress) + Sync)) -> Result<Scan, ScanError> {
    std::thread::scope(|s| {
        let reader = s.spawn(move || read_frames(pipe, on_progress));
        loop {
            // SAFETY : handle de processus valide.
            let wait = unsafe { WaitForSingleObject(process.0, 100) };
            if cancel.load(Ordering::Relaxed) {
                // SAFETY : handle de processus valide ; le tube se rompt, la lecture s'arrête.
                let _ = unsafe { TerminateProcess(process.0, 1) };
            }
            if wait != WAIT_TIMEOUT || reader.is_finished() {
                break;
            }
        }
        let result = reader.join().map_err(|_| ScanError::Mft("fil de lecture arrêté".into()))?;
        if cancel.load(Ordering::Relaxed) {
            return Err(ScanError::Cancelled);
        }
        result
    })
}

fn launch_elevated(exe: &str, args: &[String]) -> Result<Handle, ScanError> {
    // Arguments sans espace ni guillemet (lettre, hexadécimal, nombre) : aucune ambiguïté.
    let params = HSTRING::from(args.join(" "));
    let file = HSTRING::from(exe);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };
    // SAFETY : structure et chaînes possédées pendant l'appel.
    if let Err(e) = unsafe { ShellExecuteExW(&mut info) } {
        return Err(if win32_code(&e) == ERROR_CANCELLED { ScanError::NotElevated } else { ScanError::Mft(e.message()) });
    }
    if info.hProcess.is_invalid() {
        return Err(ScanError::Mft("assistant lancé sans handle de processus".into()));
    }
    Ok(Handle(info.hProcess))
}

/// Lit la MFT du volume `drive` par l'assistant élevé (une demande UAC).
pub fn scan_volume_elevated(drive: char, cancel: &AtomicBool, on_progress: &(dyn Fn(&Progress) + Sync)) -> Result<Scan, ScanError> {
    let drive = drive.to_ascii_uppercase();
    validate_volume(drive).map_err(ScanError::Mft)?;
    let exe = trust::helper_executable().map_err(|e| ScanError::Mft(e.to_string()))?;
    run_with(drive, cancel, on_progress, |args| launch_elevated(&exe, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::win::is_elevated;
    use std::os::windows::ffi::OsStrExt;

    use windows::Win32::Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS};
    use windows::Win32::Storage::FileSystem::{DefineDosDeviceW, GetLogicalDrives, DDD_REMOVE_DEFINITION, DEFINE_DOS_DEVICE_FLAGS};
    use windows::Win32::System::Threading::{CreateProcessW, CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION, STARTUPINFOW};

    const CHILD_ENV: &str = "LUMEN_HELPER_CHILD";

    /// Point d'entrée de l'assistant pour les tests bout en bout : ne fait rien dans une exécution
    /// normale ; relancé avec `LUMEN_HELPER_CHILD`, il joue l'assistant et quitte avec son code.
    #[test]
    fn point_d_entree_assistant() {
        let Ok(line) = std::env::var(CHILD_ENV) else { return };
        let mut argv = vec!["lumen.exe".to_owned()];
        argv.extend(line.split('\u{1f}').map(str::to_owned));
        std::process::exit(run_helper(&argv));
    }

    /// Lance l'exécutable des tests en assistant (sans élévation : inutile pour l'échange).
    fn spawn_child(helper_args: &[String]) -> Result<Handle, ScanError> {
        let exe = std::env::current_exe().map_err(|e| ScanError::Mft(e.to_string()))?;
        let mut cmd: Vec<u16> = format!("\"{}\" win::helper::tests::point_d_entree_assistant --exact --nocapture --test-threads=1", exe.display())
            .encode_utf16()
            .chain([0])
            .collect();
        let mut env: Vec<u16> = Vec::new();
        for (k, v) in std::env::vars_os() {
            if k.to_string_lossy().eq_ignore_ascii_case(CHILD_ENV) {
                continue;
            }
            env.extend(k.encode_wide());
            env.push(u16::from(b'='));
            env.extend(v.encode_wide());
            env.push(0);
        }
        env.extend(format!("{CHILD_ENV}={}", helper_args.join("\u{1f}")).encode_utf16());
        env.extend([0, 0]);
        let si = STARTUPINFOW { cb: std::mem::size_of::<STARTUPINFOW>() as u32, ..Default::default() };
        let mut pi = PROCESS_INFORMATION::default();
        // SAFETY : tampons possédés pendant l'appel ; handles fermés par Handle.
        unsafe {
            CreateProcessW(
                PCWSTR::null(),
                Some(PWSTR(cmd.as_mut_ptr())),
                None,
                None,
                false,
                CREATE_UNICODE_ENVIRONMENT,
                Some(env.as_ptr() as *const c_void),
                PCWSTR::null(),
                &si,
                &mut pi,
            )
        }
        .map_err(|e| ScanError::Mft(e.message()))?;
        drop(Handle(pi.hThread));
        Ok(Handle(pi.hProcess))
    }

    fn wait_exit(process: &Handle, max: Duration) -> (i32, Duration) {
        let started = Instant::now();
        // SAFETY : handle de processus valide.
        let r = unsafe { WaitForSingleObject(process.0, max.as_millis() as u32) };
        assert_eq!(r, WAIT_OBJECT_0, "l'assistant ne s'est pas arrêté");
        (exit_code(process) as i32, started.elapsed())
    }

    fn free_letter() -> char {
        // SAFETY : sans paramètre.
        let used = unsafe { GetLogicalDrives() };
        ('H'..='Y').rev().find(|d| used & (1 << (*d as u32 - 'A' as u32)) == 0).unwrap()
    }

    fn valid_args() -> HelperArgs {
        HelperArgs { drive: 'C', pipe: hex(&[7u8; 16]), client_token: [1; 32], server_token: [2; 32], parent: 1234 }
    }

    #[test]
    fn arguments_de_l_assistant() {
        let good = valid_args();
        let mut argv = vec!["lumen.exe".to_owned()];
        argv.extend(good.to_args());
        assert_eq!(parse_args(&argv), Some(good.clone()));
        let with = |i: usize, v: &str| {
            let mut a = argv.clone();
            a[i] = v.to_owned();
            a
        };
        for bad in [
            with(2, "c"),                          // minuscule
            with(2, "CD"),                         // pas une lettre
            with(2, "1"),
            with(2, "C:\\Users\\Arthur"),          // jamais un chemin
            with(3, "court"),
            with(4, &"z".repeat(64)),              // pas de l'hexadécimal
            with(5, &"0".repeat(63)),
            with(6, "0"),
            with(6, "12a"),
            with(1, "--autre-commande"),
        ] {
            assert!(parse_args(&bad).is_none(), "{bad:?}");
        }
        let mut extra = argv.clone();
        extra.push("C:\\Windows".into());
        assert!(parse_args(&extra).is_none(), "argument en trop");
        assert!(parse_args(&argv[..6]).is_none(), "argument manquant");
        assert!(same_token(&[1, 2, 3], &[1, 2, 3]) && !same_token(&[1, 2, 3], &[1, 2, 4]));
        let a: [u8; 32] = random_bytes().unwrap();
        let b: [u8; 32] = random_bytes().unwrap();
        assert_ne!(a, b);
    }

    struct Subst(char);

    impl Drop for Subst {
        fn drop(&mut self) {
            // SAFETY : retire la définition créée par le test.
            unsafe {
                let _ = DefineDosDeviceW(DDD_REMOVE_DEFINITION, &HSTRING::from(format!("{}:", self.0)), PCWSTR::null());
            }
        }
    }

    #[test]
    fn lettre_de_volume_invalide_refusee() {
        assert_eq!(validate_volume('C'), Ok(()));
        assert!(validate_volume('c').is_err());
        assert!(validate_volume('1').is_err());
        let unused = free_letter();
        assert!(validate_volume(unused).is_err(), "lecteur absent");
        let dir = tempfile::tempdir().unwrap();
        let subst = Subst(unused);
        // SAFETY : chaînes possédées ; définition retirée par Subst.
        unsafe { DefineDosDeviceW(DEFINE_DOS_DEVICE_FLAGS(0), &HSTRING::from(format!("{unused}:")), &HSTRING::from(dir.path().to_str().unwrap())) }
            .unwrap();
        assert!(validate_volume(subst.0).is_err(), "lecteur SUBST refusé");

        // Bout en bout : l'assistant refuse et quitte aussitôt, sans attendre aucun tube.
        let channel = Channel::create().unwrap();
        let mut a = channel.helper_args('C');
        a.drive = subst.0;
        let child = spawn_child(&a.to_args()).unwrap();
        let (code, took) = wait_exit(&child, Duration::from_secs(5));
        assert_eq!(code, exit::VOLUME);
        assert!(took < Duration::from_secs(5));
        let mut a = a.to_args();
        a[1] = "C:\\Windows".into();
        let child = spawn_child(&a).unwrap();
        assert_eq!(wait_exit(&child, Duration::from_secs(5)).0, exit::ARGS);
    }

    fn connect_foreign(name: &str) -> File {
        // SAFETY : chaîne possédée.
        let h = unsafe {
            CreateFileW(&HSTRING::from(name), (GENERIC_READ | GENERIC_WRITE).0, FILE_SHARE_NONE, None, OPEN_EXISTING, SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION, None)
        }
        .unwrap();
        // SAFETY : handle valide, possédé par le File.
        unsafe { File::from_raw_handle(h.0) }
    }

    #[test]
    fn client_etranger_refuse() {
        // Un programme se connecte au tube avant l'assistant : Lumen le voit et arrête tout.
        let channel = Channel::create().unwrap();
        let foreign = connect_foreign(&channel.name);
        let helper = channel.helper_args('C');
        let child = spawn_child(&helper.to_args()).unwrap();
        // SAFETY : handle de processus valide.
        let pid = unsafe { GetProcessId(child.0) };
        let r = channel.accept(pid, &|| true, &AtomicBool::new(false), HANDSHAKE_TIMEOUT);
        // SAFETY : sans paramètre.
        assert_eq!(r.err(), Some(Handshake::ForeignClient(unsafe { GetCurrentProcessId() })));
        drop(foreign);
        // L'assistant ne trouve jamais de tube valide et s'arrête seul.
        let (code, took) = wait_exit(&child, Duration::from_secs(20));
        assert_eq!(code, exit::TIMEOUT);
        assert!(took <= Duration::from_secs(15), "{took:?}");

        // Le bon processus, mais un jeton faux : refusé aussi.
        let channel = Channel::create().unwrap();
        let name = channel.name.clone();
        let fake = std::thread::spawn(move || {
            let mut f = connect_foreign(&name);
            f.write_all(&[0xAA; TOKEN_LEN]).unwrap();
            f
        });
        // SAFETY : sans paramètre.
        let own = unsafe { GetCurrentProcessId() };
        let r = channel.accept(own, &|| true, &AtomicBool::new(false), HANDSHAKE_TIMEOUT);
        assert_eq!(r.err(), Some(Handshake::BadToken));
        drop(fake.join().unwrap());
    }

    #[test]
    fn commande_inconnue_refusee() {
        // Lumen (ici le test) répond par une « commande » au lieu du jeton : l'assistant refuse
        // et quitte sans rien lire d'autre ni rien analyser.
        let channel = Channel::create().unwrap();
        let (client_token, _) = channel.tokens();
        let child = spawn_child(&channel.helper_args('C').to_args()).unwrap();
        // SAFETY : handle de processus valide.
        let pid = unsafe { GetProcessId(child.0) };
        let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
        assert_eq!(channel.wait_client(deadline, &|| true, &AtomicBool::new(false)), Ok(pid));
        let (mut pipe, _, _) = channel.into_file().unwrap();
        assert_eq!(read_token(&mut pipe, deadline, &|| true).unwrap(), client_token, "l'assistant présente son jeton");
        let mut command = [0u8; TOKEN_LEN];
        command[..24].copy_from_slice(b"SCAN C:\\Windows\\System32");
        pipe.write_all(&command).unwrap();
        let (code, took) = wait_exit(&child, Duration::from_secs(10));
        assert_eq!(code, exit::HANDSHAKE);
        assert!(took < Duration::from_secs(5), "{took:?}");
        let mut rest = Vec::new();
        let _ = pipe.read_to_end(&mut rest);
        assert!(rest.is_empty(), "l'assistant n'a rien envoyé après le refus");
    }

    #[test]
    fn assistant_arrete_seul_apres_le_delai() {
        // Personne ne répond : ni tube (nom inconnu), ni jeton (tube muet).
        let lonely = HelperArgs { pipe: hex(&random_bytes::<16>().unwrap()), ..Channel::create().unwrap().helper_args('C') };
        let silent = Channel::create().unwrap();
        let started = Instant::now();
        let a = spawn_child(&lonely.to_args()).unwrap();
        let b = spawn_child(&silent.helper_args('C').to_args()).unwrap();
        for child in [a, b] {
            // Durée comptée depuis le lancement (le second assistant a pu finir pendant l'attente du premier).
            let (code, _) = wait_exit(&child, Duration::from_secs(20));
            let took = started.elapsed();
            assert_eq!(code, exit::TIMEOUT);
            assert!(took >= Duration::from_secs(8) && took <= Duration::from_secs(16), "{took:?}");
        }
        drop(silent);
    }

    #[test]
    fn assistant_bout_en_bout() {
        if !is_elevated() {
            assert!(std::env::var("CI").is_err(), "la CI doit lancer ce test en administrateur");
            eprintln!("ignoré : la lecture de la MFT demande les droits administrateur");
            return;
        }
        // Un fichier témoin sur le volume lu (les autres tests écrivent en parallèle sur ce volume :
        // seul ce fichier a un contenu connu).
        let base = std::env::var("LUMEN_TEST_NTFS").ok();
        let dir = match &base {
            Some(v) => tempfile::Builder::new().prefix("lumen-e2e-").tempdir_in(v).unwrap(),
            None => tempfile::Builder::new().prefix("lumen-e2e-").tempdir().unwrap(),
        };
        let witness = dir.path().join("temoin.bin");
        std::fs::write(&witness, vec![3u8; 70_000]).unwrap();
        let real = super::super::root_path(witness.to_str().unwrap());
        let drive = real.chars().next().unwrap().to_ascii_uppercase();
        let mut launched = None;
        let scan = run_with(drive, &AtomicBool::new(false), &|_: &Progress| {}, |a| {
            let h = spawn_child(a)?;
            let mut copy = HANDLE::default();
            // SAFETY : handles valides ; copie fermée par Handle.
            unsafe { DuplicateHandle(GetCurrentProcess(), h.0, GetCurrentProcess(), &mut copy, 0, false, DUPLICATE_SAME_ACCESS) }
                .map_err(|e| ScanError::Mft(e.message()))?;
            launched = Some(Handle(copy));
            Ok(h)
        })
        .unwrap();
        // L'assistant quitte de lui-même, avec succès, dès le résultat envoyé.
        let (code, _) = wait_exit(&launched.unwrap(), Duration::from_secs(10));
        assert_eq!(code, exit::OK);
        assert_eq!(scan.root_path, format!("{drive}:\\"));
        let id = scan.find(&real[3..]).expect("fichier témoin trouvé dans la MFT lue par l'assistant");
        let n = scan.node(id).unwrap();
        assert_eq!(n.logical, 70_000);
        assert!(n.allocated >= 70_000);
    }
}
