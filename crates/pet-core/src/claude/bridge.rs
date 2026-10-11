//! Bridge from the Claude Code status line.
//!
//! Claude Code runs a user-configured status-line command on every update and
//! pipes a JSON document to its stdin. Our tiny status-line command turns that
//! into a [`BridgeData`] file that the app reads. The input format is not
//! documented and changes, so everything here is tolerant.

use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::oauth::{parse_time, SESSION_LABEL, SESSION_MINUTES, WEEKLY_LABEL, WEEKLY_MINUTES};
use crate::{Error, Result, UsageWindow, WindowKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BridgeData {
    /// When this file was last written.
    pub written_at: DateTime<Utc>,
    pub session_id: Option<String>,
    pub transcript_path: Option<String>,
    pub model: Option<String>,
    pub session_cost_usd: Option<f64>,
    pub windows: Vec<UsageWindow>,
    /// When `windows` were last refreshed from a status-line input that carried
    /// rate limits. Differs from `written_at` when old windows were preserved.
    #[serde(default)]
    pub windows_at: Option<DateTime<Utc>>,
}

/// Build a [`BridgeData`] from the status-line stdin JSON, stamped "now".
pub fn from_statusline_input(stdin_json: &str) -> Result<BridgeData> {
    from_statusline_input_at(stdin_json, Utc::now())
}

/// Same as [`from_statusline_input`] with an explicit clock (for tests).
pub fn from_statusline_input_at(stdin_json: &str, now: DateTime<Utc>) -> Result<BridgeData> {
    let root: Value = serde_json::from_str(stdin_json)
        .map_err(|_| Error::Format("status-line input is not valid JSON".into()))?;
    let obj = root
        .as_object()
        .ok_or_else(|| Error::Format("status-line input is not a JSON object".into()))?;

    let text = |key: &str| {
        obj.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };

    let model = match obj.get("model") {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(Value::Object(m)) => ["display_name", "id"]
            .iter()
            .find_map(|k| m.get(*k).and_then(Value::as_str).filter(|s| !s.is_empty()))
            .map(str::to_string),
        _ => None,
    };

    let session_cost_usd = obj
        .get("cost")
        .and_then(|c| c.get("total_cost_usd"))
        .and_then(Value::as_f64)
        .filter(|c| c.is_finite());

    let mut windows = Vec::new();
    if let Some(rl) = obj.get("rate_limits") {
        if let Some(w) = rate_window(
            rl.get("five_hour"),
            WindowKind::Session,
            SESSION_LABEL,
            SESSION_MINUTES,
        ) {
            windows.push(w);
        }
        if let Some(w) = rate_window(
            rl.get("seven_day"),
            WindowKind::Weekly,
            WEEKLY_LABEL,
            WEEKLY_MINUTES,
        ) {
            windows.push(w);
        }
    }
    let windows_at = (!windows.is_empty()).then_some(now);

    Ok(BridgeData {
        written_at: now,
        session_id: text("session_id"),
        transcript_path: text("transcript_path"),
        model,
        session_cost_usd,
        windows,
        windows_at,
    })
}

fn rate_window(
    v: Option<&Value>,
    kind: WindowKind,
    label: &str,
    minutes: u32,
) -> Option<UsageWindow> {
    let o = v?.as_object()?;
    let used_pct = o
        .get("used_percentage")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())?;
    Some(UsageWindow {
        kind,
        label: label.to_string(),
        used_pct,
        resets_at: o.get("resets_at").and_then(parse_time),
        window_minutes: Some(minutes),
    })
}

/// Atomically write `data` to `path` (temp file in the same directory, then
/// rename). If `data.windows` is empty and the existing file has windows, the
/// previous windows (and their timestamp) are kept so an input without
/// `rate_limits` never erases the last good reading.
pub fn write(path: &Path, data: &BridgeData) -> Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;

    let mut merged = data.clone();
    if merged.windows.is_empty() {
        if let Ok(Some(old)) = read(path) {
            if !old.windows.is_empty() {
                merged.windows = old.windows;
                merged.windows_at = old.windows_at.or(Some(old.written_at));
            }
        }
    }

    let json = serde_json::to_vec_pretty(&merged)?;
    crate::fsutil::write_atomic(path, &json)?;
    Ok(())
}

/// Read the bridge file; `Ok(None)` if it does not exist.
pub fn read(path: &Path) -> Result<Option<BridgeData>> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| Error::Format("bridge file is malformed".into())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::Io(e)),
    }
}
