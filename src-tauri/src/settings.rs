//! Persisted user settings (stored as JSON in the app config dir).
//!
//! The OpenAI API key lives only here, in the Rust process. It is never sent
//! back to the webview in clear text — the frontend receives a [`SettingsView`]
//! that only exposes whether a key is configured. Where the OS has a credential
//! store ([`secret`]) the key is kept there and left out of the JSON file.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::secret;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub openai_api_key: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_shortcut")]
    pub shortcut: String,
    #[serde(default = "default_auto_paste")]
    pub auto_paste: bool,
    /// Whether the first-launch welcome window has been dismissed.
    #[serde(default)]
    pub onboarded: bool,
}

fn default_model() -> String {
    "gpt-4o-mini-transcribe".into()
}
fn default_shortcut() -> String {
    "CmdOrCtrl+Shift+X".into()
}
fn default_auto_paste() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            openai_api_key: String::new(),
            model: default_model(),
            shortcut: default_shortcut(),
            auto_paste: default_auto_paste(),
            onboarded: false,
        }
    }
}

/// Settings as exposed to the frontend (no key in clear text).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub api_key_configured: bool,
    pub model: String,
    pub shortcut: String,
    pub auto_paste: bool,
}

/// Settings update coming from the frontend.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsUpdate {
    /// Empty string means "keep the stored key".
    pub openai_api_key: String,
    pub model: String,
    pub shortcut: String,
    pub auto_paste: bool,
}

impl Settings {
    /// Apply an update from the settings window. An empty key keeps the
    /// stored one.
    pub fn apply_update(&mut self, update: SettingsUpdate) {
        let key = update.openai_api_key.trim();
        if !key.is_empty() {
            self.openai_api_key = key.to_string();
        }
        self.model = update.model;
        self.shortcut = update.shortcut;
        self.auto_paste = update.auto_paste;
    }

    /// What gets written to `settings.json`: everything except the key when
    /// the OS credential store holds it.
    fn to_disk(&self) -> Settings {
        let mut disk = self.clone();
        if secret::AVAILABLE {
            disk.openai_api_key.clear();
        }
        disk
    }

    pub fn view(&self) -> SettingsView {
        SettingsView {
            api_key_configured: !self.openai_api_key.trim().is_empty(),
            model: self.model.clone(),
            shortcut: self.shortcut.clone(),
            auto_paste: self.auto_paste,
        }
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| anyhow!("resolving config dir: {e}"))?;
    Ok(dir.join("settings.json"))
}

/// Load settings, falling back to defaults if the file is missing or invalid.
///
/// A key found in the file (written by versions before the credential store)
/// is moved into the store and removed from the file.
pub fn load(app: &AppHandle) -> Settings {
    let mut settings: Settings = match settings_path(app).ok().and_then(|p| std::fs::read(p).ok()) {
        Some(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        None => Settings::default(),
    };
    if !secret::AVAILABLE {
        return settings;
    }
    if settings.openai_api_key.is_empty() {
        match secret::load() {
            Ok(key) => settings.openai_api_key = key.unwrap_or_default(),
            Err(e) => eprintln!("[whisperaround] {e}"),
        }
    } else if let Err(e) = save(app, &settings) {
        eprintln!("[whisperaround] migrando la API key al llavero: {e}");
    }
    settings
}

/// Persist settings to disk (and the key to the credential store), creating
/// the config dir if needed.
pub fn save(app: &AppHandle, settings: &Settings) -> Result<()> {
    if secret::AVAILABLE {
        secret::store(&settings.openai_api_key)?;
    }
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(&settings.to_disk())?;
    std::fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "sk-test-1234567890";

    fn with_key() -> Settings {
        Settings {
            openai_api_key: KEY.into(),
            ..Settings::default()
        }
    }

    fn update(key: &str) -> SettingsUpdate {
        SettingsUpdate {
            openai_api_key: key.into(),
            model: "local:small".into(),
            shortcut: "Alt+Space".into(),
            auto_paste: false,
        }
    }

    #[test]
    fn view_never_contains_the_key() {
        let json = serde_json::to_string(&with_key().view()).unwrap();
        assert!(!json.contains(KEY), "key leaked to the webview payload: {json}");
        assert!(json.contains(r#""apiKeyConfigured":true"#));
    }

    #[test]
    fn view_reports_missing_key() {
        assert!(!Settings::default().view().api_key_configured);
        let blank = Settings {
            openai_api_key: "   ".into(),
            ..Settings::default()
        };
        assert!(!blank.view().api_key_configured);
    }

    #[test]
    fn empty_update_keeps_stored_key() {
        let mut s = with_key();
        s.apply_update(update("  "));
        assert_eq!(s.openai_api_key, KEY);
        assert_eq!(s.model, "local:small");
        assert_eq!(s.shortcut, "Alt+Space");
        assert!(!s.auto_paste);
    }

    #[test]
    fn new_key_is_trimmed() {
        let mut s = with_key();
        s.apply_update(update("  sk-new \n"));
        assert_eq!(s.openai_api_key, "sk-new");
    }

    #[test]
    fn old_settings_file_gets_defaults() {
        // A 0.1.x file without the newer fields.
        let s: Settings = serde_json::from_str(r#"{"openai_api_key":"sk-x"}"#).unwrap();
        assert_eq!(s.model, default_model());
        assert_eq!(s.shortcut, default_shortcut());
        assert!(s.auto_paste);
        assert!(!s.onboarded);
    }

    #[test]
    fn key_stays_out_of_the_file_when_the_os_store_exists() {
        let disk = serde_json::to_string(&with_key().to_disk()).unwrap();
        assert_eq!(disk.contains(KEY), !secret::AVAILABLE);
    }
}
