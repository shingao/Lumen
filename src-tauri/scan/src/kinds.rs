//! Type d'un fichier d'après son extension (vidéo, image, archive…), partagé par les gros
//! fichiers et la recherche : une seule table.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileType {
    Video,
    Image,
    Audio,
    Archive,
    Disk,
    Installer,
    Document,
    Other,
}

impl FileType {
    /// Type d'après l'extension d'un nom (casse ignorée).
    pub fn of(name: &str) -> FileType {
        let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        FileType::of_ext(&ext)
    }

    /// Type d'après une extension déjà en minuscules, sans point.
    pub fn of_ext(ext: &str) -> FileType {
        match ext {
            "mp4" | "mkv" | "mov" | "avi" | "wmv" | "m4v" | "webm" | "flv" | "mpg" | "mpeg" | "ts" | "m2ts" | "mts" => FileType::Video,
            "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tif" | "tiff" | "heic" | "raw" | "cr2" | "cr3" | "nef" | "arw" | "dng" | "psd" | "webp" => FileType::Image,
            "mp3" | "flac" | "wav" | "aac" | "m4a" | "ogg" | "wma" | "opus" | "aiff" => FileType::Audio,
            "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "cab" => FileType::Archive,
            "iso" | "img" | "vhd" | "vhdx" | "vmdk" | "vdi" | "qcow2" | "wim" | "esd" | "dmg" => FileType::Disk,
            "exe" | "msi" | "msix" | "msixbundle" | "appx" | "appxbundle" => FileType::Installer,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "txt" | "csv" | "epub" => FileType::Document,
            _ => FileType::Other,
        }
    }
}
