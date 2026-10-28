//! Entrées de Lumen dans le registre de l'utilisateur (HKCU), sans droits d'administrateur :
//! « Analyser avec Lumen » dans le menu contextuel de l'Explorateur (dossiers, lecteurs, fond
//! d'un dossier) et l'entrée Run du démarrage de Windows. Jamais de service ni de tâche planifiée.
//!
//! Le modèle (clés, valeurs, lignes de commande) est pur et testé avec `MemHive` ; `WinHive`
//! (Windows) l'applique à HKCU. L'installeur écrit et retire les mêmes clés
//! (`installer/hooks.nsh`).

#[cfg(test)]
use std::collections::BTreeMap;
use std::io;

use super::launch::{BACKGROUND_ARG, OPEN_ARG};

/// Menu contextuel classique : sous Windows 11, dans « Afficher plus d'options ».
pub const MENU_KEYS: [&str; 3] = [
    r"Software\Classes\Directory\shell\Lumen",
    r"Software\Classes\Directory\Background\shell\Lumen",
    r"Software\Classes\Drive\shell\Lumen",
];
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
/// Même nom que celui que retire la désinstallation (nom du produit).
pub const RUN_VALUE: &str = "Lumen";

/// Accès minimal à une ruche : valeurs texte seulement.
pub trait Hive {
    fn get(&self, key: &str, name: &str) -> Option<String>;
    fn set(&mut self, key: &str, name: &str, value: &str) -> io::Result<()>;
    fn delete_value(&mut self, key: &str, name: &str) -> io::Result<()>;
    /// Supprime la clé et ses sous-clés ; absente : rien à faire.
    fn delete_key(&mut self, key: &str) -> io::Result<()>;
}

/// `"C:\Program Files\Lumen\lumen.exe" --lumen-open "%V"` : `%V` est le dossier cliqué (ou le
/// dossier ouvert, pour le fond), toujours entre guillemets.
pub fn menu_command(exe: &str) -> String {
    format!("\"{exe}\" {OPEN_ARG} \"%V\"")
}

pub fn run_command(exe: &str) -> String {
    format!("\"{exe}\" {BACKGROUND_ARG}")
}

fn command_key(key: &str) -> String {
    format!(r"{key}\command")
}

/// Exécutable d'une commande du registre : `"C:\…\lumen.exe" --lumen-…` → `C:\…\lumen.exe`.
pub fn command_exe(command: &str) -> Option<&str> {
    let rest = command.trim_start().strip_prefix('"')?;
    rest.split('"').next().filter(|exe| !exe.is_empty())
}

/// Une commande de Lumen dont l'exécutable existe encore. Une entrée laissée par une
/// installation supprimée (autre compte, profil non chargé à la désinstallation) ne lance plus
/// rien : Windows échoue sans rien exécuter. Lumen la voit comme absente et la réécrit si on
/// réactive le réglage.
fn live(command: Option<String>, arg: &str, exists: &dyn Fn(&str) -> bool) -> bool {
    command.is_some_and(|c| c.contains(arg) && command_exe(&c).is_some_and(exists))
}

/// Le menu est en place si les trois entrées existent et lancent un Lumen présent.
pub fn menu_registered(h: &dyn Hive, exists: &dyn Fn(&str) -> bool) -> bool {
    MENU_KEYS.iter().all(|k| live(h.get(&command_key(k), ""), OPEN_ARG, exists))
}

pub fn register_menu(h: &mut dyn Hive, exe: &str, label: &str) -> io::Result<()> {
    for key in MENU_KEYS {
        h.set(key, "", label)?;
        h.set(key, "Icon", &format!("\"{exe}\",0"))?;
        h.set(&command_key(key), "", &menu_command(exe))?;
    }
    Ok(())
}

/// Retire les trois entrées (seulement les nôtres : une clé « Lumen » qui ne lance pas Lumen
/// n'est pas touchée).
pub fn unregister_menu(h: &mut dyn Hive) -> io::Result<()> {
    for key in MENU_KEYS {
        let ours = h.get(&command_key(key), "").map_or(true, |c| c.contains(OPEN_ARG));
        if ours {
            h.delete_key(key)?;
        }
    }
    Ok(())
}

/// Langue de l'interface changée : libellé du menu mis à jour, commande inchangée.
pub fn relabel_menu(h: &mut dyn Hive, label: &str, exists: &dyn Fn(&str) -> bool) -> io::Result<()> {
    if !menu_registered(h, exists) {
        return Ok(());
    }
    for key in MENU_KEYS {
        if h.get(key, "").as_deref() != Some(label) {
            h.set(key, "", label)?;
        }
    }
    Ok(())
}

pub fn autostart_registered(h: &dyn Hive, exists: &dyn Fn(&str) -> bool) -> bool {
    live(h.get(RUN_KEY, RUN_VALUE), BACKGROUND_ARG, exists)
}

pub fn set_autostart(h: &mut dyn Hive, exe: &str, on: bool) -> io::Result<()> {
    if on {
        h.set(RUN_KEY, RUN_VALUE, &run_command(exe))
    } else if h.get(RUN_KEY, RUN_VALUE).is_some() {
        h.delete_value(RUN_KEY, RUN_VALUE)
    } else {
        Ok(())
    }
}

/// Ruche en mémoire (tests).
#[cfg(test)]
#[derive(Debug, Default)]
pub struct MemHive {
    pub values: BTreeMap<(String, String), String>,
}

#[cfg(test)]
impl Hive for MemHive {
    fn get(&self, key: &str, name: &str) -> Option<String> {
        self.values.get(&(key.to_ascii_lowercase(), name.to_ascii_lowercase())).cloned()
    }
    fn set(&mut self, key: &str, name: &str, value: &str) -> io::Result<()> {
        self.values.insert((key.to_ascii_lowercase(), name.to_ascii_lowercase()), value.to_owned());
        Ok(())
    }
    fn delete_value(&mut self, key: &str, name: &str) -> io::Result<()> {
        self.values.remove(&(key.to_ascii_lowercase(), name.to_ascii_lowercase()));
        Ok(())
    }
    fn delete_key(&mut self, key: &str) -> io::Result<()> {
        let key = key.to_ascii_lowercase();
        let sub = format!("{key}\\");
        self.values.retain(|(k, _), _| *k != key && !k.starts_with(&sub));
        Ok(())
    }
}

/// HKEY_CURRENT_USER.
#[cfg(windows)]
pub struct WinHive;

#[cfg(windows)]
impl Hive for WinHive {
    fn get(&self, key: &str, name: &str) -> Option<String> {
        winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey(key).ok()?.get_value::<String, _>(name).ok()
    }
    fn set(&mut self, key: &str, name: &str, value: &str) -> io::Result<()> {
        let (k, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).create_subkey(key)?;
        k.set_value(name, &value)
    }
    fn delete_value(&mut self, key: &str, name: &str) -> io::Result<()> {
        let k = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey_with_flags(key, winreg::enums::KEY_SET_VALUE)?;
        match k.delete_value(name) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
    fn delete_key(&mut self, key: &str) -> io::Result<()> {
        match winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).delete_subkey_all(key) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = r"C:\Program Files\Lumen\lumen.exe";

    fn present(exe: &str) -> bool {
        exe == EXE
    }

    #[test]
    fn entree_laissee_par_une_installation_supprimee() {
        assert_eq!(command_exe(r#""C:\Program Files\Lumen\lumen.exe" --lumen-open "%V""#), Some(EXE));
        assert_eq!(command_exe("lumen.exe --lumen-open"), None);
        assert_eq!(command_exe(r#""" --lumen-open"#), None);
        let mut h = MemHive::default();
        let gone = r"D:\Ancien\Lumen\lumen.exe";
        register_menu(&mut h, gone, "Analyser avec Lumen").unwrap();
        set_autostart(&mut h, gone, true).unwrap();
        // L'exécutable n'existe plus : vu comme absent, jamais relancé ni réétiqueté.
        assert!(!menu_registered(&h, &present));
        assert!(!autostart_registered(&h, &present));
        relabel_menu(&mut h, "Analyze with Lumen", &present).unwrap();
        assert_eq!(h.get(MENU_KEYS[0], "").as_deref(), Some("Analyser avec Lumen"));
        // Réactivé : réécrit vers l'exécutable présent ; retiré : plus rien.
        register_menu(&mut h, EXE, "Analyser avec Lumen").unwrap();
        set_autostart(&mut h, EXE, true).unwrap();
        assert!(menu_registered(&h, &present) && autostart_registered(&h, &present));
        unregister_menu(&mut h).unwrap();
        set_autostart(&mut h, EXE, false).unwrap();
        assert!(h.values.is_empty());
    }

    #[test]
    fn menu_contextuel_sur_les_dossiers_les_lecteurs_et_le_fond() {
        let mut h = MemHive::default();
        assert!(!menu_registered(&h, &present));
        register_menu(&mut h, EXE, "Analyser avec Lumen").unwrap();
        assert!(menu_registered(&h, &present));
        for key in MENU_KEYS {
            assert_eq!(h.get(key, "").as_deref(), Some("Analyser avec Lumen"));
            assert_eq!(h.get(key, "Icon").as_deref(), Some(r#""C:\Program Files\Lumen\lumen.exe",0"#));
            assert_eq!(h.get(&format!(r"{key}\command"), "").as_deref(), Some(r#""C:\Program Files\Lumen\lumen.exe" --lumen-open "%V""#));
        }
        relabel_menu(&mut h, "Analyze with Lumen", &present).unwrap();
        assert_eq!(h.get(MENU_KEYS[2], "").as_deref(), Some("Analyze with Lumen"));
        unregister_menu(&mut h).unwrap();
        assert!(h.values.is_empty());
        // Retiré : le libellé ne le recrée pas.
        relabel_menu(&mut h, "Analyser avec Lumen", &present).unwrap();
        assert!(h.values.is_empty());
    }

    #[test]
    fn une_cle_lumen_etrangere_n_est_pas_retiree() {
        let mut h = MemHive::default();
        h.set(r"Software\Classes\Drive\shell\Lumen\command", "", "autre.exe \"%1\"").unwrap();
        register_menu(&mut h, EXE, "Analyser avec Lumen").unwrap();
        h.set(r"Software\Classes\Drive\shell\Lumen\command", "", "autre.exe \"%1\"").unwrap();
        assert!(!menu_registered(&h, &present));
        unregister_menu(&mut h).unwrap();
        assert_eq!(h.values.len(), 3, "la clé étrangère reste entière : {:?}", h.values);
    }

    #[test]
    fn entree_run_de_l_utilisateur() {
        let mut h = MemHive::default();
        assert!(!autostart_registered(&h, &present));
        set_autostart(&mut h, EXE, true).unwrap();
        assert!(autostart_registered(&h, &present));
        assert_eq!(h.get(RUN_KEY, "Lumen").as_deref(), Some(r#""C:\Program Files\Lumen\lumen.exe" --lumen-background"#));
        set_autostart(&mut h, EXE, false).unwrap();
        assert!(!autostart_registered(&h, &present));
        set_autostart(&mut h, EXE, false).unwrap();
        assert!(h.values.is_empty());
    }
}
