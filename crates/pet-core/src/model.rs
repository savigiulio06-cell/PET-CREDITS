//! Normalized data model shared by every provider and sent as-is (JSON) to the UI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Which kind of usage window a limit refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowKind {
    /// Rolling short window (Claude: 5 hours, Codex: "primary").
    Session,
    /// Rolling 7-day window across all models.
    Weekly,
    /// 7-day window scoped to a model family (e.g. Opus).
    WeeklyScoped,
    /// Anything we don't recognise yet; kept so new limits still show up.
    Other,
}

/// One rate-limit window, e.g. "5h session: 42% used, resets at 17:00".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageWindow {
    pub kind: WindowKind,
    /// Human label, e.g. "Session (5h)", "Weekly", "Weekly · Opus".
    pub label: String,
    /// 0.0 ..= 100.0 (may exceed 100 if the provider reports overage).
    pub used_pct: f64,
    pub resets_at: Option<DateTime<Utc>>,
    /// Window length in minutes if known (300 for 5h, 10080 for 7d).
    pub window_minutes: Option<u32>,
}

/// Token counters, summed over messages.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TokenTotals {
    pub input: u64,
    pub output: u64,
    pub cache_creation: u64,
    pub cache_read: u64,
}

impl TokenTotals {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_creation + self.cache_read
    }
    pub fn add(&mut self, other: &TokenTotals) {
        self.input += other.input;
        self.output += other.output;
        self.cache_creation += other.cache_creation;
        self.cache_read += other.cache_read;
    }
}

/// Consumption of one agent (main thread or a subagent) inside a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentUsage {
    /// "main" for the main conversation, otherwise the subagent id/name.
    pub agent: String,
    pub model: Option<String>,
    pub tokens: TokenTotals,
    /// Rough API-price equivalent in USD; subscription users don't pay this,
    /// it is only a relative measure of "how heavy" an agent is.
    pub cost_usd_est: Option<f64>,
    pub requests: u32,
}

/// The currently active (most recent) local session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionUsage {
    pub session_id: String,
    /// Project folder name as stored by the provider (no absolute path).
    pub project: Option<String>,
    pub last_activity: Option<DateTime<Utc>>,
    pub tokens: TokenTotals,
    pub cost_usd_est: Option<f64>,
    pub agents: Vec<AgentUsage>,
}

/// Where the limit numbers came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Same endpoint the provider's settings page uses.
    Api,
    /// Pushed by the Claude Code status line on every API response.
    Statusline,
    /// Read from local session logs (Codex rollouts, transcripts).
    LocalLogs,
    None,
}

/// Why the numbers cannot be refreshed; lets the UI show a clear message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Issue {
    /// Login token expired locally or rejected by the API (401/403).
    AuthExpired,
    /// No Claude Code credentials were found.
    LoginMissing,
    /// The API asked us to slow down (429).
    RateLimited,
    /// Network error, unexpected format or anything else.
    ApiUnavailable,
}

/// Everything the pet needs to draw its sign.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageSnapshot {
    /// Provider id, e.g. "claude", "codex".
    pub provider: String,
    pub windows: Vec<UsageWindow>,
    pub session: Option<SessionUsage>,
    pub source: Source,
    pub fetched_at: DateTime<Utc>,
    /// True when the numbers are older than the freshness threshold.
    pub stale: bool,
    /// Short, user-safe message when something is degraded (never secrets).
    pub note: Option<String>,
    /// Machine-readable reason the data cannot be refreshed (None = all fine).
    #[serde(default)]
    pub issue: Option<Issue>,
}

impl UsageSnapshot {
    pub fn empty(provider: &str) -> Self {
        Self {
            provider: provider.to_string(),
            windows: Vec::new(),
            session: None,
            source: Source::None,
            fetched_at: Utc::now(),
            stale: true,
            note: None,
            issue: None,
        }
    }

    pub fn window(&self, kind: WindowKind) -> Option<&UsageWindow> {
        self.windows.iter().find(|w| w.kind == kind)
    }
}
