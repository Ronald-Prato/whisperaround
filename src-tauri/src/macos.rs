//! macOS-only integration: the privacy permissions dictation needs and the
//! floating-window behaviour.
//!
//! - **Microphone**: macOS asks the first time the app opens an input stream;
//!   if the user said no, `cpal` keeps "recording" pure silence, so the status
//!   is checked explicitly to give a useful error.
//! - **Accessibility**: required to post the simulated Cmd+V. Without it the
//!   keystroke is silently dropped, so it is checked before pasting.

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::msg_send;
use objc2_foundation::{NSDictionary, NSNumber, NSString};
use serde::Serialize;
use tauri::WebviewWindow;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const AnyObject) -> bool;
    static kAXTrustedCheckOptionPrompt: &'static NSString;
}

#[link(name = "AVFoundation", kind = "framework")]
unsafe extern "C" {
    static AVMediaTypeAudio: &'static NSString;
}

const PRIVACY_PANE: &str = "x-apple.systempreferences:com.apple.preference.security";

/// State of the two permissions, sent to the welcome window.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Permissions {
    pub microphone: MicPermission,
    pub accessibility: bool,
}

/// `AVAuthorizationStatus`, lowercased for the frontend.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum MicPermission {
    Undetermined,
    Denied,
    Granted,
}

pub fn permissions() -> Permissions {
    Permissions {
        microphone: microphone(),
        accessibility: accessibility_trusted(),
    }
}

/// Whether the app may post keyboard events (Cmd+V).
pub fn accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Ask for Accessibility: macOS shows its own dialog pointing to System
/// Settings and lists the app there (still switched off) so the user only has
/// to flip the toggle.
pub fn request_accessibility() -> bool {
    let key: &NSString = unsafe { kAXTrustedCheckOptionPrompt };
    let yes = NSNumber::new_bool(true);
    let options: Retained<NSDictionary<NSString, NSNumber>> =
        NSDictionary::from_slices(&[key], &[&*yes]);
    let ptr = Retained::as_ptr(&options) as *const AnyObject;
    unsafe { AXIsProcessTrustedWithOptions(ptr) }
}

pub fn microphone() -> MicPermission {
    let Some(cls) = AnyClass::get(c"AVCaptureDevice") else {
        return MicPermission::Undetermined;
    };
    let media: &NSString = unsafe { AVMediaTypeAudio };
    let status: isize = unsafe { msg_send![cls, authorizationStatusForMediaType: media] };
    match status {
        0 => MicPermission::Undetermined,
        3 => MicPermission::Granted,
        // 1 = restricted (parental controls / MDM), 2 = denied.
        _ => MicPermission::Denied,
    }
}

/// Show the system microphone prompt (only does something while the status is
/// still undetermined; afterwards the user has to go to System Settings).
pub fn request_microphone() {
    let Some(cls) = AnyClass::get(c"AVCaptureDevice") else {
        return;
    };
    let media: &NSString = unsafe { AVMediaTypeAudio };
    let handler = block2::RcBlock::new(|_granted: Bool| {});
    let _: () = unsafe {
        msg_send![cls, requestAccessForMediaType: media, completionHandler: &*handler]
    };
}

/// Open System Settings › Privacy & Security at the given section
/// (`Privacy_Microphone`, `Privacy_Accessibility`).
pub fn open_privacy_settings(section: &str) {
    let _ = std::process::Command::new("open")
        .arg(format!("{PRIVACY_PANE}?{section}"))
        .spawn();
}

/// Let the floating pill follow the user across Spaces and over full-screen
/// apps, like a system overlay, without showing up in Cmd+Tab.
pub fn make_overlay(window: &WebviewWindow) {
    // NSWindowCollectionBehavior: CanJoinAllSpaces | Stationary |
    // IgnoresCycle | FullScreenAuxiliary.
    const BEHAVIOR: usize = (1 << 0) | (1 << 4) | (1 << 6) | (1 << 8);
    let Ok(ns_window) = window.ns_window() else {
        return;
    };
    let ns_window = ns_window as *mut AnyObject;
    if ns_window.is_null() {
        return;
    }
    // SAFETY: `ns_window` is the live NSWindow of this Tauri window and setup
    // runs on the main thread.
    unsafe {
        let _: () = msg_send![ns_window, setCollectionBehavior: BEHAVIOR];
    }
}
