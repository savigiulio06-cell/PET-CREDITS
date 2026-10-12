//! Placement, resizing and position persistence of the pet window.

use std::sync::atomic::Ordering;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewWindow};

use crate::config::Position;
use crate::poller::Cmd;
use crate::state::AppState;

pub const LABEL: &str = "main";
pub const WIDTH: f64 = 320.0;
pub const HEIGHT_COMPACT: f64 = 280.0;
pub const HEIGHT_EXPANDED: f64 = 460.0;
/// Gap between the window and the screen edges on first launch (logical px).
const MARGIN: f64 = 24.0;
const SAVE_DEBOUNCE: Duration = Duration::from_millis(600);

pub fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

pub fn is_visible(app: &AppHandle) -> bool {
    main_window(app).is_some_and(|w| w.is_visible().unwrap_or(false))
}

/// Manual show (tray click / menu / manual launch): un-minimize and focus the
/// pet and refresh the numbers (the poller ignores it within a few seconds of
/// the last poll). A pet the user opened by hand is never hidden by the Claude
/// watcher.
pub fn show_main(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.auto_shown.store(false, Ordering::Relaxed);
    let _ = state.poller.send(Cmd::RefreshNow);
    if let Some(w) = main_window(app) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn toggle_main(app: &AppHandle) {
    if is_visible(app) {
        app.state::<AppState>()
            .auto_shown
            .store(false, Ordering::Relaxed);
        if let Some(w) = main_window(app) {
            let _ = w.hide();
        }
    } else {
        show_main(app);
    }
}

/// Automatic show (Claude opened, SessionStart hook): no focus stealing. Only
/// acts on a hidden window; returns whether it did. Tells the UI (`pet-appear`)
/// so sprite skins replay their entrance.
pub fn show_auto(app: &AppHandle) -> bool {
    let Some(w) = main_window(app) else {
        return false;
    };
    if w.is_visible().unwrap_or(false) {
        // Already there, maybe minimized: bring it back without taking focus.
        if w.is_minimized().unwrap_or(false) {
            let _ = w.unminimize();
        }
        return false;
    }
    app.state::<AppState>()
        .auto_shown
        .store(true, Ordering::Relaxed);
    let _ = w.show();
    let _ = w.unminimize();
    let _ = app.emit("pet-appear", ());
    true
}

/// Hide the pet after Claude went away, but only if it was shown automatically.
pub fn hide_if_auto(app: &AppHandle) {
    let was_auto = app
        .state::<AppState>()
        .auto_shown
        .swap(false, Ordering::Relaxed);
    if was_auto {
        if let Some(w) = main_window(app) {
            let _ = w.hide();
        }
    }
}

fn phys(logical: f64, scale: f64) -> i32 {
    (logical * scale).round() as i32
}

fn contains(m: &Monitor, x: i32, y: i32) -> bool {
    let p = m.position();
    let s = m.size();
    x >= p.x && y >= p.y && x < p.x + s.width as i32 && y < p.y + s.height as i32
}

/// Where the compact window goes: the saved spot if it is still on a screen,
/// otherwise the bottom-right of the primary monitor, above the taskbar.
fn initial_position(w: &WebviewWindow, saved: Option<Position>) -> Option<PhysicalPosition<i32>> {
    let scale = w.scale_factor().unwrap_or(1.0);
    let (win_w, win_h) = (phys(WIDTH, scale), phys(HEIGHT_COMPACT, scale));
    let monitors = w.available_monitors().unwrap_or_default();

    if let Some(p) = saved {
        let (cx, cy) = (p.x + win_w / 2, p.y + win_h / 2);
        if monitors.iter().any(|m| contains(m, cx, cy)) {
            return Some(PhysicalPosition::new(p.x, p.y));
        }
    }

    let monitor = w
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| monitors.into_iter().next())?;
    let area = monitor.work_area(); // excludes the taskbar / dock
    let margin = phys(MARGIN, monitor.scale_factor());
    Some(PhysicalPosition::new(
        area.position.x + area.size.width as i32 - win_w - margin,
        area.position.y + area.size.height as i32 - win_h - margin,
    ))
}

/// Position the window (it is created hidden); `show` = false keeps it hidden
/// (`--standby`).
pub fn place(w: &WebviewWindow, saved: Option<Position>, show: bool) {
    if let Some(pos) = initial_position(w, saved) {
        let _ = w.set_position(pos);
    }
    if show {
        let _ = w.show();
    }
}

/// 320x280 (compact) or 320x460 (expanded), keeping the bottom edge in place.
pub fn set_expanded(w: &WebviewWindow, expanded: bool) -> Result<(), String> {
    let err = |_| "window operation failed".to_string();
    let scale = w.scale_factor().map_err(err)?;
    let target_h = phys(
        if expanded {
            HEIGHT_EXPANDED
        } else {
            HEIGHT_COMPACT
        },
        scale,
    );
    let size = w.outer_size().map_err(err)?;
    let pos = w.outer_position().map_err(err)?;
    let cur_h = size.height as i32;
    if cur_h == target_h {
        return Ok(());
    }
    let mut y = pos.y + cur_h - target_h;
    if let Ok(Some(m)) = w.current_monitor() {
        // Never grow past the top of the screen.
        y = y.max(m.work_area().position.y);
    }
    w.set_size(PhysicalSize::new(
        phys(WIDTH, scale) as u32,
        target_h as u32,
    ))
    .map_err(err)?;
    w.set_position(PhysicalPosition::new(pos.x, y)).map_err(err)
}

/// Persist the window position once dragging has stopped for a moment.
/// The stored value is always the top-left of the *compact* window, so it
/// does not depend on whether the detail panel happens to be open.
pub fn spawn_position_saver(app: AppHandle, rx: Receiver<()>) {
    let _ = std::thread::Builder::new()
        .name("position-saver".into())
        .spawn(move || {
            while rx.recv().is_ok() {
                while rx.recv_timeout(SAVE_DEBOUNCE).is_ok() {}
                save_position(&app);
            }
        });
}

fn save_position(app: &AppHandle) {
    let Some(w) = main_window(app) else { return };
    if !w.is_visible().unwrap_or(false) || w.is_minimized().unwrap_or(false) {
        return; // hidden/minimized windows report bogus coordinates
    }
    let (Ok(pos), Ok(size), Ok(scale)) = (w.outer_position(), w.outer_size(), w.scale_factor())
    else {
        return;
    };
    let compact_h = phys(HEIGHT_COMPACT, scale);
    let position = Position {
        x: pos.x,
        y: pos.y + size.height as i32 - compact_h,
    };
    app.state::<AppState>()
        .update_config(|c| c.position = Some(position));
}
