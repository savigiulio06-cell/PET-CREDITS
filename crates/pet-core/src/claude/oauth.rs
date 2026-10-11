//! Live usage limits from the endpoint behind Claude's settings page.
//!
//! Security rules (see CLAUDE.md): the OAuth access token is read only in
//! memory, only at request time, never written to disk, logged, put in an error
//! message or handed to the UI. It is sent exclusively to `api.anthropic.com`
//! over HTTPS. The refresh token is never read and we NEVER refresh: when the
//! access token has expired we simply wait for Claude Code to renew it.

use std::fmt;
use std::path::Path;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::{Error, Result, UsageWindow, WindowKind};

/// Hard-coded on purpose: the token must never be sendable anywhere else.
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const BETA_HEADER: &str = "oauth-2025-04-20";
const TIMEOUT: Duration = Duration::from_secs(10);
const CREDENTIALS_FILE: &str = ".credentials.json";

pub const SESSION_LABEL: &str = "Session (5h)";
pub const WEEKLY_LABEL: &str = "Weekly";
pub const SESSION_MINUTES: u32 = 300;
pub const WEEKLY_MINUTES: u32 = 10_080;

/// A string whose `Debug` output never reveals the content.
struct Secret(String);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("\"***\"")
    }
}

/// Access-token credentials of Claude Code. Deliberately has no refresh token
/// and no `Clone`/`Serialize`, so the secret cannot be copied around by accident.
pub struct OAuthCredentials {
    access_token: Secret,
    pub expires_at: Option<DateTime<Utc>>,
    pub subscription_type: Option<String>,
}

impl fmt::Debug for OAuthCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuthCredentials")
            .field("access_token", &self.access_token)
            .field("expires_at", &self.expires_at)
            .field("subscription_type", &self.subscription_type)
            .finish()
    }
}

impl OAuthCredentials {
    pub fn new(
        access_token: impl Into<String>,
        expires_at: Option<DateTime<Utc>>,
        subscription_type: Option<String>,
    ) -> Self {
        Self {
            access_token: Secret(access_token.into()),
            expires_at,
            subscription_type,
        }
    }

    /// True if the token's expiry is known and already in the past.
    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|e| e < now)
    }

    pub fn is_expired(&self) -> bool {
        self.is_expired_at(Utc::now())
    }
}

#[derive(Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    oauth: Option<OAuthSection>,
}

/// Only the fields we need; `refreshToken` and everything else is skipped by
/// serde and never stored.
#[derive(Deserialize)]
struct OAuthSection {
    #[serde(rename = "accessToken")]
    access_token: Option<String>,
    #[serde(rename = "expiresAt")]
    expires_at: Option<f64>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
}

/// Parse the credentials JSON. Error messages never echo the input.
pub fn parse_credentials(json: &str) -> Result<OAuthCredentials> {
    let file: CredentialsFile = serde_json::from_str(json)
        .map_err(|_| Error::Format("credentials JSON is malformed".into()))?;
    let section = file
        .oauth
        .ok_or_else(|| Error::Auth("no Claude Code login found in credentials".into()))?;
    let token = section
        .access_token
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| Error::Auth("no access token in credentials".into()))?;
    let expires_at = section
        .expires_at
        .filter(|v| v.is_finite() && *v > 0.0)
        .and_then(|ms| Utc.timestamp_millis_opt(ms as i64).single());
    Ok(OAuthCredentials::new(
        token,
        expires_at,
        section.subscription_type,
    ))
}

/// Load credentials from `crate::paths::claude_home()`.
pub fn load_credentials() -> Result<OAuthCredentials> {
    load_credentials_from(&crate::paths::claude_home())
}

/// Load credentials from `<claude_home>/.credentials.json`; on macOS, when the
/// file is missing, from the login keychain item used by Claude Code.
pub fn load_credentials_from(claude_home: &Path) -> Result<OAuthCredentials> {
    match std::fs::read_to_string(claude_home.join(CREDENTIALS_FILE)) {
        Ok(text) => parse_credentials(&text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => match read_keychain() {
            Some(text) => parse_credentials(&text),
            None => Err(Error::Auth(
                "no Claude Code credentials found; log in with `claude` first".into(),
            )),
        },
        Err(e) => Err(Error::Io(e)),
    }
}

#[cfg(target_os = "macos")]
fn read_keychain() -> Option<String> {
    use std::process::{Command, Stdio};
    // Fixed arguments, no shell, no external input.
    let out = Command::new("/usr/bin/security")
        .args([
            "find-generic-password",
            "-s",
            "Claude Code-credentials",
            "-w",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

#[cfg(not(target_os = "macos"))]
fn read_keychain() -> Option<String> {
    None
}

/// Fetch the current usage windows. Never refreshes the token.
pub fn fetch_usage(creds: &OAuthCredentials) -> Result<Vec<UsageWindow>> {
    if creds.is_expired() {
        return Err(Error::Auth(
            "token expired; waiting for Claude Code to refresh it".into(),
        ));
    }

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        // Never follow redirects: the bearer token must not leave our one URL.
        .max_redirects(0)
        .build()
        .into();

    let mut response = agent
        .get(USAGE_URL)
        .header("Authorization", format!("Bearer {}", creds.access_token.0))
        .header("anthropic-beta", BETA_HEADER)
        .header(
            "User-Agent",
            concat!("pet-credits/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .map_err(|e| match e {
            ureq::Error::Timeout(_) => Error::Http("request timed out".into()),
            _ => Error::Http("network error".into()),
        })?;

    let status = response.status().as_u16();
    match status {
        200..=299 => {}
        401 | 403 => return Err(Error::Auth("usage API rejected the credentials".into())),
        429 => {
            let retry_after_secs = response
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok());
            return Err(Error::RateLimited { retry_after_secs });
        }
        other => return Err(Error::Http(format!("status {other}"))),
    }

    let body = response
        .body_mut()
        .with_config()
        .limit(1024 * 1024)
        .read_to_string()
        .map_err(|_| Error::Http("could not read response body".into()))?;
    parse_usage_response(&body)
}

// ---------------------------------------------------------------------------
// Pure parsing
// ---------------------------------------------------------------------------

/// Parse a point in time given as ISO-8601 string or epoch number (seconds, or
/// milliseconds when > 1e12). Shared with the status-line bridge.
pub(crate) fn parse_time(v: &Value) -> Option<DateTime<Utc>> {
    match v {
        Value::String(s) => {
            let s = s.trim();
            if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                return Some(dt.with_timezone(&Utc));
            }
            // Numeric strings ("1760000000") are tolerated too.
            s.parse::<f64>().ok().and_then(epoch_to_time)
        }
        Value::Number(n) => n.as_f64().and_then(epoch_to_time),
        _ => None,
    }
}

fn epoch_to_time(n: f64) -> Option<DateTime<Utc>> {
    if !n.is_finite() || n < 0.0 {
        return None;
    }
    if n > 1e12 {
        Utc.timestamp_millis_opt(n as i64).single()
    } else {
        Utc.timestamp_opt(n as i64, 0).single()
    }
}

fn number(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|n| n.is_finite())
}

/// Parse the body of `GET /api/oauth/usage`. Tolerant of both known shapes:
///
/// * old: `{"five_hour":{"utilization":42.0,"resets_at":..},"seven_day":{..},
///   "seven_day_opus":{..}|null, ..}`
/// * new: `{"limits":[{"kind":"session","percent":42,"resets_at":..},..]}`
///
/// When both are present a non-empty `limits[]` wins. Percentages are taken as
/// 0-100 and are NOT rescaled: if every value is <= 1.0 the unit is ambiguous
/// (0.4 could mean 0.4% or 40%), so we leave the numbers untouched. Unknown
/// fields are ignored.
pub fn parse_usage_response(body: &str) -> Result<Vec<UsageWindow>> {
    let root: Value = serde_json::from_str(body)
        .map_err(|_| Error::Format("usage response is not valid JSON".into()))?;
    let obj = root
        .as_object()
        .ok_or_else(|| Error::Format("usage response is not a JSON object".into()))?;

    if let Some(limits) = obj.get("limits").and_then(Value::as_array) {
        let windows: Vec<UsageWindow> = limits.iter().filter_map(parse_limit_entry).collect();
        if !windows.is_empty() {
            return Ok(windows);
        }
    }

    let mut windows = Vec::new();
    if let Some(w) = obj.get("five_hour").and_then(legacy_window) {
        windows.push(window(
            WindowKind::Session,
            SESSION_LABEL,
            w,
            SESSION_MINUTES,
        ));
    }
    if let Some(w) = obj.get("seven_day").and_then(legacy_window) {
        windows.push(window(WindowKind::Weekly, WEEKLY_LABEL, w, WEEKLY_MINUTES));
    }
    for (key, value) in obj {
        let Some(scope) = key.strip_prefix("seven_day_") else {
            continue;
        };
        if let Some(w) = legacy_window(value) {
            let label = format!("{WEEKLY_LABEL} · {}", title_case(scope));
            windows.push(window(WindowKind::WeeklyScoped, &label, w, WEEKLY_MINUTES));
        }
    }
    Ok(windows)
}

/// (used_pct, resets_at) from an old-shape window object; `None` for null /
/// missing utilization.
fn legacy_window(v: &Value) -> Option<(f64, Option<DateTime<Utc>>)> {
    let o = v.as_object()?;
    let pct = number(o.get("utilization"))?;
    Some((pct, o.get("resets_at").and_then(parse_time)))
}

fn window(
    kind: WindowKind,
    label: &str,
    (used_pct, resets_at): (f64, Option<DateTime<Utc>>),
    minutes: u32,
) -> UsageWindow {
    UsageWindow {
        kind,
        label: label.to_string(),
        used_pct,
        resets_at,
        window_minutes: Some(minutes),
    }
}

fn parse_limit_entry(v: &Value) -> Option<UsageWindow> {
    let o = v.as_object()?;
    let pct = number(o.get("percent")).or_else(|| number(o.get("utilization")))?;
    let resets_at = o.get("resets_at").and_then(parse_time);
    let kind = o.get("kind").and_then(Value::as_str).unwrap_or("unknown");
    let (wk, label, minutes) = match kind {
        "session" => (
            WindowKind::Session,
            SESSION_LABEL.to_string(),
            Some(SESSION_MINUTES),
        ),
        "weekly_all" | "weekly" => (
            WindowKind::Weekly,
            WEEKLY_LABEL.to_string(),
            Some(WEEKLY_MINUTES),
        ),
        "weekly_scoped" => {
            let name = o
                .get("scope")
                .and_then(|s| s.get("model"))
                .and_then(|m| m.get("display_name"))
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("scoped");
            (
                WindowKind::WeeklyScoped,
                format!("{WEEKLY_LABEL} · {name}"),
                Some(WEEKLY_MINUTES),
            )
        }
        other => (WindowKind::Other, other.to_string(), None),
    };
    Some(UsageWindow {
        kind: wk,
        label,
        used_pct: pct,
        resets_at,
        window_minutes: minutes,
    })
}

/// "opus" -> "Opus", "oauth_apps" -> "Oauth Apps".
fn title_case(s: &str) -> String {
    s.split('_')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let mut c = p.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
