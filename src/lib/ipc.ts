/**
 * Typed wrappers around the Tauri IPC bridge.
 * Centralizes every `invoke`/`listen` call so the rest of the UI stays decoupled
 * from the command names exposed by the Rust backend (`src-tauri/src/commands.rs`).
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getVersion } from "@tauri-apps/api/app";
import type { AppStatus, SettingsView, SettingsUpdate, StatusEvent } from "./types";

/**
 * Whether the Tauri runtime is present. When running the Vite dev server in a
 * plain browser (UI preview) it isn't, so the IPC helpers below fall back to
 * in-memory defaults instead of throwing.
 */
const IN_TAURI =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** Mock settings used only for the browser UI preview. */
const previewSettings: SettingsView = {
  apiKeyConfigured: false,
  model: "gpt-4o-mini-transcribe",
  shortcut: "CmdOrCtrl+Shift+X",
  autoPaste: true,
};

/** Browser preview only: a fake pipeline so the pill's states can be seen. */
let previewStatus: AppStatus = "idle";
const previewListeners = new Set<(e: StatusEvent) => void>();
function previewEmit(status: AppStatus) {
  previewStatus = status;
  previewListeners.forEach((fn) => fn({ status }));
  // Fake a speech-like mic level while "recording".
  clearInterval(previewLevelTimer);
  if (status === "recording") {
    previewLevelTimer = setInterval(() => {
      const t = Date.now() / 1000;
      const level = Math.max(0, Math.sin(t * 3) * 0.5 + Math.random() * 0.5);
      previewLevelListeners.forEach((fn) => fn(level));
    }, 50);
  }
}
let previewLevelTimer: ReturnType<typeof setInterval> | undefined;
const previewLevelListeners = new Set<(level: number) => void>();

/** Read the current pipeline status (used to seed the UI on mount). */
export function getStatus(): Promise<AppStatus> {
  if (!IN_TAURI) return Promise.resolve(previewStatus);
  return invoke<AppStatus>("get_status");
}

/** Manually toggle recording (same effect as pressing the global shortcut). */
export function toggleRecording(): Promise<void> {
  if (!IN_TAURI) {
    if (previewStatus === "recording") {
      previewEmit("processing");
      setTimeout(() => previewEmit("idle"), 1500);
    } else if (previewStatus !== "processing") {
      previewEmit("recording");
    }
    return Promise.resolve();
  }
  return invoke("toggle_recording");
}

/** Load the current settings (API key returned only as a boolean flag). */
export function loadSettings(): Promise<SettingsView> {
  if (!IN_TAURI) return Promise.resolve(previewSettings);
  return invoke<SettingsView>("load_settings");
}

/** Persist settings. Returns the refreshed view. */
export function saveSettings(update: SettingsUpdate): Promise<SettingsView> {
  if (!IN_TAURI) {
    return Promise.resolve({
      apiKeyConfigured: update.openaiApiKey.trim().length > 0 || previewSettings.apiKeyConfigured,
      model: update.model,
      shortcut: update.shortcut,
      autoPaste: update.autoPaste,
    });
  }
  return invoke<SettingsView>("save_settings", { update });
}

/** Subscribe to backend status changes. Returns an unlisten function. */
export function onStatus(handler: (e: StatusEvent) => void): Promise<UnlistenFn> {
  if (!IN_TAURI) {
    previewListeners.add(handler);
    return Promise.resolve(() => previewListeners.delete(handler));
  }
  return listen<StatusEvent>("status", (event) => handler(event.payload));
}

/** Subscribe to the live mic level (0..1, ~20 Hz) while recording. */
export function onLevel(handler: (level: number) => void): Promise<UnlistenFn> {
  if (!IN_TAURI) {
    previewLevelListeners.add(handler);
    return Promise.resolve(() => previewLevelListeners.delete(handler));
  }
  return listen<number>("level", (event) => handler(event.payload));
}

/** Version of the system Python 3 (e.g. "Python 3.12.7"), or null if missing. */
export function checkPython(): Promise<string | null> {
  if (!IN_TAURI) return Promise.resolve(null);
  return invoke<string | null>("check_python");
}

/** Open the Python install page (Microsoft Store on Windows). */
export function openPythonInstall(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("open_python_install");
}

/** Installed app version (e.g. "0.1.5"). */
export function appVersion(): Promise<string> {
  if (!IN_TAURI) return Promise.resolve("dev");
  return getVersion();
}

/** Version of a newer published release, or null if up to date. */
export function checkUpdate(): Promise<string | null> {
  if (!IN_TAURI) return Promise.resolve(null);
  return invoke<string | null>("check_update");
}

/** Download + install the newer release, then restart the app. */
export function installUpdate(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("install_update");
}

/** Dismiss the first-launch welcome window for good. */
export function finishOnboarding(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("finish_onboarding");
}

/** Update only the transcription model (pill model dropdown). */
export function setModel(model: string): Promise<void> {
  if (!IN_TAURI) {
    previewSettings.model = model;
    return Promise.resolve();
  }
  return invoke("set_model", { model });
}

/**
 * Resize the floating window (logical px), keeping its top-center fixed. The
 * pill sequences these around its morph animations.
 */
export function fitWindow(width: number, height: number): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("fit_window", { width, height });
}

/** Open the standalone settings window. */
export function openSettings(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return invoke("open_settings");
}

/** Start an OS-level drag of the current window (call while the mouse is down). */
export function startDragging(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return getCurrentWindow().startDragging();
}

/** Hide the current window to the tray. */
export function hideWindow(): Promise<void> {
  if (!IN_TAURI) return Promise.resolve();
  return getCurrentWindow().hide();
}

/** Label of the current Tauri window ("main" pill, "settings" or "welcome"). */
export function currentWindowLabel(): string {
  // Browser preview: `?window=welcome` previews another window.
  if (!IN_TAURI) return new URLSearchParams(location.search).get("window") ?? "main";
  return getCurrentWindow().label;
}
