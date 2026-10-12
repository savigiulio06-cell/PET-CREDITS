//! Pure decision logic for "is Claude open right now?".
//!
//! The app crate lists the running processes and looks at the newest
//! transcript; this module only decides what those observations mean. No I/O,
//! so everything here is unit-tested headless.

use std::path::Path;
use std::time::{Duration, Instant};

/// A transcript touched within this window counts as "Claude is open". It
/// covers the npm-installed CLI, which runs as a `node` process.
pub const TRANSCRIPT_RECENT: Duration = Duration::from_secs(120);
/// Claude must stay gone this long before the pet is told to hide.
pub const GONE_AFTER: Duration = Duration::from_secs(60);

/// True for the process names of the Claude desktop app and of the native
/// Claude Code CLI: exactly `claude`, case-insensitive, any extension
/// (`claude.exe`). Nothing else (`claude-helper`, `claudex`, `node`) matches.
pub fn is_claude_process_name(name: &str) -> bool {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    Path::new(base)
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("claude"))
}

/// Combine the two observations into one "Claude looks open" signal.
/// `transcript_age` is the age of the newest transcript, if there is one.
pub fn claude_signal(process_found: bool, transcript_age: Option<Duration>) -> bool {
    process_found || transcript_age.is_some_and(|age| age <= TRANSCRIPT_RECENT)
}

/// What changed on the last observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    None,
    /// Claude was not open and now is: show the pet (at once).
    Appeared,
    /// Claude has been gone for the whole grace period: hide the pet.
    Gone,
}

/// Debounce / hysteresis: appearing is immediate, disappearing only counts
/// after [`GONE_AFTER`] of uninterrupted absence (a restart of the app or a
/// brief gap between two transcripts must not make the pet flicker).
#[derive(Debug, Clone)]
pub struct PresenceTracker {
    open: bool,
    absent_since: Option<Instant>,
    gone_after: Duration,
}

impl Default for PresenceTracker {
    fn default() -> Self {
        Self::new(GONE_AFTER)
    }
}

impl PresenceTracker {
    pub fn new(gone_after: Duration) -> Self {
        Self {
            open: false,
            absent_since: None,
            gone_after,
        }
    }

    /// Whether Claude is currently considered open (after debouncing).
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Feed one observation taken at `now`.
    pub fn observe(&mut self, now: Instant, present: bool) -> Transition {
        if present {
            self.absent_since = None;
            if self.open {
                return Transition::None;
            }
            self.open = true;
            return Transition::Appeared;
        }
        if !self.open {
            return Transition::None;
        }
        let since = *self.absent_since.get_or_insert(now);
        if now.saturating_duration_since(since) >= self.gone_after {
            self.open = false;
            self.absent_since = None;
            return Transition::Gone;
        }
        Transition::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_only_the_exact_claude_name() {
        for yes in ["claude", "claude.exe", "Claude.exe", "CLAUDE.EXE", "Claude"] {
            assert!(is_claude_process_name(yes), "{yes}");
        }
        assert!(is_claude_process_name(r"C:\Apps\Claude\claude.exe"));
        for no in [
            "",
            "node",
            "node.exe",
            "claudex",
            "claude-helper.exe",
            "Claude Helper (Renderer)",
            "my-claude",
            "claud",
            "pet-credits.exe",
        ] {
            assert!(!is_claude_process_name(no), "{no}");
        }
    }

    #[test]
    fn signal_from_process_or_fresh_transcript() {
        assert!(claude_signal(true, None));
        assert!(claude_signal(false, Some(Duration::from_secs(5))));
        assert!(claude_signal(false, Some(TRANSCRIPT_RECENT)));
        assert!(!claude_signal(
            false,
            Some(TRANSCRIPT_RECENT + Duration::from_secs(1))
        ));
        assert!(!claude_signal(false, None));
    }

    #[test]
    fn appears_immediately_and_goes_after_the_grace_period() {
        let t0 = Instant::now();
        let s = |n| t0 + Duration::from_secs(n);
        let mut p = PresenceTracker::new(Duration::from_secs(60));

        assert_eq!(p.observe(s(0), false), Transition::None);
        assert!(!p.is_open());
        assert_eq!(p.observe(s(1), true), Transition::Appeared);
        assert!(p.is_open());
        assert_eq!(p.observe(s(2), true), Transition::None);

        // Absence starts at 10 s; still open until 60 s later.
        assert_eq!(p.observe(s(10), false), Transition::None);
        assert_eq!(p.observe(s(69), false), Transition::None);
        assert!(p.is_open());
        assert_eq!(p.observe(s(70), false), Transition::Gone);
        assert!(!p.is_open());
        assert_eq!(p.observe(s(80), false), Transition::None);

        // Back again.
        assert_eq!(p.observe(s(90), true), Transition::Appeared);
    }

    #[test]
    fn a_brief_return_restarts_the_grace_period() {
        let t0 = Instant::now();
        let s = |n| t0 + Duration::from_secs(n);
        let mut p = PresenceTracker::new(Duration::from_secs(60));
        assert_eq!(p.observe(s(0), true), Transition::Appeared);
        assert_eq!(p.observe(s(10), false), Transition::None);
        assert_eq!(p.observe(s(50), true), Transition::None); // came back: no event
        assert_eq!(p.observe(s(60), false), Transition::None); // new absence from 60
        assert_eq!(p.observe(s(119), false), Transition::None);
        assert_eq!(p.observe(s(120), false), Transition::Gone);
    }
}
