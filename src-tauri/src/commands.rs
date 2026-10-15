//! Tauri commands exposed to the frontend (`src/lib/ipc.ts`).

use tauri::{AppHandle, Manager, State};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

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

    if !update.openai_api_key.trim().is_empty() {
        settings.openai_api_key = update.openai_api_key.trim().to_string();
    }
    settings.model = update.model;
    settings.shortcut = update.shortcut;
    settings.auto_paste = update.auto_paste;

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
