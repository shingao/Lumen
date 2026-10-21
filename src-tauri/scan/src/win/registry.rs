//! Lecture seule du registre (launchers de jeux, distributions WSL) : sous-clés et valeurs texte
//! ou entières, dans la vue 64 bits puis 32 bits (`WOW6432Node`). Rien n'est jamais écrit ; une
//! clé absente ou illisible donne simplement « rien ».

use std::ffi::c_void;

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_ENUMERATE_SUB_KEYS, KEY_QUERY_VALUE,
    KEY_WOW64_32KEY, KEY_WOW64_64KEY, REG_SAM_FLAGS, RRF_RT_REG_DWORD, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6432KEY,
    RRF_SUBKEY_WOW6464KEY,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hive {
    LocalMachine,
    CurrentUser,
}

/// Vue du registre : 64 bits (par défaut) ou 32 bits (`WOW6432Node`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Native,
    Wow32,
}

fn hkey(h: Hive) -> HKEY {
    match h {
        Hive::LocalMachine => HKEY_LOCAL_MACHINE,
        Hive::CurrentUser => HKEY_CURRENT_USER,
    }
}

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY : clé ouverte par RegOpenKeyExW, fermée une seule fois.
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

/// Sous-clés directes de `path` (noms), vide si la clé n'existe pas.
pub fn subkeys(hive: Hive, view: View, path: &str) -> Vec<String> {
    let sam: REG_SAM_FLAGS = KEY_ENUMERATE_SUB_KEYS | KEY_QUERY_VALUE | if view == View::Wow32 { KEY_WOW64_32KEY } else { KEY_WOW64_64KEY };
    let mut raw = HKEY::default();
    // SAFETY : chaîne et poignée possédées.
    if unsafe { RegOpenKeyExW(hkey(hive), &HSTRING::from(path), None, sam, &mut raw) } != ERROR_SUCCESS {
        return Vec::new();
    }
    let key = Key(raw);
    let mut out = Vec::new();
    for index in 0..4096u32 {
        let mut name = [0u16; 256];
        let mut len = name.len() as u32;
        // SAFETY : tampon de la taille annoncée.
        let status = unsafe { RegEnumKeyExW(key.0, index, Some(PWSTR(name.as_mut_ptr())), &mut len, None, None, None, None) };
        if status != ERROR_SUCCESS {
            break;
        }
        out.push(String::from_utf16_lossy(&name[..(len as usize).min(name.len())]));
    }
    out
}

fn view_flag(view: View) -> windows::Win32::System::Registry::REG_ROUTINE_FLAGS {
    if view == View::Wow32 {
        RRF_SUBKEY_WOW6432KEY
    } else {
        RRF_SUBKEY_WOW6464KEY
    }
}

/// Valeur texte (REG_SZ ou REG_EXPAND_SZ, variables développées), 32 Kio au plus.
pub fn string(hive: Hive, view: View, path: &str, name: &str) -> Option<String> {
    let mut buf = vec![0u16; 16 * 1024];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY : chaînes et tampon possédés, taille en octets annoncée.
    let status = unsafe {
        RegGetValueW(
            hkey(hive),
            &HSTRING::from(path),
            &HSTRING::from(name),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | view_flag(view),
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let chars = (size as usize / 2).min(buf.len());
    let text = &buf[..chars];
    let end = text.iter().position(|&c| c == 0).unwrap_or(text.len());
    Some(String::from_utf16_lossy(&text[..end]))
}

/// Valeur entière (REG_DWORD).
pub fn dword(hive: Hive, view: View, path: &str, name: &str) -> Option<u32> {
    let mut value = 0u32;
    let mut size = 4u32;
    // SAFETY : chaînes et entier possédés.
    let status = unsafe {
        RegGetValueW(hkey(hive), &HSTRING::from(path), &HSTRING::from(name), RRF_RT_REG_DWORD | view_flag(view), None, Some(&mut value as *mut u32 as *mut c_void), Some(&mut size))
    };
    (status == ERROR_SUCCESS).then_some(value)
}
