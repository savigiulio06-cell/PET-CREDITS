//! The two user choices that have side effects, shared by the tray menu and the
//! first-run card: live usage (starts/stops API polling) and "show when Claude
//! opens" (login entry + watcher). Both are saved in `config.json`.

use serde::Serialize;
use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager};

use crate::autostart;
use crate::poller::Cmd;
use crate::state::AppState;
use crate::tray;

/// What the page needs to know (`get_prefs`, event `prefs-changed`).
#[derive(Clone, Serialize)]
pub struct Prefs {
    pub onboarded: bool,
    pub live_usage: bool,
    pub show_with_claude: bool,
    /// Changing the login entry failed; the choice was reset to the real state.
    pub login_error: bool,
}

pub fn current(app: &AppHandle) -> Prefs {
    let c = app.state::<AppState>().config();
    Prefs {
        onboarded: c.onboarded,
        live_usage: c.use_api,
        show_with_claude: c.show_with_claude,
        login_error: app.state::<AppState>().login_error.load(Ordering::Relaxed),
    }
}

fn announce(app: &AppHandle) {
    let _ = app.emit("prefs-changed", current(app));
}

pub fn set_live_usage(app: &AppHandle, on: bool) {
    let state = app.state::<AppState>();
    state.update_config(|c| c.use_api = on);
    let _ = state.poller.send(Cmd::SetUseApi(on));
    announce(app);
}

/// Register / remove the login entry (started with `--standby`) and remember
/// the choice. If the OS refuses, nothing is saved: config and menu check are
/// put back to the real state of the entry and the details panel / tooltip
/// show a note. The watcher thread reads the saved flag, so it follows at once.
pub fn set_show_with_claude(app: &AppHandle, on: bool) {
    let state = app.state::<AppState>();
    let failed = autostart::set(on).is_err();
    let value = if failed { autostart::actual(on) } else { on };
    state.update_config(|c| c.show_with_claude = value);
    let was_failed = state.login_error.swap(failed, Ordering::Relaxed);
    if failed || was_failed {
        if let Some(tray) = app.tray_by_id("main") {
            let tip = if failed {
                "PET-CREDITS - voce di avvio non modificabile"
            } else {
                "PET-CREDITS"
            };
            let _ = tray.set_tooltip(Some(tip));
        }
    }
    tray::sync_checks(app);
    announce(app);
}

/// Save the answers of the first-run card. Does nothing if it was answered
/// before. Returns whether anything was applied.
pub fn complete_onboarding(app: &AppHandle, show_with_claude: bool, live_usage: bool) -> bool {
    let state = app.state::<AppState>();
    if state.config().onboarded {
        return false;
    }
    state.update_config(|c| c.onboarded = true);
    set_show_with_claude(app, show_with_claude);
    set_live_usage(app, live_usage);
    tray::sync_checks(app);
    true
}
