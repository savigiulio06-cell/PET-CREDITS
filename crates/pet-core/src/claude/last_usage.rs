//! Persistence of the last successful usage-API response, so the pet can keep
//! showing the last known numbers (marked stale) after a restart or when the
//! login has expired.
//!
//! The file holds ONLY window kind, label, percentages, reset times, window
//! length and a timestamp: never credentials or anything derived from them.

use std::io::Read;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Result, UsageWindow};

/// Format version written by this build; other versions are ignored on load.
const VERSION: u32 = 1;
/// A real file is well under 2 KB; anything bigger is not ours.
const MAX_BYTES: u64 = 64 * 1024;

#[derive(Serialize, Deserialize)]
struct LastUsage {
    version: u32,
    fetched_at: DateTime<Utc>,
    windows: Vec<UsageWindow>,
}

/// Atomically save `windows` (owner-only permissions via `fsutil`).
pub fn save(path: &Path, windows: &[UsageWindow], fetched_at: DateTime<Utc>) -> Result<()> {
    let json = serde_json::to_vec_pretty(&LastUsage {
        version: VERSION,
        fetched_at,
        windows: windows.to_vec(),
    })?;
    crate::fsutil::write_atomic(path, &json)?;
    Ok(())
}

/// Load the saved windows. Missing, oversized, corrupt, empty or future-version
/// files all yield `None`; this never panics.
pub fn load(path: &Path) -> Option<(Vec<UsageWindow>, DateTime<Utc>)> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    let data: LastUsage = serde_json::from_slice(&bytes).ok()?;
    if data.version != VERSION {
        return None;
    }
    let windows: Vec<UsageWindow> = data
        .windows
        .into_iter()
        .filter(|w| w.used_pct.is_finite())
        .collect();
    if windows.is_empty() {
        return None;
    }
    // A clock jump must not make old numbers look fresh.
    Some((windows, data.fetched_at.min(Utc::now())))
}
