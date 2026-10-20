//! Confiance dans l'exécutable relancé en administrateur pour le « Scan rapide ».
//!
//! Un exécutable que l'utilisateur peut modifier ne doit jamais être élevé : un logiciel malveillant
//! tournant sous son compte le remplacerait (ou poserait une DLL à côté) et obtiendrait les droits
//! administrateur au prochain clic. Avant tout lancement, Lumen vérifie donc que son exécutable :
//! - est installé sous `Program Files` (chemin réel, liens résolus) ;
//! - n'est modifiable ni lui-même, ni son dossier (ajout de fichier : DLL), ni aucun dossier parent
//!   (suppression, changement de droits ou de propriétaire) par l'utilisateur sans ses droits
//!   administrateur — vérifié avec `AccessCheck` sur un jeton où le groupe Administrateurs est
//!   désactivé, comme le jeton filtré de l'UAC.

use std::ffi::c_void;

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::{LocalFree, ERROR_SUCCESS, HANDLE, HLOCAL};
use windows::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
use windows::Win32::Security::{
    AccessCheck, CreateRestrictedToken, CreateWellKnownSid, DuplicateToken, MapGenericMask, SecurityIdentification,
    WinBuiltinAdministratorsSid, CREATE_RESTRICTED_TOKEN_FLAGS, DACL_SECURITY_INFORMATION, GENERIC_MAPPING,
    GROUP_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PRIVILEGE_SET, PSECURITY_DESCRIPTOR, PSID, SID_AND_ATTRIBUTES,
    TOKEN_DUPLICATE, TOKEN_QUERY,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::{FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

use super::Handle;

const FILE_WRITE_DATA: u32 = 0x0002; // dossier : ajouter un fichier
const FILE_APPEND_DATA: u32 = 0x0004; // dossier : ajouter un sous-dossier
const FILE_WRITE_EA: u32 = 0x0010;
const FILE_DELETE_CHILD: u32 = 0x0040;
const DELETE: u32 = 0x0001_0000;
const WRITE_DAC: u32 = 0x0004_0000;
const WRITE_OWNER: u32 = 0x0008_0000;
const MAXIMUM_ALLOWED: u32 = 0x0200_0000;

/// Droits qui permettraient de remplacer l'exécutable.
const EXE_RIGHTS: u32 = FILE_WRITE_DATA | FILE_APPEND_DATA | FILE_WRITE_EA | DELETE | WRITE_DAC | WRITE_OWNER;
/// Dossier de l'exécutable : y poser une DLL, ou en retirer l'exécutable.
const DIR_RIGHTS: u32 = FILE_WRITE_DATA | FILE_APPEND_DATA | FILE_DELETE_CHILD | DELETE | WRITE_DAC | WRITE_OWNER;
/// Dossiers parents : les renommer, les supprimer ou en changer les droits.
const ANCESTOR_RIGHTS: u32 = FILE_DELETE_CHILD | DELETE | WRITE_DAC | WRITE_OWNER;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TrustError {
    #[error("le scan rapide n'est disponible que pour Lumen installé dans Program Files (exécutable : {0})")]
    NotInstalled(String),
    #[error("le scan rapide est désactivé : {0} est modifiable sans droits administrateur")]
    Writable(String),
    #[error("vérification de l'exécutable impossible : {0}")]
    Check(String),
}

/// Chemin réel (liens et noms courts résolus), sans préfixe `\\?\`.
fn final_path(path: &str) -> Result<String, TrustError> {
    super::real_path(path).ok_or_else(|| TrustError::Check(format!("{path} : chemin réel introuvable")))
}

fn known_folder(id: &windows::core::GUID) -> Option<String> {
    // SAFETY : GUID statique ; chaîne libérée par CoTaskMemFree.
    unsafe {
        let p: PWSTR = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const c_void));
        s
    }
}

fn under(path: &str, dir: &str) -> bool {
    let (p, d) = (path.to_lowercase(), dir.trim_end_matches('\\').to_lowercase());
    p.len() > d.len() + 1 && p.starts_with(&d) && p.as_bytes()[d.len()] == b'\\'
}

/// Jeton d'identification de l'utilisateur courant, groupe Administrateurs désactivé.
fn user_token() -> Result<Handle, TrustError> {
    let fail = |what: &str, e: windows::core::Error| TrustError::Check(format!("{what} : {}", e.message()));
    let mut process_token = HANDLE::default();
    // SAFETY : pseudo-handle du processus courant ; jeton fermé par Handle.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_DUPLICATE | TOKEN_QUERY, &mut process_token) }.map_err(|e| fail("jeton", e))?;
    let process_token = Handle(process_token);
    let mut sid = [0u8; 68];
    let mut size = sid.len() as u32;
    // SAFETY : tampon possédé de la taille annoncée.
    unsafe { CreateWellKnownSid(WinBuiltinAdministratorsSid, None, Some(PSID(sid.as_mut_ptr() as *mut c_void)), &mut size) }
        .map_err(|e| fail("SID administrateurs", e))?;
    let disable = [SID_AND_ATTRIBUTES { Sid: PSID(sid.as_mut_ptr() as *mut c_void), Attributes: 0 }];
    let mut restricted = HANDLE::default();
    // SAFETY : jeton valide, tableau possédé.
    unsafe { CreateRestrictedToken(process_token.0, CREATE_RESTRICTED_TOKEN_FLAGS(0), Some(&disable), None, None, &mut restricted) }
        .map_err(|e| fail("jeton restreint", e))?;
    let restricted = Handle(restricted);
    let mut identification = HANDLE::default();
    // SAFETY : jeton valide.
    unsafe { DuplicateToken(restricted.0, SecurityIdentification, &mut identification) }.map_err(|e| fail("jeton d'identification", e))?;
    Ok(Handle(identification))
}

/// Droits que `token` obtiendrait sur `path` (DACL du fichier ou du dossier).
fn granted(token: &Handle, path: &str) -> Result<u32, TrustError> {
    let mut sd = PSECURITY_DESCRIPTOR::default();
    // SAFETY : chaîne possédée ; descripteur libéré par LocalFree.
    let status = unsafe {
        GetNamedSecurityInfoW(
            &HSTRING::from(path),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            None,
            None,
            None,
            None,
            &mut sd,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(TrustError::Check(format!("{path} : droits illisibles ({})", status.0)));
    }
    let mapping = GENERIC_MAPPING { GenericRead: 0x0012_0089, GenericWrite: 0x0012_0116, GenericExecute: 0x0012_00A0, GenericAll: 0x001F_01FF };
    let mut desired = MAXIMUM_ALLOWED;
    // SAFETY : entier et structure possédés.
    unsafe { MapGenericMask(&mut desired, &mapping) };
    let mut privileges = [0u8; 256];
    let mut privileges_len = privileges.len() as u32;
    let mut access = 0u32;
    let mut ok = windows::core::BOOL(0);
    // SAFETY : descripteur valide, jeton d'identification valide, tampons possédés.
    let r = unsafe {
        AccessCheck(
            sd,
            token.0,
            desired,
            &mapping,
            Some(privileges.as_mut_ptr() as *mut PRIVILEGE_SET),
            &mut privileges_len,
            &mut access,
            &mut ok,
        )
    };
    // SAFETY : descripteur alloué par GetNamedSecurityInfoW.
    unsafe { LocalFree(Some(HLOCAL(sd.0))) };
    r.map_err(|e| TrustError::Check(format!("{path} : {}", e.message())))?;
    Ok(if ok.as_bool() { access } else { 0 })
}

/// Vérifie qu'un exécutable peut être relancé en administrateur ; renvoie son chemin réel.
pub fn check_executable(path: &str) -> Result<String, TrustError> {
    let real = final_path(path)?;
    let roots: Vec<String> = [&FOLDERID_ProgramFiles, &FOLDERID_ProgramFilesX86]
        .into_iter()
        .filter_map(known_folder)
        .filter_map(|r| final_path(&r).ok())
        .collect();
    if !roots.iter().any(|r| under(&real, r)) {
        return Err(TrustError::NotInstalled(real));
    }
    let token = user_token()?;
    if granted(&token, &real)? & EXE_RIGHTS != 0 {
        return Err(TrustError::Writable(real));
    }
    let mut dir = real.rsplit_once('\\').map(|(d, _)| d.to_owned()).unwrap_or_default();
    let mut rights = DIR_RIGHTS;
    // Du dossier de l'exécutable jusqu'à la racine du volume incluse.
    loop {
        let check = if dir.len() == 2 { format!("{dir}\\") } else { dir.clone() };
        if granted(&token, &check)? & rights != 0 {
            return Err(TrustError::Writable(check));
        }
        rights = ANCESTOR_RIGHTS;
        match dir.rsplit_once('\\') {
            Some((parent, _)) if !parent.is_empty() => dir = parent.to_owned(),
            _ => break,
        }
    }
    Ok(real)
}

/// Exécutable de Lumen, s'il peut servir d'assistant élevé.
pub fn helper_executable() -> Result<String, TrustError> {
    let exe = std::env::current_exe().map_err(|e| TrustError::Check(e.to_string()))?;
    let exe = exe.to_str().ok_or_else(|| TrustError::Check("chemin non Unicode".into()))?;
    check_executable(exe)
}

/// Pour les tests : chemin réel d'un fichier.
#[cfg(test)]
pub(crate) fn real_path(path: &str) -> Result<String, TrustError> {
    final_path(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SetNamedSecurityInfoW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{GetSecurityDescriptorDacl, ACL, PROTECTED_DACL_SECURITY_INFORMATION};

    fn this_exe() -> String {
        std::env::current_exe().unwrap().to_str().unwrap().to_owned()
    }

    #[test]
    fn executable_hors_program_files_refuse() {
        let dir = tempfile::Builder::new().prefix("lumen-confiance-").tempdir().unwrap();
        let copy = dir.path().join("lumen.exe");
        fs::write(&copy, fs::read(this_exe()).unwrap()).unwrap();
        let r = check_executable(copy.to_str().unwrap());
        assert!(matches!(r, Err(TrustError::NotInstalled(_))), "{r:?}");
    }

    #[test]
    fn executable_de_program_files_accepte() {
        let pf = known_folder(&FOLDERID_ProgramFiles).unwrap();
        let candidates = ["Windows Defender\\MpCmdRun.exe", "Windows NT\\Accessories\\wordpad.exe", "Internet Explorer\\iexplore.exe"];
        let exe = candidates.iter().map(|c| format!("{pf}\\{c}")).find(|p| Path::new(p).exists()).expect("un exécutable système sous Program Files");
        let real = check_executable(&exe).unwrap();
        assert!(real.to_lowercase().ends_with(".exe"));
        // Le même fichier désigné par un nom court 8.3 ou une casse différente reste reconnu.
        assert_eq!(check_executable(&exe.to_uppercase()).unwrap().to_lowercase(), real.to_lowercase());
    }

    #[test]
    fn dossier_program_files_inscriptible_refuse() {
        if !super::super::is_elevated() {
            assert!(std::env::var("CI").is_err(), "la CI doit lancer ce test en administrateur");
            eprintln!("ignoré : créer un dossier sous Program Files demande les droits administrateur");
            return;
        }
        let pf = known_folder(&FOLDERID_ProgramFiles).unwrap();
        let dir = tempfile::Builder::new().prefix("lumen-test-").tempdir_in(&pf).unwrap();
        // Dossier ouvert à tous les utilisateurs : exactement ce que Lumen doit refuser.
        let sddl = HSTRING::from("D:P(A;OICI;FA;;;BU)(A;OICI;FA;;;BA)(A;OICI;FA;;;SY)");
        let mut sd = PSECURITY_DESCRIPTOR::default();
        // SAFETY : chaîne possédée ; descripteur libéré par LocalFree.
        unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(&sddl, SDDL_REVISION_1, &mut sd, None) }.unwrap();
        let mut present = windows::core::BOOL(0);
        let mut defaulted = windows::core::BOOL(0);
        let mut dacl: *mut ACL = std::ptr::null_mut();
        // SAFETY : descripteur valide.
        unsafe { GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted) }.unwrap();
        // SAFETY : chemin possédé, ACL valide tant que le descripteur vit.
        let status = unsafe {
            SetNamedSecurityInfoW(
                &HSTRING::from(dir.path().to_str().unwrap()),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                None,
                None,
                Some(dacl),
                None,
            )
        };
        assert_eq!(status, ERROR_SUCCESS);
        // SAFETY : descripteur alloué par la conversion.
        unsafe { LocalFree(Some(HLOCAL(sd.0))) };
        let copy = dir.path().join("lumen.exe");
        fs::write(&copy, fs::read(this_exe()).unwrap()).unwrap();
        let r = check_executable(copy.to_str().unwrap());
        assert!(matches!(r, Err(TrustError::Writable(_))), "{r:?}");
    }

    #[test]
    fn executable_de_test_non_installe() {
        // L'exécutable des tests (dossier target\) ne doit jamais être élevé.
        assert!(check_executable(&this_exe()).is_err());
        assert!(real_path(&this_exe()).is_ok());
    }
}
