//! Compactage des disques virtuels WSL et Docker (réglage Expérimental) : liste des
//! distributions (registre `Lxss`, en lecture seule) et des disques de Docker Desktop, taille de
//! chaque `.vhdx`. Le plan arrête d'abord WSL (`wsl --shutdown`, prérequis obligatoire : tous les
//! processus Linux et Docker s'arrêtent), puis compacte chaque disque par une commande élevée
//! lancée directement (jamais par l'assistant MFT). Jamais de suppression d'un vhdx ; gain mesuré
//! par l'espace libre avant et après, journalisé.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cleanup::Selection;
use crate::error::LumenError;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VhdxDto {
    /// Identifiant de la liste (le front ne renvoie jamais de chemin).
    pub id: u32,
    pub name: String,
    /// `wsl` (distribution) ou `docker` (Docker Desktop).
    pub kind: &'static str,
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WslDto {
    pub disks: Vec<VhdxDto>,
    /// Module Hyper-V présent : `Optimize-VHD` ; sinon `diskpart` en lecture seule.
    pub hyperv: bool,
}

/// Distribution déclarée dans `HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss\{GUID}`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Distro {
    pub name: String,
    pub base_path: String,
    /// `VhdFileName` (par défaut `ext4.vhdx`).
    pub vhd: Option<String>,
}

/// Disques à proposer : ceux des distributions, puis ceux de Docker Desktop, s'ils existent ;
/// `size` lit la taille d'un fichier (absent ou illisible = écarté).
pub fn disks(distros: &[Distro], local_app_data: Option<&Path>, size: &dyn Fn(&Path) -> Option<u64>) -> Vec<(VhdxDto, PathBuf)> {
    let mut out: Vec<(VhdxDto, PathBuf)> = Vec::new();
    let mut push = |name: &str, kind: &'static str, path: PathBuf| {
        let key = path.to_string_lossy().to_lowercase();
        if out.iter().any(|(_, p)| p.to_string_lossy().to_lowercase() == key) || !key.ends_with(".vhdx") {
            return;
        }
        if let Some(s) = size(&path) {
            let id = out.len() as u32;
            out.push((VhdxDto { id, name: name.to_owned(), kind, path: path.display().to_string(), size: s }, path));
        }
    };
    for d in distros {
        let base = d.base_path.trim().trim_start_matches("\\\\?\\");
        if base.is_empty() || d.name.trim().is_empty() {
            continue;
        }
        let file = d.vhd.as_deref().filter(|v| !v.trim().is_empty()).unwrap_or("ext4.vhdx");
        push(d.name.trim(), if d.name.to_ascii_lowercase().starts_with("docker-desktop") { "docker" } else { "wsl" }, Path::new(base).join(file));
    }
    if let Some(local) = local_app_data {
        let docker = local.join("Docker").join("wsl");
        for (name, rel) in [("Docker Desktop (données)", ["disk", "docker_data.vhdx"]), ("Docker Desktop (données)", ["data", "ext4.vhdx"]), ("Docker Desktop", ["main", "ext4.vhdx"])] {
            push(name, "docker", docker.join(rel[0]).join(rel[1]));
        }
    }
    out
}

/// Plan : `wsl --shutdown` obligatoire, puis un compactage élevé par disque coché.
pub fn select(paths: &[PathBuf]) -> Result<Selection, LumenError> {
    use safety::commands::{OfficialCommand, Vhdx};
    use safety::plan::PlannedCommand;
    if paths.is_empty() {
        return Err(LumenError::Plan("aucun disque virtuel choisi".into()));
    }
    let mut commands = vec![PlannedCommand::prerequisite(OfficialCommand::WslShutdown, true)];
    for p in paths {
        let path = safety::path::WinPath::parse(&p.display().to_string()).map_err(|e| LumenError::Plan(e.to_string()))?;
        let vhdx = Vhdx::new(&path).map_err(|e| LumenError::Plan(e.to_string()))?;
        commands.push(PlannedCommand::action(OfficialCommand::VhdCompact(vhdx), Some(path), 0));
    }
    Ok(Selection {
        targets: Vec::new(),
        allow: safety::allow::Allowlist::new(),
        commands,
        rule_names: std::collections::HashMap::new(),
        elevated: true,
        guard: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distributions_et_docker() {
        let distros = vec![
            Distro { name: "Ubuntu".into(), base_path: "\\\\?\\C:\\Users\\Arthur\\AppData\\Local\\Packages\\Ubuntu\\LocalState".into(), vhd: None },
            Distro { name: "Debian".into(), base_path: "D:\\WSL\\Debian".into(), vhd: Some("debian.vhdx".into()) },
            // Sans chemin, sans nom, disque absent : écartés sans erreur.
            Distro { name: "Cassée".into(), base_path: String::new(), vhd: None },
            Distro { name: " ".into(), base_path: "D:\\WSL\\X".into(), vhd: None },
            Distro { name: "Absente".into(), base_path: "E:\\Absent".into(), vhd: None },
        ];
        let local = PathBuf::from("C:\\Users\\Arthur\\AppData\\Local");
        let size = |p: &Path| {
            let s = p.to_string_lossy().replace('/', "\\");
            (!s.starts_with("E:") && !s.contains("\\data\\") && !s.contains("\\main\\")).then_some(1 << 30)
        };
        let list = disks(&distros, Some(&local), &size);
        let names: Vec<(&str, &str)> = list.iter().map(|(d, _)| (d.name.as_str(), d.kind)).collect();
        assert_eq!(names, [("Ubuntu", "wsl"), ("Debian", "wsl"), ("Docker Desktop (données)", "docker")]);
        assert!(list[0].1.ends_with("ext4.vhdx") && list[0].0.path.starts_with("C:\\Users"));
        assert!(list[1].1.ends_with("debian.vhdx"));
        assert_eq!(list.iter().map(|(d, _)| d.id).collect::<Vec<_>>(), [0, 1, 2]);
    }

    #[test]
    fn plan_arret_puis_compactage_jamais_de_suppression() {
        let sel = select(&[PathBuf::from("C:\\WSL\\Ubuntu\\ext4.vhdx")]).unwrap();
        assert!(sel.elevated && sel.targets.is_empty(), "aucun fichier n'est supprimé");
        assert_eq!(sel.commands.len(), 2);
        assert!(sel.commands[0].required && sel.commands[0].command == safety::commands::OfficialCommand::WslShutdown);
        assert!(matches!(sel.commands[1].command, safety::commands::OfficialCommand::VhdCompact(_)));
        assert!(!sel.commands[1].command.wants_restore_point());
        assert!(select(&[]).is_err());
        assert!(select(&[PathBuf::from("C:\\WSL\\Ubuntu\\ext4.img")]).is_err());
    }
}
