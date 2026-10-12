//! Shared application state (managed by Tauri).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use pet_core::UsageSnapshot;
use tauri::menu::Menu;
use tauri::Wry;

use crate::config::Config;
use crate::poller::Cmd;

pub struct AppState {
    snapshot: Mutex<Option<UsageSnapshot>>,
    config: Mutex<Config>,
    app_home: PathBuf,
    pub poller: Sender<Cmd>,
    /// Tells the position-saver thread that the window moved.
    pub moved: Sender<()>,
    /// The pet menu, also opened by a right click on the pet (set once the menu is built).
    pub menu: OnceLock<Menu<Wry>>,
    /// The window was shown by the Claude watcher (or the SessionStart hook), so
    /// the watcher may hide it again. Cleared by every manual show.
    pub auto_shown: AtomicBool,
    /// Claude is considered open (set by the presence watcher, read by the poller).
    pub claude_open: AtomicBool,
    /// The last attempt to change the login entry failed (shown in the details panel).
    pub login_error: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panicked holder cannot leave these plain values inconsistent.
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl AppState {
    pub fn new(config: Config, app_home: PathBuf, poller: Sender<Cmd>, moved: Sender<()>) -> Self {
        Self {
            snapshot: Mutex::new(None),
            config: Mutex::new(config),
            app_home,
            poller,
            moved,
            menu: OnceLock::new(),
            auto_shown: AtomicBool::new(false),
            claude_open: AtomicBool::new(false),
            login_error: AtomicBool::new(false),
        }
    }

    pub fn snapshot(&self) -> UsageSnapshot {
        lock(&self.snapshot)
            .clone()
            .unwrap_or_else(|| UsageSnapshot::empty("claude"))
    }

    pub fn set_snapshot(&self, snap: UsageSnapshot) {
        *lock(&self.snapshot) = Some(snap);
    }

    pub fn config(&self) -> Config {
        lock(&self.config).clone()
    }

    /// Change the config and persist it (a failed write is not fatal: the
    /// setting simply won't survive a restart).
    pub fn update_config(&self, f: impl FnOnce(&mut Config)) {
        let mut cfg = lock(&self.config);
        f(&mut cfg);
        let _ = cfg.save(&self.app_home);
    }
}
