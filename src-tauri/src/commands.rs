//! Tauri commands exposed to the frontend (`src/lib/ipc.ts`).

use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use tauri_plugin_updater::UpdaterExt;

use crate::local;
use crate::pipeline;
use crate::settings::{self, SettingsUpdate, SettingsView};
use crate::state::{set_status, AppState, AppStatus};
use crate::window;

/// Current pipeline status (used to seed the UI on mount).
#[tauri::command]
pub fn get_status(state: State<AppState>) -> AppStatus {
    *state.status.lock().unwrap()
}

/// Manually toggle recording — equivalent to pressing the global shortcut.
#[tauri::command]
pub fn toggle_recording(app: AppHandle) {
    pipeline::toggle(&app);
}

/// Current settings, with the API key reduced to a boolean flag.
#[tauri::command]
pub fn load_settings(state: State<AppState>) -> SettingsView {
    state.settings.lock().unwrap().view()
}

/// Persist settings and return the refreshed view.
///
/// An empty `openai_api_key` leaves the stored key untouched. If the shortcut
/// changed, it is re-registered live so no restart is needed.
#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    state: State<AppState>,
    update: SettingsUpdate,
) -> Result<SettingsView, String> {
    let mut settings = state.settings.lock().unwrap();
    let old_shortcut = settings.shortcut.clone();

    settings.apply_update(update);

    if settings.shortcut != old_shortcut {
        rebind_shortcut(&app, &old_shortcut, &settings.shortcut)?;
    }

    settings::save(&app, &settings).map_err(|e| e.to_string())?;
    warm_local_model(&app, &settings.model);
    Ok(settings.view())
}

/// Update only the transcription model (used by the pill's model dropdown),
/// leaving the API key and other settings untouched.
#[tauri::command]
pub fn set_model(app: AppHandle, state: State<AppState>, model: String) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.model = model.clone();
    settings::save(&app, &settings).map_err(|e| e.to_string())?;
    drop(settings);
    warm_local_model(&app, &model);
    Ok(())
}

/// If `model` is a local faster-whisper model, prepare it in the background.
/// The first-time venv install is surfaced as `Processing` so the pill shows
/// it is busy; a plain model load stays silent.
pub fn warm_local_model(app: &AppHandle, model: &str) {
    let Some(name) = local::model_name(model) else {
        return;
    };
    let (app, name) = (app.clone(), name.to_string());
    tauri::async_runtime::spawn_blocking(move || {
        let installing = local::needs_setup(&app);
        if installing {
            set_status(&app, AppStatus::Processing, None);
        }
        match local::warm(&app, &name) {
            // Only clear the busy state we set (don't clobber a new recording).
            Ok(()) if installing => {
                let current = *app.state::<AppState>().status.lock().unwrap();
                if current == AppStatus::Processing {
                    set_status(&app, AppStatus::Idle, None);
                }
            }
            Ok(()) => {}
            Err(e) => set_status(&app, AppStatus::Error, Some(e.to_string())),
        }
    });
}

/// Resize the floating window to `width`×`height` logical px. The frontend
/// decides sizes so it can sequence them around its morph animations. Kept
/// anchored to the top-center so the pill grows in place, wherever it was
/// dragged.
#[tauri::command]
pub fn fit_window(app: AppHandle, width: f64, height: f64) {
    if let Some(window) = app.get_webview_window("main") {
        window::resize_anchored(&window, (width, height));
    }
}

/// Version of the system Python 3 (e.g. `"Python 3.12.7"`), or `None` if it
/// is missing. Only local faster-whisper models need it. Runs off the main
/// thread since probing the launchers spawns processes.
#[tauri::command]
pub async fn check_python() -> Option<String> {
    tauri::async_runtime::spawn_blocking(local::detect_python)
        .await
        .ok()
        .flatten()
}

/// Microsoft Store page for Python 3.12 (well covered by faster-whisper wheels).
const PYTHON_STORE_URL: &str = "ms-windows-store://pdp/?productid=9NCVDN91XZQP";
const PYTHON_DOWNLOAD_URL: &str = "https://www.python.org/downloads/";

/// Open the Python install page: the Microsoft Store on Windows, python.org
/// elsewhere.
#[tauri::command]
pub fn open_python_install() -> Result<(), String> {
    let result = if cfg!(windows) {
        std::process::Command::new("explorer").arg(PYTHON_STORE_URL).spawn()
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(PYTHON_DOWNLOAD_URL).spawn()
    } else {
        std::process::Command::new("xdg-open").arg(PYTHON_DOWNLOAD_URL).spawn()
    };
    result.map(|_| ()).map_err(|e| format!("no se pudo abrir la tienda: {e}"))
}

/// Dismiss the welcome window for good (persisted in settings).
#[tauri::command]
pub fn finish_onboarding(app: AppHandle, state: State<AppState>) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.onboarded = true;
    settings::save(&app, &settings).map_err(|e| e.to_string())?;
    if let Some(window) = app.get_webview_window("welcome") {
        let _ = window.hide();
    }
    Ok(())
}

/// Reveal the welcome window (first launch, or from the tray).
pub fn show_welcome_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("welcome") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Version of a newer release published on GitHub, or `None` if up to date.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<String>, String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let update = updater.check().await.map_err(|e| e.to_string())?;
    Ok(update.map(|u| u.version))
}

/// Download the newer release (signature-checked against the public key in
/// `tauri.conf.json`), install it and restart.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<(), String> {
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Ok(());
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| format!("actualizando: {e}"))?;
    app.restart();
}

/// On launch, look for a newer release and, if there is one, open the
/// settings window where the update banner offers to install it.
pub fn check_update_on_launch(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match check_update(app.clone()).await {
            Ok(Some(version)) => {
                eprintln!("[whisperaround] update available: {version}");
                show_settings_window(&app);
            }
            Ok(None) => {}
            Err(e) => eprintln!("[whisperaround] update check failed: {e}"),
        }
    });
}

/// Show and focus the standalone settings window.
#[tauri::command]
pub fn open_settings(app: AppHandle) {
    show_settings_window(&app);
}

/// Shared helper: reveal the settings window (used by the command and the tray).
pub fn show_settings_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Swap the registered global shortcut, restoring the old one on failure.
fn rebind_shortcut(app: &AppHandle, old: &str, new: &str) -> Result<(), String> {
    let new_sc: Shortcut = new
        .parse()
        .map_err(|_| format!("atajo inválido: '{new}'"))?;

    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    match gs.register(new_sc) {
        Ok(()) => {
            eprintln!("[whisperaround] global shortcut re-registered: {new}");
            Ok(())
        }
        Err(e) => {
            // Best-effort restore of the previous binding.
            if let Ok(old_sc) = old.parse::<Shortcut>() {
                let _ = gs.register(old_sc);
            }
            Err(format!("no se pudo registrar el atajo '{new}': {e}"))
        }
    }
}
