//! [`ClaudeProvider`]: combines the usage API, the status-line bridge and the
//! local transcripts into one [`UsageSnapshot`].

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use chrono::{DateTime, Utc};

use super::{bridge, last_usage, oauth, transcripts};
use crate::{
    Error, Issue, Result, SessionUsage, Source, UsageProvider, UsageSnapshot, UsageWindow,
};

/// Limit data older than this is flagged stale.
const STALE_AFTER_SECS: i64 = 10 * 60;
/// Back-off after an HTTP 429 without a usable `Retry-After`.
const DEFAULT_RETRY_AFTER_SECS: u64 = 120;
/// "Active" = bridge or transcript touched within this window.
const ACTIVE_WITHIN: Duration = Duration::from_secs(120);

#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    /// Query the unofficial usage API (needs Claude Code's OAuth login).
    /// Default: false (opt-in).
    pub use_api: bool,
    pub claude_home: PathBuf,
    pub bridge_path: PathBuf,
    /// Where the last successful API response is kept across restarts.
    pub last_usage_path: PathBuf,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            use_api: false,
            claude_home: crate::paths::claude_home(),
            bridge_path: crate::paths::bridge_file(),
            last_usage_path: crate::paths::last_usage_file(),
        }
    }
}

pub struct ClaudeProvider {
    config: ClaudeConfig,
    /// No API call before this instant (set after a 429).
    api_blocked_until: Option<Instant>,
    /// Last successful API response, reused by [`ClaudeProvider::poll_local`].
    last_api: Option<(Vec<UsageWindow>, DateTime<Utc>)>,
    /// Why the last API attempt failed (None = it worked or was never tried).
    api_note: Option<&'static str>,
    /// Machine-readable counterpart of `api_note`.
    api_issue: Option<Issue>,
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new(ClaudeConfig::default())
    }
}

impl ClaudeProvider {
    pub fn new(config: ClaudeConfig) -> Self {
        let last_api = last_usage::load(&config.last_usage_path);
        Self {
            config,
            api_blocked_until: None,
            last_api,
            api_note: None,
            api_issue: None,
        }
    }

    pub fn config(&self) -> &ClaudeConfig {
        &self.config
    }

    /// Switch the usage API on or off at runtime. The 429 back-off is kept.
    pub fn set_use_api(&mut self, use_api: bool) {
        self.config.use_api = use_api;
        if !use_api {
            // Only the in-memory copy goes; the file stays for the next start.
            self.last_api = None;
            self.api_note = None;
            self.api_issue = None;
        }
    }

    /// Refresh from the local sources only (status-line bridge + transcripts)
    /// plus the last API response seen, **without** any network request.
    /// Cheap enough to call whenever a local file changes.
    pub fn poll_local(&mut self) -> Result<UsageSnapshot> {
        Ok(self.build(false))
    }

    fn build(&mut self, allow_api: bool) -> UsageSnapshot {
        let now = Utc::now();
        let mut snap = UsageSnapshot::empty("claude");
        snap.fetched_at = now;

        let bridge_data = bridge::read(&self.config.bridge_path).ok().flatten();

        if self.config.use_api && allow_api {
            match self.try_api() {
                Ok(w) => {
                    // Write errors are ignored: the cache is a convenience.
                    let _ = last_usage::save(&self.config.last_usage_path, &w, now);
                    self.last_api = Some((w, now));
                    self.api_note = None;
                    self.api_issue = None;
                }
                Err((issue, note)) => {
                    self.api_note = Some(note);
                    self.api_issue = Some(issue);
                }
            }
        }
        let api = if self.config.use_api {
            self.last_api.as_ref()
        } else {
            None
        };
        let api_note = if self.config.use_api {
            self.api_note
        } else {
            None
        };
        let issue = if self.config.use_api {
            self.api_issue
        } else {
            None
        };
        let from_bridge = bridge_data
            .as_ref()
            .filter(|b| !b.windows.is_empty())
            .map(|b| (b, b.windows_at.unwrap_or(b.written_at)));

        let mut windows_at: Option<DateTime<Utc>> = None;
        match (api, from_bridge) {
            // The API wins ties; the status line only when it is strictly fresher.
            (Some((w, at)), bridge_side) if bridge_side.is_none_or(|(_, bat)| bat <= *at) => {
                snap.windows = w.clone();
                snap.source = Source::Api;
                windows_at = Some(*at);
                snap.note = api_note.map(|n| format!("{n}; showing the last API data"));
            }
            (_, Some((b, bat))) => {
                snap.windows = b.windows.clone();
                snap.source = Source::Statusline;
                windows_at = Some(bat);
                snap.note = api_note.map(|n| format!("{n}; using the status line"));
            }
            _ => {
                snap.note = Some(match api_note {
                    Some(n) => format!("{n}; no status-line data yet"),
                    None => "No usage data yet; install the status line or enable the API".into(),
                });
            }
        }

        snap.stale = windows_at.is_none_or(|t| (now - t).num_seconds() > STALE_AFTER_SECS);
        let login_problem = matches!(issue, Some(Issue::AuthExpired | Issue::LoginMissing));
        // A login problem makes the API numbers old, but fresh status-line
        // numbers are still good: then the login hint would only mislead.
        if login_problem && snap.source == Source::Api {
            snap.stale = true;
        }
        snap.issue = if login_problem && snap.source == Source::Statusline && !snap.stale {
            None
        } else {
            issue
        };
        snap.session = self.session(bridge_data.as_ref());
        snap
    }

    /// Record a 429: pause API calls for `retry_after_secs` (default 120 s).
    fn block_api(&mut self, now: Instant, retry_after_secs: Option<u64>) {
        let secs = retry_after_secs.unwrap_or(DEFAULT_RETRY_AFTER_SECS);
        self.api_blocked_until = Some(now + Duration::from_secs(secs));
    }

    fn api_allowed(&self, now: Instant) -> bool {
        self.api_blocked_until.is_none_or(|t| now >= t)
    }

    /// Try the API. `Err` carries the [`Issue`] and a short user-safe note.
    fn try_api(&mut self) -> std::result::Result<Vec<UsageWindow>, (Issue, &'static str)> {
        const RATE_NOTE: &str = "API rate limited, waiting before retrying";
        let now = Instant::now();
        if !self.api_allowed(now) {
            return Err((Issue::RateLimited, RATE_NOTE));
        }
        let creds = match oauth::load_credentials_from(&self.config.claude_home) {
            Ok(c) => c,
            Err(Error::Auth(_)) => return Err((Issue::LoginMissing, "Claude login not found")),
            Err(_) => return Err((Issue::ApiUnavailable, "Claude credentials unreadable")),
        };
        match oauth::fetch_usage(&creds) {
            Ok(w) if !w.is_empty() => Ok(w),
            Ok(_) => Err((Issue::ApiUnavailable, "API returned no limits")),
            Err(Error::RateLimited { retry_after_secs }) => {
                self.block_api(now, retry_after_secs);
                Err((Issue::RateLimited, RATE_NOTE))
            }
            Err(Error::Auth(_)) => Err((
                Issue::AuthExpired,
                "API login expired, waiting for Claude Code to renew it",
            )),
            Err(_) => Err((Issue::ApiUnavailable, "API unavailable")),
        }
    }

    fn projects_dir(&self) -> PathBuf {
        self.config.claude_home.join("projects")
    }

    /// Session file pointed to by the bridge, if it exists and lives inside
    /// our projects dir (the bridge file is data, never trusted blindly).
    fn bridge_session(&self, b: &bridge::BridgeData) -> Option<PathBuf> {
        let path = PathBuf::from(b.transcript_path.as_deref()?);
        let real = path.canonicalize().ok()?;
        let root = self.projects_dir().canonicalize().ok()?;
        (real.starts_with(root) && real.is_file()).then_some(real)
    }

    fn session(&self, b: Option<&bridge::BridgeData>) -> Option<SessionUsage> {
        let file = b
            .and_then(|b| self.bridge_session(b))
            .or_else(|| transcripts::find_latest_session(&self.projects_dir()))?;
        transcripts::summarize(&file).ok()
    }
}

fn modified_recently(path: &Path, now: SystemTime) -> bool {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| now.duration_since(m).ok())
        .is_some_and(|age| age <= ACTIVE_WITHIN)
}

impl UsageProvider for ClaudeProvider {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn name(&self) -> &'static str {
        "Claude Code"
    }

    fn poll(&mut self) -> Result<UsageSnapshot> {
        Ok(self.build(true))
    }

    fn is_active(&self) -> bool {
        let now = SystemTime::now();
        if modified_recently(&self.config.bridge_path, now) {
            return true;
        }
        transcripts::find_latest_session(&self.projects_dir())
            .is_some_and(|p| modified_recently(&p, now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_blocks_api_until_retry_after() {
        let mut p = ClaudeProvider::new(ClaudeConfig {
            use_api: true,
            claude_home: PathBuf::from("/nonexistent"),
            bridge_path: PathBuf::from("/nonexistent/bridge.json"),
            last_usage_path: PathBuf::from("/nonexistent/last-usage.json"),
        });
        let t0 = Instant::now();
        assert!(p.api_allowed(t0));
        p.block_api(t0, Some(30));
        assert!(!p.api_allowed(t0 + Duration::from_secs(29)));
        assert!(p.api_allowed(t0 + Duration::from_secs(30)));
        p.block_api(t0, None);
        assert!(!p.api_allowed(t0 + Duration::from_secs(119)));
        assert!(p.api_allowed(t0 + Duration::from_secs(120)));
    }

    fn window(pct: f64) -> UsageWindow {
        UsageWindow {
            kind: crate::WindowKind::Session,
            label: "Session (5h)".into(),
            used_pct: pct,
            resets_at: None,
            window_minutes: Some(300),
        }
    }

    fn write_bridge(path: &Path, pct: f64, windows_at: DateTime<Utc>) {
        let data = bridge::BridgeData {
            written_at: windows_at,
            session_id: None,
            transcript_path: None,
            model: None,
            session_cost_usd: None,
            windows: vec![window(pct)],
            windows_at: Some(windows_at),
        };
        bridge::write(path, &data).unwrap();
    }

    #[test]
    fn poll_local_never_calls_the_api_and_picks_the_freshest_windows() {
        let dir = tempfile::tempdir().unwrap();
        let bridge_path = dir.path().join("bridge.json");
        // use_api = true but no credentials in claude_home: a real poll() would
        // report "Claude login not found"; poll_local() must not even try.
        let mut p = ClaudeProvider::new(ClaudeConfig {
            use_api: true,
            claude_home: dir.path().join("claude"),
            bridge_path: bridge_path.clone(),
            last_usage_path: dir.path().join("last-usage.json"),
        });

        let snap = p.poll_local().unwrap();
        assert_eq!(snap.source, Source::None);
        assert!(snap.stale);
        assert!(!snap.note.unwrap_or_default().contains("login"));

        // Bridge only.
        write_bridge(&bridge_path, 30.0, Utc::now());
        let snap = p.poll_local().unwrap();
        assert_eq!(snap.source, Source::Statusline);
        assert_eq!(snap.windows[0].used_pct, 30.0);
        assert!(!snap.stale);

        // Cached API response newer than the bridge: the API wins.
        p.last_api = Some((
            vec![window(55.0)],
            Utc::now() + chrono::Duration::seconds(5),
        ));
        let snap = p.poll_local().unwrap();
        assert_eq!(snap.source, Source::Api);
        assert_eq!(snap.windows[0].used_pct, 55.0);

        // Bridge newer than the cached API response: the status line wins.
        write_bridge(
            &bridge_path,
            61.0,
            Utc::now() + chrono::Duration::seconds(60),
        );
        let snap = p.poll_local().unwrap();
        assert_eq!(snap.source, Source::Statusline);
        assert_eq!(snap.windows[0].used_pct, 61.0);

        // API switched off: the cache is dropped.
        p.set_use_api(false);
        assert!(p.last_api.is_none());
    }
}
