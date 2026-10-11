//! Background polling thread around [`ClaudeProvider`].
//!
//! - Full poll (API allowed): every 60 s while Claude Code is active, every
//!   300 s otherwise. The provider applies the 429 back-off by itself.
//! - Local poll (bridge + transcripts, no network): as soon as the status-line
//!   bridge file or the active transcript changes (checked every 2 s).
//!
//! Every new snapshot is stored in [`AppState`] and sent to the UI as
//! `usage-update`; `activity` is sent whenever "Claude is working" flips.
//! This module never touches the network or credentials itself.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime};

use pet_core::claude::provider::ClaudeConfig;
use pet_core::claude::{transcripts, ClaudeProvider};
use pet_core::UsageProvider;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

const TICK: Duration = Duration::from_secs(2);
const API_EVERY_ACTIVE: Duration = Duration::from_secs(60);
const API_EVERY_IDLE: Duration = Duration::from_secs(300);
/// "Aggiorna ora" within this gap of the last full poll only re-reads local files.
const MIN_MANUAL_GAP: Duration = Duration::from_secs(5);
const ACTIVITY_RECHECK: Duration = Duration::from_secs(10);
const TRANSCRIPT_RESCAN: Duration = Duration::from_secs(30);

pub enum Cmd {
    /// "Aggiorna ora".
    RefreshNow,
    /// Toggle the (unofficial) usage API.
    SetUseApi(bool),
}

pub fn spawn(app: AppHandle, rx: Receiver<Cmd>, use_api: bool) {
    let _ = std::thread::Builder::new()
        .name("poller".into())
        .spawn(move || run(app, rx, use_api));
}

/// What "something changed locally" means: bridge mtime + active transcript.
#[derive(Debug, Clone, PartialEq, Default)]
struct Stamp {
    bridge: Option<SystemTime>,
    transcript: Option<(PathBuf, Option<SystemTime>)>,
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

struct Watcher {
    bridge_path: PathBuf,
    projects_dir: PathBuf,
    transcript: Option<PathBuf>,
    scanned_at: Option<Instant>,
    last_bridge: Option<SystemTime>,
}

impl Watcher {
    fn new(cfg: &ClaudeConfig) -> Self {
        Self {
            bridge_path: cfg.bridge_path.clone(),
            projects_dir: cfg.claude_home.join("projects"),
            transcript: None,
            scanned_at: None,
            last_bridge: None,
        }
    }

    fn stamp(&mut self) -> Stamp {
        let bridge = mtime(&self.bridge_path);
        // Looking for the newest transcript walks every project folder: only do
        // it now and then, or right after the status line wrote something.
        let rescan = bridge != self.last_bridge
            || self
                .scanned_at
                .is_none_or(|t| t.elapsed() >= TRANSCRIPT_RESCAN);
        if rescan {
            self.transcript = transcripts::find_latest_session(&self.projects_dir);
            self.scanned_at = Some(Instant::now());
            self.last_bridge = bridge;
        }
        Stamp {
            bridge,
            transcript: self.transcript.clone().map(|p| {
                let m = mtime(&p);
                (p, m)
            }),
        }
    }
}

fn publish(app: &AppHandle, snap: pet_core::UsageSnapshot) {
    let state = app.state::<AppState>();
    state.set_snapshot(snap.clone());
    let _ = app.emit("usage-update", &snap);
}

fn run(app: AppHandle, rx: Receiver<Cmd>, use_api: bool) {
    let mut provider = ClaudeProvider::new(ClaudeConfig {
        use_api,
        ..ClaudeConfig::default()
    });
    let mut watcher = Watcher::new(provider.config());

    // Show something right away from local files, then do the first full poll.
    if let Ok(s) = provider.poll_local() {
        publish(&app, s);
    }
    let mut active = provider.is_active();
    let _ = app.emit("activity", active);
    let mut last_stamp = watcher.stamp();
    let mut last_full: Option<Instant> = None;
    let mut last_activity_check = Instant::now();
    let mut force_full = true;
    let mut force_local = false;

    loop {
        match rx.recv_timeout(TICK) {
            Ok(Cmd::RefreshNow) => {
                if last_full.is_none_or(|t| t.elapsed() >= MIN_MANUAL_GAP) {
                    force_full = true;
                } else {
                    force_local = true;
                }
            }
            Ok(Cmd::SetUseApi(on)) => {
                provider.set_use_api(on);
                if on {
                    force_full = true;
                } else {
                    force_local = true;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        let stamp = watcher.stamp();
        let changed = stamp != last_stamp;
        last_stamp = stamp;

        let every = if active {
            API_EVERY_ACTIVE
        } else {
            API_EVERY_IDLE
        };
        let due = last_full.is_none_or(|t| t.elapsed() >= every);

        let snap = if force_full || due {
            force_full = false;
            force_local = false;
            last_full = Some(Instant::now());
            provider.poll().ok()
        } else if changed || force_local {
            force_local = false;
            provider.poll_local().ok()
        } else {
            None
        };
        let refreshed = snap.is_some();
        if let Some(s) = snap {
            publish(&app, s);
        }

        if changed || refreshed || last_activity_check.elapsed() >= ACTIVITY_RECHECK {
            last_activity_check = Instant::now();
            let now_active = provider.is_active();
            if now_active != active || refreshed {
                // Also re-sent after a full poll: a UI that loaded late still catches up.
                let _ = app.emit("activity", now_active);
            }
            active = now_active;
        }
    }
}
