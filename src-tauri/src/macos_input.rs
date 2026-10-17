//! macOS-only shortcut sources the global-shortcut plugin can't express: a
//! lone modifier key (e.g. Right ⌘) and extra mouse buttons.
//!
//! Both come from `NSEvent` monitors — a global one for events going to other
//! apps and a local one for our own windows. Monitors are passive: the events
//! still reach their app. Key / modifier monitoring needs the Accessibility
//! permission (without it the callbacks silently never fire); mouse buttons
//! don't.
//!
//! - Modifier bound to Toggle (or Change mode / Cancel): fires on release, and
//!   only if no other key was pressed meanwhile, so Right ⌘+C still copies.
//! - Modifier bound to Push to Talk: press starts, release stops.
//! - Mouse button: a tap toggles; holding records until the button is released.

use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use tauri::{AppHandle, Manager};

use crate::shortcuts::{self, Action, Binding, ModKey};
use crate::state::{AppState, AppStatus};

static APP: OnceLock<AppHandle> = OnceLock::new();

// NSEventType values.
const KEY_DOWN: usize = 10;
const FLAGS_CHANGED: usize = 12;
const OTHER_MOUSE_DOWN: usize = 25;
const OTHER_MOUSE_UP: usize = 26;

/// Which modifier is currently held alone (`ModKey as u8 + 1`, 0 = none) and
/// whether another key was pressed while it was.
static HELD: AtomicU8 = AtomicU8::new(0);
static TAINTED: AtomicBool = AtomicBool::new(false);
/// Mouse hold tracking: when the bound button went down, and whether that
/// press started the recording.
static MOUSE_DOWN: Mutex<Option<(Instant, bool)>> = Mutex::new(None);
/// Holding the mouse button longer than this means push-to-talk.
const HOLD: Duration = Duration::from_millis(350);

/// Install the monitors. Must run on the main thread (app setup).
pub fn install(app: &AppHandle) {
    if APP.set(app.clone()).is_err() {
        return;
    }
    let Some(cls) = AnyClass::get(c"NSEvent") else {
        return;
    };
    let mask: u64 = (1 << KEY_DOWN) | (1 << FLAGS_CHANGED) | (1 << OTHER_MOUSE_DOWN) | (1 << OTHER_MOUSE_UP);

    let global = RcBlock::new(|event: NonNull<AnyObject>| {
        // SAFETY: AppKit hands us a live NSEvent for the duration of the call.
        handle(unsafe { event.as_ref() });
    });
    let local = RcBlock::new(|event: NonNull<AnyObject>| -> *mut AnyObject {
        // SAFETY: as above; returning the event lets it through unchanged.
        handle(unsafe { event.as_ref() });
        event.as_ptr()
    });
    // SAFETY: documented NSEvent class methods; AppKit copies the blocks. The
    // returned monitor objects are kept alive for the life of the process.
    unsafe {
        let g: Option<Retained<AnyObject>> =
            msg_send![cls, addGlobalMonitorForEventsMatchingMask: mask, handler: &*global];
        let l: Option<Retained<AnyObject>> =
            msg_send![cls, addLocalMonitorForEventsMatchingMask: mask, handler: &*local];
        std::mem::forget(g);
        std::mem::forget(l);
    }
}

/// `(keyCode, device-dependent flag mask)` for each modifier key.
fn key_info(k: ModKey) -> (u16, usize) {
    match k {
        ModKey::MetaRight => (54, 0x10),
        ModKey::MetaLeft => (55, 0x08),
        ModKey::ShiftLeft => (56, 0x02),
        ModKey::AltLeft => (58, 0x20),
        ModKey::ControlLeft => (59, 0x01),
        ModKey::ShiftRight => (60, 0x04),
        ModKey::AltRight => (61, 0x40),
        ModKey::ControlRight => (62, 0x2000),
        ModKey::Fn => (63, 1 << 23),
    }
}

/// Modifier flags that mean "some modifier is down" (shift/ctrl/alt/cmd/fn).
const ANY_MODIFIER: usize = (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 23);

fn handle(event: &AnyObject) {
    let Some(app) = APP.get() else { return };
    // SAFETY: `type` is valid for every NSEvent.
    let ty: usize = unsafe { msg_send![event, type] };
    match ty {
        KEY_DOWN => TAINTED.store(true, Ordering::Relaxed),
        FLAGS_CHANGED => {
            // SAFETY: keyCode / modifierFlags are valid for flagsChanged events.
            let code: u16 = unsafe { msg_send![event, keyCode] };
            let flags: usize = unsafe { msg_send![event, modifierFlags] };
            on_modifier(app, code, flags);
        }
        OTHER_MOUSE_DOWN | OTHER_MOUSE_UP => {
            // SAFETY: buttonNumber is valid for mouse events.
            let button: isize = unsafe { msg_send![event, buttonNumber] };
            on_mouse(app, button, ty == OTHER_MOUSE_DOWN);
        }
        _ => {}
    }
}

fn on_modifier(app: &AppHandle, code: u16, flags: usize) {
    // Another modifier changing while ours is held turns the tap into a combo.
    if let Some(held) = held_key() {
        if key_info(held).0 != code {
            TAINTED.store(true, Ordering::Relaxed);
        }
    }
    for (binding, action) in shortcuts::native_bindings(app) {
        let Binding::Modifier(key) = binding else { continue };
        let (key_code, mask) = key_info(key);
        if key_code != code {
            continue;
        }
        let tag = key as u8 + 1;
        if flags & mask != 0 {
            // Pressed: only arm when it is the sole modifier held.
            if flags & ANY_MODIFIER & !modifier_class(key) != 0 {
                continue;
            }
            HELD.store(tag, Ordering::Relaxed);
            TAINTED.store(false, Ordering::Relaxed);
            if action == Action::PushToTalk {
                shortcuts::fire(app, action, true);
            }
        } else if HELD.load(Ordering::Relaxed) == tag {
            HELD.store(0, Ordering::Relaxed);
            if action == Action::PushToTalk {
                shortcuts::fire(app, action, false);
            } else if !TAINTED.load(Ordering::Relaxed) {
                shortcuts::fire(app, action, true);
            }
        }
    }
}

fn held_key() -> Option<ModKey> {
    use ModKey::*;
    let tag = HELD.load(Ordering::Relaxed);
    [MetaLeft, MetaRight, AltLeft, AltRight, ShiftLeft, ShiftRight, ControlLeft, ControlRight, Fn]
        .into_iter()
        .find(|k| *k as u8 + 1 == tag)
}

/// The device-independent flag for a modifier's class (both sides).
fn modifier_class(key: ModKey) -> usize {
    match key {
        ModKey::ShiftLeft | ModKey::ShiftRight => 1 << 17,
        ModKey::ControlLeft | ModKey::ControlRight => 1 << 18,
        ModKey::AltLeft | ModKey::AltRight => 1 << 19,
        ModKey::MetaLeft | ModKey::MetaRight => 1 << 20,
        ModKey::Fn => 1 << 23,
    }
}

fn on_mouse(app: &AppHandle, button: isize, down: bool) {
    let bound = shortcuts::native_bindings(app).into_iter().find_map(|(b, a)| match b {
        Binding::Mouse(n) if n as isize == button => Some(a),
        _ => None,
    });
    let Some(action) = bound else { return };
    if action != Action::Mouse {
        if down {
            shortcuts::fire(app, action, true);
        }
        return;
    }
    let status = *app.state::<AppState>().status.lock().unwrap();
    let mut held = MOUSE_DOWN.lock().unwrap();
    if down {
        match status {
            AppStatus::Idle | AppStatus::Error => {
                *held = Some((Instant::now(), true));
                drop(held);
                shortcuts::fire(app, Action::Toggle, true);
            }
            AppStatus::Recording => {
                *held = Some((Instant::now(), false));
                drop(held);
                shortcuts::fire(app, Action::Toggle, true);
            }
            AppStatus::Processing => {}
        }
    } else if let Some((at, started)) = held.take() {
        drop(held);
        if started && at.elapsed() >= HOLD && status == AppStatus::Recording {
            shortcuts::fire(app, Action::Toggle, true);
        }
    }
}
