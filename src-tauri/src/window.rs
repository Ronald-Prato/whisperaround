//! Floating-window layout helpers shared by the setup code (`lib.rs`) and the
//! IPC commands (`commands.rs`). The `main` window is a single always-on-top
//! surface that resizes to fit whatever the pill is showing — an empty pill at
//! rest, the icon row / tooltip on hover, the model menu, the recording state,
//! or the expanded panel. Resizes keep the top-center fixed so the pill stays
//! where the user dragged it instead of snapping back to the top.

use tauri::{LogicalSize, PhysicalPosition, WebviewWindow};

/// Resting (empty) pill window size (logical px). Must match `IDLE` in
/// `src/lib/Pill.svelte`, which drives every other size.
pub const PILL_SIZE: (f64, f64) = (72.0, 40.0);
/// Gap between the window and the top edge of the screen (logical px).
const TOP_MARGIN: f64 = 12.0;

/// Resize and re-pin to the top-center of the active monitor. Used on launch
/// and when revealing the pill from the tray (a deliberate "reset to top").
pub fn layout_main(window: &WebviewWindow, size: (f64, f64)) {
    let _ = window.set_size(LogicalSize::new(size.0, size.1));
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    let screen = monitor.size();
    let origin = monitor.position();
    let win_w = size.0 * scale;
    let x = origin.x as f64 + (screen.width as f64 - win_w) / 2.0;
    let y = origin.y as f64 + TOP_MARGIN * scale;
    let _ = window.set_position(PhysicalPosition::new(x.round(), y.round()));
}

/// Resize keeping the top edge and horizontal center fixed (the pill grows
/// downward and symmetrically), clamped to the monitor.
pub fn resize_anchored(window: &WebviewWindow, size: (f64, f64)) {
    let (pos, cur) = match (window.outer_position(), window.outer_size()) {
        (Ok(p), Ok(c)) => (p, c),
        _ => {
            let _ = window.set_size(LogicalSize::new(size.0, size.1));
            return;
        }
    };
    let scale = window.scale_factor().unwrap_or(1.0);
    let new_w = size.0 * scale;
    let new_h = size.1 * scale;

    let center_x = pos.x as f64 + cur.width as f64 / 2.0;
    let mut nx = center_x - new_w / 2.0;
    let mut ny = pos.y as f64;

    // Keep the whole window on the active monitor.
    if let Ok(Some(m)) = window.current_monitor() {
        let ms = m.size();
        let mo = m.position();
        let max_x = mo.x as f64 + ms.width as f64 - new_w;
        let max_y = mo.y as f64 + ms.height as f64 - new_h;
        nx = nx.clamp(mo.x as f64, max_x.max(mo.x as f64));
        ny = ny.clamp(mo.y as f64, max_y.max(mo.y as f64));
    }

    set_bounds(window, nx.round() as i32, ny.round() as i32, new_w.round() as i32, new_h.round() as i32);
}

/// Move + resize in one native call where possible. Two separate calls (size,
/// then position) briefly show the content off-center — a visible jump.
#[cfg(windows)]
fn set_bounds(window: &WebviewWindow, x: i32, y: i32, w: i32, h: i32) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER,
    };
    if let Ok(hwnd) = window.hwnd() {
        let flags = SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
        // SAFETY: `hwnd` is the live native handle of this Tauri window.
        if unsafe { SetWindowPos(HWND(hwnd.0), None, x, y, w, h, flags) }.is_ok() {
            return;
        }
    }
    let _ = window.set_size(tauri::PhysicalSize::new(w as u32, h as u32));
    let _ = window.set_position(PhysicalPosition::new(x, y));
}

#[cfg(not(windows))]
fn set_bounds(window: &WebviewWindow, x: i32, y: i32, w: i32, h: i32) {
    let _ = window.set_size(tauri::PhysicalSize::new(w as u32, h as u32));
    let _ = window.set_position(PhysicalPosition::new(x, y));
}
