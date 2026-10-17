//! The core toggle pipeline: Idle → Recording → Processing → Idle.
//!
//! Driven both by the global shortcut handler and the manual `toggle_recording`
//! command, so the logic lives here in one place.

use tauri::{AppHandle, Emitter, Manager};

use crate::settings::Settings;
use crate::state::{set_status, AppState, AppStatus};
use crate::{inject, local, recorder, whisper};

#[cfg(target_os = "macos")]
const SILENT_MIC: &str =
    "el micrófono no captó audio: revisa Ajustes del Sistema › Privacidad y seguridad › Micrófono";
#[cfg(not(target_os = "macos"))]
const SILENT_MIC: &str = "el micrófono no captó audio: revisa que no esté silenciado";

/// Advance the state machine based on the current status.
pub fn toggle(app: &AppHandle) {
    let current = {
        let state = app.state::<AppState>();
        let status = *state.status.lock().unwrap();
        status
    };

    match current {
        AppStatus::Idle | AppStatus::Error => start(app),
        AppStatus::Recording => stop_and_transcribe(app),
        // Busy transcribing — ignore further toggles until done.
        AppStatus::Processing => {}
    }
}

fn start(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    if let Err(msg) = check_mic_permission() {
        set_status(app, AppStatus::Error, Some(msg.into()));
        return;
    }

    let state = app.state::<AppState>();
    // Live input level for the pill's waveform (`level` event, 0.0..=1.0).
    let level_app = app.clone();
    let on_level = move |level: f32| {
        let _ = level_app.emit("level", level);
    };
    match recorder::Recorder::start(on_level) {
        Ok(rec) => {
            *state.recorder.lock().unwrap() = Some(rec);
            set_status(app, AppStatus::Recording, None);
        }
        Err(e) => set_status(app, AppStatus::Error, Some(format!("micrófono: {e}"))),
    }
}

fn stop_and_transcribe(app: &AppHandle) {
    let state = app.state::<AppState>();

    let Some(rec) = state.recorder.lock().unwrap().take() else {
        set_status(app, AppStatus::Idle, None);
        return;
    };
    set_status(app, AppStatus::Processing, None);

    let recording = match rec.stop() {
        Ok(r) => r,
        Err(e) => {
            set_status(app, AppStatus::Error, Some(format!("grabación: {e}")));
            return;
        }
    };

    // Exact digital silence means the OS handed us a muted stream (on macOS:
    // microphone permission denied), not a quiet room.
    if recording.samples.iter().all(|&s| s == 0.0) {
        set_status(app, AppStatus::Error, Some(SILENT_MIC.into()));
        return;
    }

    let wav = match recorder::encode_wav(&recording) {
        Ok(w) => w,
        Err(e) => {
            set_status(app, AppStatus::Error, Some(format!("codificando WAV: {e}")));
            return;
        }
    };

    let settings: Settings = state.settings.lock().unwrap().clone();
    let app = app.clone();

    // Off-load the network call / local inference so the UI stays responsive.
    tauri::async_runtime::spawn(async move {
        let result = match local::model_name(&settings.model) {
            Some(name) => {
                let (app2, name) = (app.clone(), name.to_string());
                tauri::async_runtime::spawn_blocking(move || local::transcribe(&app2, &name, &wav))
                    .await
                    .unwrap_or_else(|e| Err(anyhow::anyhow!("faster-whisper: {e}")))
            }
            None => whisper::transcribe(&settings.openai_api_key, &settings.model, wav).await,
        };
        match result {
            Ok(text) => {
                let text = text.trim().to_string();
                if text.is_empty() {
                    set_status(&app, AppStatus::Idle, None);
                    return;
                }
                if let Err(e) = inject::copy_to_clipboard(&app, &text) {
                    set_status(&app, AppStatus::Error, Some(e.to_string()));
                    return;
                }
                if settings.auto_paste {
                    // Let the clipboard write settle before the paste keystroke.
                    std::thread::sleep(std::time::Duration::from_millis(120));
                    if let Err(e) = inject::paste(&app) {
                        set_status(&app, AppStatus::Error, Some(format!("auto-paste: {e}")));
                        return;
                    }
                }
                set_status(&app, AppStatus::Idle, None);
            }
            Err(e) => set_status(&app, AppStatus::Error, Some(e.to_string())),
        }
    });
}

/// Fail fast when macOS will not give us the microphone: recording anyway
/// would only capture silence.
#[cfg(target_os = "macos")]
fn check_mic_permission() -> Result<(), &'static str> {
    use crate::macos::{self, MicPermission};
    match macos::microphone() {
        MicPermission::Granted => Ok(()),
        MicPermission::Undetermined => {
            macos::request_microphone();
            Err("permite el acceso al micrófono y vuelve a pulsar el atajo")
        }
        MicPermission::Denied => {
            macos::open_privacy_settings("Privacy_Microphone");
            Err("micrófono bloqueado: activa WhisperAround en Ajustes del Sistema › Privacidad y seguridad › Micrófono")
        }
    }
}
