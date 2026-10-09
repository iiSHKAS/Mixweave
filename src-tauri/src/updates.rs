//! Automatic AppImage updates; the webview cannot supply URLs or bytes.
use crate::state::AppState;
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

// Also held by restart/disable, so no replacement can overlap either action.
pub static INSTALL_LOCK: Mutex<()> = Mutex::new(());
const INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);
#[derive(Clone, Serialize, Default)]
pub struct UpdateStatus {
    phase: String,
    version: Option<String>,
    downloaded: usize,
    total: Option<u64>,
    error: Option<String>,
}
pub struct UpdateRuntime {
    status: Mutex<UpdateStatus>,
    busy: AtomicBool,
    cancel: tokio::sync::watch::Sender<u64>,
}
impl Default for UpdateRuntime {
    fn default() -> Self {
        Self {
            status: Mutex::new(UpdateStatus {
                phase: "idle".into(),
                ..Default::default()
            }),
            busy: AtomicBool::new(false),
            cancel: tokio::sync::watch::channel(0).0,
        }
    }
}
fn publish(app: &tauri::AppHandle, status: UpdateStatus) {
    if let Ok(mut current) = app.state::<UpdateRuntime>().status.lock() {
        *current = status.clone();
    }
    let _ = app.emit("mixweave://update", status);
}
fn phase(app: &tauri::AppHandle, phase: &str, version: Option<String>) {
    publish(
        app,
        UpdateStatus {
            phase: phase.into(),
            version,
            ..Default::default()
        },
    );
}
fn notify(app: &tauri::AppHandle, body: &str) {
    // Persistent in-app status remains available if desktop notifications are disabled.
    let _ = app
        .notification()
        .builder()
        .title("Mixweave")
        .body(body)
        .show();
}
fn appimage_path() -> Result<PathBuf, String> {
    if !matches!(
        tauri::utils::platform::bundle_type(),
        Some(tauri::utils::config::BundleType::AppImage)
    ) {
        return Err("Automatic updates are available only in the packaged AppImage edition".into());
    }
    let path = std::env::var_os("APPIMAGE")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .ok_or("Automatic updates are available only in the AppImage edition")?;
    crate::update_install::validate_path(&path).map_err(|e| e.to_string())?;
    Ok(path)
}
fn configured(app: &tauri::AppHandle) -> bool {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|v| v.get("pubkey"))
        .and_then(|v| v.as_str())
        .is_some_and(|v| !v.trim().is_empty())
}
#[derive(Serialize)]
pub struct UpdateInfo {
    enabled: bool,
    supported: bool,
    configured: bool,
    status: UpdateStatus,
}
#[tauri::command]
pub fn get_update_status(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    Ok(UpdateInfo {
        enabled: app
            .state::<AppState>()
            .lock_mixer()?
            .prefs
            .auto_update_enabled,
        supported: appimage_path().is_ok(),
        configured: configured(&app),
        status: app
            .state::<UpdateRuntime>()
            .status
            .lock()
            .map_err(|e| e.to_string())?
            .clone(),
    })
}
#[tauri::command]
pub fn set_auto_update(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let _install = INSTALL_LOCK.lock().map_err(|e| e.to_string())?;
    let state = app.state::<AppState>();
    let mut mixer = state.lock_mixer()?;
    let mut prefs = mixer.prefs.clone();
    prefs.auto_update_enabled = enabled;
    prefs.save().map_err(|e| e.to_string())?;
    mixer.prefs = prefs;
    app.state::<UpdateRuntime>()
        .cancel
        .send_modify(|generation| *generation = generation.wrapping_add(1));
    Ok(())
}
#[tauri::command]
pub async fn check_and_install_update(app: tauri::AppHandle) -> Result<(), String> {
    run(app, false).await
}
struct BusyGuard<'a>(&'a AtomicBool);
impl Drop for BusyGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
async fn run(app: tauri::AppHandle, automatic: bool) -> Result<(), String> {
    let runtime = app.state::<UpdateRuntime>();
    if runtime.busy.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    let _busy = BusyGuard(&runtime.busy);
    let mut cancel = runtime.cancel.subscribe();
    let generation = *cancel.borrow();
    if runtime.status.lock().map_err(|e| e.to_string())?.phase == "installed" {
        return Ok(());
    }
    if automatic
        && !app
            .state::<AppState>()
            .lock_mixer()?
            .prefs
            .auto_update_enabled
    {
        return Ok(());
    }
    let result = async {
        let path = appimage_path()?;
        if !configured(&app) { return Err("This build has no update signing public key".into()); }
        phase(&app, "checking", None);
        let updater = app.updater_builder().timeout(Duration::from_secs(120)).build().map_err(|e| e.to_string())?;
        let update = tokio::select! {
            result = updater.check() => result.map_err(|e| e.to_string())?,
            _ = cancel.changed() => { phase(&app, "idle", None); return Ok(()); }
        };
        let Some(mut update) = update else { phase(&app, "current", None); return Ok(()); };
        // Releases are immutable versioned URLs, never arbitrary mirror commands.
        if update.download_url.scheme() != "https" || update.download_url.host_str() != Some("github.com")
            || !update.download_url.path().starts_with("/iishkas/Mixweave/releases/download/") {
            return Err("Update download URL is outside the configured release repository".into());
        }
        update.timeout = Some(Duration::from_secs(30 * 60));
        let version = update.version.clone();
        phase(&app, "downloading", Some(version.clone()));
        notify(&app, &format!("Mixweave {version} is downloading and will install automatically. Audio will keep running."));
        let mut downloaded = 0usize;
        let mut last_event = std::time::Instant::now();
        let download = update.download(|chunk, total| {
            downloaded = downloaded.saturating_add(chunk);
            if last_event.elapsed() >= Duration::from_millis(250) {
                publish(&app, UpdateStatus { phase: "downloading".into(), version: Some(version.clone()), downloaded, total, error: None });
                last_event = std::time::Instant::now();
            }
        }, || {});
        // download() returns only after checking the embedded public key signature.
        let bytes = tokio::select! {
            result = download => result.map_err(|e| e.to_string())?,
            _ = cancel.changed() => { phase(&app, "idle", None); return Ok(()); }
        };
        let install_app = app.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let _install = INSTALL_LOCK.lock().map_err(|e| e.to_string())?;
            if *install_app.state::<UpdateRuntime>().cancel.borrow() != generation {
                phase(&install_app, "idle", None);
                return Ok(());
            }
            if crate::persistence::config_writes_quiesced() { return Err("Application is restarting".into()); }
            phase(&install_app, "installing", Some(version.clone()));
            crate::update_install::install(&path, &bytes).map_err(|e| e.to_string())?;
            phase(&install_app, "installed", Some(version.clone()));
            notify(&install_app, &format!("Mixweave {version} is installed. It will run next time you open the app."));
            Ok::<(), String>(())
        }).await.map_err(|e| e.to_string())?
    }.await;
    if let Err(error) = &result {
        publish(
            &app,
            UpdateStatus {
                phase: "error".into(),
                error: Some(error.clone()),
                ..Default::default()
            },
        );
    }
    result
}
pub fn start(app: tauri::AppHandle) {
    if appimage_path().is_err() || !configured(&app) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut changes = app.state::<UpdateRuntime>().cancel.subscribe();
        tokio::time::sleep(Duration::from_secs(30)).await;
        loop {
            let _ = run(app.clone(), true).await;
            tokio::select! {
                _ = tokio::time::sleep(INTERVAL) => {},
                _ = changes.changed() => {},
            }
        }
    });
}
