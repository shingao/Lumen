//! Docker : images sans étiquette (`dangling`), listées par la commande officielle en lecture
//! seule ; `docker image prune` (ces mêmes images) seulement si Docker tourne, après une
//! confirmation qui montre la liste.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImageDto {
    pub id: String,
    pub repository: String,
    pub tag: String,
    /// Taille en octets (Docker l'affiche en unités décimales : 1.2GB).
    pub size: u64,
    pub created: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DockerDto {
    /// Docker installé et son démon joignable (la commande a réussi).
    pub running: bool,
    /// `notInstalled`, `notRunning` ou rien.
    pub reason: Option<&'static str>,
    pub images: Vec<ImageDto>,
    pub total: u64,
}

/// Taille affichée par Docker (`1.2GB`, `523MB`, `12.5kB`, `0B`) en octets (unités décimales).
pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let split = s.find(|c: char| c.is_ascii_alphabetic())?;
    let (num, unit) = s.split_at(split);
    let value: f64 = num.trim().parse().ok()?;
    let mult = match unit.trim().to_ascii_uppercase().as_str() {
        "B" => 1.0,
        "KB" => 1e3,
        "MB" => 1e6,
        "GB" => 1e9,
        "TB" => 1e12,
        _ => return None,
    };
    (value >= 0.0).then(|| (value * mult).round() as u64)
}

/// Sortie de `docker image ls --format "{{.ID}}\t{{.Repository}}\t{{.Tag}}\t{{.Size}}\t{{.CreatedSince}}"` ;
/// toute ligne mal formée est ignorée.
pub fn parse_images(stdout: &str) -> Vec<ImageDto> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut f = line.split('\t');
            let id = f.next()?.trim();
            let repository = f.next()?.trim();
            let tag = f.next()?.trim();
            let size = parse_size(f.next()?)?;
            let created = f.next().unwrap_or_default().trim();
            (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == ':')).then(|| ImageDto {
                id: id.to_owned(),
                repository: repository.to_owned(),
                tag: tag.to_owned(),
                size,
                created: created.to_owned(),
            })
        })
        .collect()
}

pub fn dto(run: Option<&safety::exec::CommandRun>) -> DockerDto {
    use safety::exec::CommandStatus;
    match run {
        Some(r) if r.status == CommandStatus::Succeeded => {
            let images = parse_images(&r.stdout);
            DockerDto { running: true, reason: None, total: images.iter().map(|i| i.size).sum(), images }
        }
        Some(r) if r.status == CommandStatus::NotFound => DockerDto { running: false, reason: Some("notInstalled"), images: Vec::new(), total: 0 },
        _ => DockerDto { running: false, reason: Some("notRunning"), images: Vec::new(), total: 0 },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safety::commands::OfficialCommand;
    use safety::exec::{CommandRun, CommandStatus};

    #[test]
    fn tailles_et_lignes_de_docker() {
        assert_eq!(parse_size("1.2GB"), Some(1_200_000_000));
        assert_eq!(parse_size("523MB"), Some(523_000_000));
        assert_eq!(parse_size("12.5kB"), Some(12_500));
        assert_eq!(parse_size("0B"), Some(0));
        assert_eq!(parse_size("beaucoup"), None);
        let out = "sha256:4f2b\t<none>\t<none>\t1.2GB\t3 weeks ago\nabc123\tmonapp\t<none>\t523MB\t2 days ago\nligne mal formée\n\t\t\t\t\n";
        let images = parse_images(out);
        assert_eq!(images.len(), 2);
        assert_eq!((images[1].repository.as_str(), images[1].size, images[1].created.as_str()), ("monapp", 523_000_000, "2 days ago"));
    }

    #[test]
    fn docker_absent_ou_arrete() {
        let mut ok = CommandRun::new(&OfficialCommand::DockerDanglingImages, CommandStatus::Succeeded);
        ok.stdout = "abc\t<none>\t<none>\t1GB\tnow\n".into();
        let d = dto(Some(&ok));
        assert_eq!((d.running, d.total, d.images.len()), (true, 1_000_000_000, 1));
        assert_eq!(dto(Some(&CommandRun::new(&OfficialCommand::DockerDanglingImages, CommandStatus::Failed(1)))).reason, Some("notRunning"));
        assert_eq!(dto(Some(&CommandRun::new(&OfficialCommand::DockerDanglingImages, CommandStatus::NotFound))).reason, Some("notInstalled"));
        assert!(!dto(None).running);
    }
}
