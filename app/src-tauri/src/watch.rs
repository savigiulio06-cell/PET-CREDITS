//! "Is Claude open?" watcher: shows the pet when the Claude desktop app, the
//! native CLI (both run as a process named `claude`) or the npm CLI (seen
//! through a freshly written transcript) is running, and hides it again once
//! Claude has been gone for a while (only if the watcher itself showed it).
//!
//! Only process *names* are read, locally; nothing is stored or sent anywhere.
//! The decision logic lives in `pet_core::presence`; this file just feeds it.

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant, SystemTime};

use pet_core::claude::transcripts;
use pet_core::paths;
use pet_core::presence::{claude_signal, is_claude_process_name, PresenceTracker, Transition};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::{AppHandle, Manager};

use crate::poller::Cmd;
use crate::state::AppState;
use crate::window;

const INTERVAL: Duration = Duration::from_secs(4);
/// Looking for the newest transcript walks every project folder: do it rarely.
const TRANSCRIPT_RESCAN: Duration = Duration::from_secs(30);

pub fn spawn(app: AppHandle) {
    let _ = std::thread::Builder::new()
        .name("claude-watch".into())
        .spawn(move || run(app));
}

struct Scanner {
    sys: System,
    own: Option<sysinfo::Pid>,
    projects_dir: PathBuf,
    latest: Option<PathBuf>,
    scanned_at: Option<Instant>,
}

impl Scanner {
    fn new() -> Self {
        Self {
            sys: System::new(),
            own: sysinfo::get_current_pid().ok(),
            projects_dir: paths::claude_home().join("projects"),
            latest: None,
            scanned_at: None,
        }
    }

    /// Process names only (no command lines, paths or users), dead ones dropped.
    fn claude_process_running(&mut self) -> bool {
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing(),
        );
        self.sys.processes().iter().any(|(pid, p)| {
            Some(*pid) != self.own && is_claude_process_name(&p.name().to_string_lossy())
        })
    }

    /// Age of the newest transcript, if any.
    fn transcript_age(&mut self) -> Option<Duration> {
        if self
            .scanned_at
            .is_none_or(|t| t.elapsed() >= TRANSCRIPT_RESCAN)
        {
            self.latest = transcripts::find_latest_session(&self.projects_dir);
            self.scanned_at = Some(Instant::now());
        }
        let modified = std::fs::metadata(self.latest.as_ref()?)
            .and_then(|m| m.modified())
            .ok()?;
        // A timestamp in the future (clock change) counts as "just now".
        Some(
            SystemTime::now()
                .duration_since(modified)
                .unwrap_or(Duration::ZERO),
        )
    }
}

fn run(app: AppHandle) {
    let mut scanner = Scanner::new();
    let mut tracker = PresenceTracker::default();
    loop {
        std::thread::sleep(INTERVAL);
        let state = app.state::<AppState>();
        if !state.config().show_with_claude {
            // Feature off: no process listing at all.
            tracker = PresenceTracker::default();
            state.claude_open.store(false, Ordering::Relaxed);
            continue;
        }
        let process = scanner.claude_process_running();
        // The transcript only matters when no process was found.
        let age = if process {
            None
        } else {
            scanner.transcript_age()
        };
        match tracker.observe(Instant::now(), claude_signal(process, age)) {
            Transition::Appeared => {
                state.claude_open.store(true, Ordering::Relaxed);
                window::show_auto(&app);
                // Fresh numbers for the freshly shown pet.
                let _ = state.poller.send(Cmd::RefreshNow);
            }
            Transition::Gone => {
                state.claude_open.store(false, Ordering::Relaxed);
                window::hide_if_auto(&app);
            }
            Transition::None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_listing_works_and_ignores_our_own_process() {
        let mut s = Scanner::new();
        // The test binary is not called `claude`; the call must simply succeed.
        let _ = s.claude_process_running();
        assert!(!s.sys.processes().is_empty());
        assert!(s.own.is_some());
    }
}
