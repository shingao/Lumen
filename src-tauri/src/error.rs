//! Erreurs typées de Lumen, avec un message utilisateur en français.

use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum LumenError {
    #[error("Lumen n'a pas pu démarrer : {0}")]
    Startup(String),
    #[error("lecteur inconnu : {0}")]
    UnknownVolume(String),
    #[error("dossier utilisateur introuvable")]
    NoProfile,
    #[error("analyse inconnue ou expirée")]
    UnknownScan,
    #[error("élément inconnu dans cette analyse")]
    UnknownNode,
    #[error("une analyse est déjà en cours")]
    ScanRunning,
    #[error("{0}")]
    Scan(#[from] scan::ScanError),
}

/// Les commandes IPC renvoient le message lisible, jamais la structure interne.
impl Serialize for LumenError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_en_francais() {
        let err = LumenError::Startup("WebView2 absent".into());
        assert_eq!(err.to_string(), "Lumen n'a pas pu démarrer : WebView2 absent");
        assert_eq!(serde_json::to_string(&err).unwrap(), "\"Lumen n'a pas pu démarrer : WebView2 absent\"");
        assert_eq!(LumenError::from(scan::ScanError::Cancelled).to_string(), "analyse annulée");
    }
}
