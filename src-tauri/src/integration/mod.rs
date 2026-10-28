//! Intégration Windows (jalon 11) : instance unique, « Analyser avec Lumen » dans l'Explorateur,
//! icône de la zone de notification, lancement au démarrage (entrée Run de HKCU), notifications
//! d'espace libre, journal de plantage local.
//!
//! Les réglages de l'interface (icône, notifications, seuil) restent côté front, qui les
//! transmet par `integration_apply` ; le menu contextuel et le démarrage sont lus et écrits
//! directement dans le registre de l'utilisateur, qui en est la seule source (l'installeur y
//! écrit les mêmes clés).

pub mod crashlog;
pub mod launch;
pub mod registry;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, Window, WindowEvent};

use crate::error::LumenError;
use launch::{KnownVolume, OpenRefusal, OpenRequest};

const TRAY_ID: &str = "lumen";
/// Sans réponse de l'interface au démarrage discret, la fenêtre est montrée quand même : Lumen
/// n'est jamais invisible sans icône.
const BACKGROUND_FALLBACK: std::time::Duration = std::time::Duration::from_secs(20);

/// Dossier demandé par le menu contextuel, ou refus expliqué.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum OpenDto {
    Open { letter: String, path: String },
    Refused { reason: &'static str },
}

#[derive(Default)]
struct Inner {
    /// Icône de la zone de notification active : fermer la fenêtre la cache.
    tray: bool,
    /// Démarré avec Windows, en attente des réglages de l'interface.
    background_pending: bool,
    pending_open: Option<OpenDto>,
}

#[derive(Default)]
pub struct IntegrationState(Mutex<Inner>);

impl IntegrationState {
    fn with<T>(&self, f: impl FnOnce(&mut Inner) -> T) -> Result<T, LumenError> {
        self.0.lock().map(|mut i| f(&mut i)).map_err(|_| LumenError::Integration("état indisponible".into()))
    }
}

/// `%LOCALAPPDATA%\Lumen\logs`.
pub fn logs_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("Lumen").join("logs"))
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn exe() -> Result<String, LumenError> {
    let path = std::env::current_exe().map_err(|e| LumenError::Integration(e.to_string()))?;
    let text = path.to_str().ok_or_else(|| LumenError::Integration("chemin de l'exécutable illisible".into()))?;
    // `\\?\` éventuel retiré : la commande du registre garde la forme ordinaire.
    Ok(text.strip_prefix(r"\\?\").unwrap_or(text).to_owned())
}

fn validate_open(raw: &str) -> OpenDto {
    let volumes = crate::scanning::volumes_list();
    let known: Vec<KnownVolume<'_>> = volumes.iter().map(|v| KnownVolume { letter: &v.letter, kind: v.kind }).collect();
    match launch::validate(raw, &known, platform::is_dir) {
        Ok(OpenRequest { letter, path }) => OpenDto::Open { letter, path },
        Err(refusal) => {
            if !matches!(refusal, OpenRefusal::NotDirectory) {
                eprintln!("menu contextuel : {refusal}");
            }
            OpenDto::Refused { reason: refusal.code() }
        }
    }
}

/// Arguments du premier lancement (avant que l'interface ne soit prête).
pub fn on_launch(app: &AppHandle, args: &[String]) {
    let launch = launch::parse(args);
    let state = app.state::<IntegrationState>();
    let _ = state.with(|i| {
        i.background_pending = launch.background;
        i.pending_open = launch.open.as_deref().map(validate_open);
    });
    if launch.background {
        let handle = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(BACKGROUND_FALLBACK);
            let pending = handle.state::<IntegrationState>().with(|i| std::mem::take(&mut i.background_pending)).unwrap_or(false);
            if pending {
                show_main(&handle);
            }
        });
    } else {
        show_main(app);
    }
}

/// Second lancement (menu contextuel, raccourci) : transmis à l'instance ouverte, qui se montre.
pub fn on_second_instance(app: &AppHandle, argv: &[String]) {
    let launch = launch::parse(argv.get(1..).unwrap_or(&[]));
    if launch.background {
        return;
    }
    if let Some(raw) = launch.open {
        let dto = validate_open(&raw);
        let _ = app.state::<IntegrationState>().with(|i| i.pending_open = Some(dto));
        let _ = app.emit("integration:open", ());
    }
    show_main(app);
}

/// Icône active : fermer la fenêtre la cache (Lumen continue, snapshot quotidien compris).
pub fn on_window_event(window: &Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        let tray = window.app_handle().state::<IntegrationState>().with(|i| i.tray).unwrap_or(false);
        if tray && window.label() == "main" {
            api.prevent_close();
            let _ = window.hide();
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationDto {
    /// « Analyser avec Lumen » présent dans le registre de l'utilisateur.
    pub explorer_menu: bool,
    /// Entrée Run présente.
    pub autostart: bool,
    /// Disque principal (zone de notification, « Analyser »).
    pub system_drive: String,
    /// Dossier du journal de plantage.
    pub logs: Option<String>,
    /// Disponible sur cette plateforme (Windows).
    pub available: bool,
}

#[tauri::command]
pub fn integration_state() -> IntegrationDto {
    let hive = platform::hive();
    IntegrationDto {
        explorer_menu: hive.as_ref().is_some_and(|h| registry::menu_registered(h.as_ref())),
        autostart: hive.as_ref().is_some_and(|h| registry::autostart_registered(h.as_ref())),
        system_drive: system_drive(),
        logs: logs_dir().and_then(|p| p.to_str().map(str::to_owned)),
        available: hive.is_some(),
    }
}

fn system_drive() -> String {
    std::env::var("SystemDrive").ok().filter(|d| d.len() == 2 && d.ends_with(':')).unwrap_or_else(|| "C:".into())
}

fn io_err(e: std::io::Error) -> LumenError {
    LumenError::Integration(e.to_string())
}

#[tauri::command]
pub fn integration_set_menu(on: bool, label: String) -> Result<IntegrationDto, LumenError> {
    let mut hive = platform::hive().ok_or_else(|| LumenError::Integration("disponible sous Windows uniquement".into()))?;
    if on {
        registry::register_menu(hive.as_mut(), &exe()?, label.trim()).map_err(io_err)?;
    } else {
        registry::unregister_menu(hive.as_mut()).map_err(io_err)?;
    }
    Ok(integration_state())
}

#[tauri::command]
pub fn integration_set_autostart(on: bool) -> Result<IntegrationDto, LumenError> {
    let mut hive = platform::hive().ok_or_else(|| LumenError::Integration("disponible sous Windows uniquement".into()))?;
    registry::set_autostart(hive.as_mut(), &exe()?, on).map_err(io_err)?;
    Ok(integration_state())
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayLabels {
    pub open: String,
    pub scan: String,
    pub quit: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyRequest {
    pub tray: bool,
    pub tooltip: String,
    pub labels: TrayLabels,
    /// Libellé du menu contextuel dans la langue de l'interface.
    pub menu_label: String,
}

/// Réglages de l'interface : icône (créée, mise à jour ou retirée), libellés, démarrage discret.
#[tauri::command]
pub fn integration_apply(app: AppHandle, state: State<'_, IntegrationState>, request: ApplyRequest) -> Result<(), LumenError> {
    let tooltip: String = request.tooltip.chars().take(120).collect();
    if request.tray {
        match app.tray_by_id(TRAY_ID) {
            Some(tray) => {
                let _ = tray.set_tooltip(Some(&tooltip));
                let _ = tray.set_menu(Some(tray_menu(&app, &request.labels)?));
            }
            None => build_tray(&app, &tooltip, &request.labels)?,
        }
    } else {
        let _ = app.remove_tray_by_id(TRAY_ID);
    }
    let was_pending = state.with(|i| {
        i.tray = request.tray;
        std::mem::take(&mut i.background_pending)
    })?;
    // Démarré avec Windows : caché dans la zone de notification, ou fenêtre réduite sans icône.
    if was_pending && !request.tray {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.minimize();
        }
    }
    if let Some(mut hive) = platform::hive() {
        let _ = registry::relabel_menu(hive.as_mut(), request.menu_label.trim());
    }
    Ok(())
}

fn tray_menu(app: &AppHandle, labels: &TrayLabels) -> Result<Menu<tauri::Wry>, LumenError> {
    let err = |e: tauri::Error| LumenError::Integration(e.to_string());
    let open = MenuItem::with_id(app, "open", &labels.open, true, None::<&str>).map_err(err)?;
    let scan = MenuItem::with_id(app, "scan", &labels.scan, true, None::<&str>).map_err(err)?;
    let sep = PredefinedMenuItem::separator(app).map_err(err)?;
    let quit = MenuItem::with_id(app, "quit", &labels.quit, true, None::<&str>).map_err(err)?;
    Menu::with_items(app, &[&open, &scan, &sep, &quit]).map_err(err)
}

fn build_tray(app: &AppHandle, tooltip: &str, labels: &TrayLabels) -> Result<(), LumenError> {
    let menu = tray_menu(app, labels)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(tooltip)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "scan" => {
                show_main(app);
                let _ = app.emit("integration:scan", system_drive());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app).map(|_| ()).map_err(|e| LumenError::Integration(e.to_string()))
}

/// Demande du menu contextuel en attente (premier lancement, ou après `integration:open`).
#[tauri::command]
pub fn open_request_take(state: State<'_, IntegrationState>) -> Result<Option<OpenDto>, LumenError> {
    state.with(|i| i.pending_open.take())
}

/// Notification d'espace libre (textes de l'interface) ; un clic ouvre l'Analyse du disque.
#[tauri::command]
pub fn notify_low_space(app: AppHandle, letter: String, title: String, body: String) -> Result<(), LumenError> {
    // Seule une lettre de la liste des volumes est acceptée.
    let letter = crate::scanning::volumes_list()
        .into_iter()
        .find(|v| v.letter.eq_ignore_ascii_case(&letter))
        .map(|v| v.letter)
        .ok_or(LumenError::UnknownVolume(letter))?;
    let title: String = title.chars().take(120).collect();
    let body: String = body.chars().take(240).collect();
    platform::toast(&app, letter, &title, &body).map_err(LumenError::Integration)
}

/// Erreur de l'interface, écrite dans le journal local.
#[tauri::command]
pub fn crash_report(message: String) {
    if let Some(dir) = logs_dir() {
        let _ = crashlog::append_front(&dir, &message);
    }
}

/// Ouvre le dossier du journal de plantage dans l'Explorateur.
#[tauri::command]
pub fn logs_open() -> Result<(), LumenError> {
    let dir = logs_dir().ok_or_else(|| LumenError::Integration("dossier des journaux introuvable".into()))?;
    std::fs::create_dir_all(&dir).map_err(io_err)?;
    platform::open_folder(&dir)
}

mod platform {
    use super::*;

    #[cfg(windows)]
    pub fn hive() -> Option<Box<dyn registry::Hive>> {
        Some(Box::new(registry::WinHive))
    }

    #[cfg(not(windows))]
    pub fn hive() -> Option<Box<dyn registry::Hive>> {
        None
    }

    /// Métadonnées seulement (un dossier, ou une jonction vers un dossier).
    #[cfg(windows)]
    pub fn is_dir(path: &safety::path::WinPath) -> bool {
        std::fs::metadata(path.verbatim()).is_ok_and(|m| m.is_dir())
    }

    #[cfg(not(windows))]
    pub fn is_dir(_: &safety::path::WinPath) -> bool {
        false
    }

    #[cfg(windows)]
    pub fn toast(app: &AppHandle, letter: String, title: &str, body: &str) -> Result<(), String> {
        // Identifiant posé par l'installeur sur le raccourci du menu Démarrer (AppUserModelID).
        let app_id = app.config().identifier.clone();
        let handle = app.clone();
        tauri_winrt_notification::Toast::new(&app_id)
            .title(title)
            .text1(body)
            .on_activated(move |_| {
                show_main(&handle);
                let _ = handle.emit("integration:alert", letter.clone());
                Ok(())
            })
            .show()
            .map_err(|e| e.to_string())
    }

    #[cfg(not(windows))]
    pub fn toast(_: &AppHandle, _: String, _: &str, _: &str) -> Result<(), String> {
        Err("disponible sous Windows uniquement".into())
    }

    #[cfg(windows)]
    pub fn open_folder(dir: &std::path::Path) -> Result<(), LumenError> {
        let text = dir.to_str().ok_or_else(|| LumenError::Integration("chemin illisible".into()))?;
        let path = safety::path::WinPath::parse(text).map_err(|e| LumenError::Integration(e.to_string()))?;
        safety::win::open_item(&path, true, safety::open::OpenAction::Open).map(|_| ()).map_err(|e| LumenError::Integration(e.to_string()))
    }

    #[cfg(not(windows))]
    pub fn open_folder(_: &std::path::Path) -> Result<(), LumenError> {
        Err(LumenError::Integration("disponible sous Windows uniquement".into()))
    }
}
