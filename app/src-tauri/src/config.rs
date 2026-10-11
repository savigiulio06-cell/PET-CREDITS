//! Our own settings, stored in `<app_home>/config.json`.
//!
//! Reading is tolerant (a broken or partial file never stops the app: every
//! field falls back to its default on its own); writing is atomic.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::variants;

/// Top-left corner of the compact window, in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub variant: String,
    pub use_api: bool,
    pub position: Option<Position>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            variant: variants::default_id().into(),
            use_api: true,
            position: None,
            lang: None,
        }
    }
}

impl Config {
    pub fn path(app_home: &Path) -> PathBuf {
        app_home.join("config.json")
    }

    /// Tolerant parse: unknown / malformed fields keep their default.
    pub fn from_json(text: &str) -> Self {
        let mut cfg = Self::default();
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
            return cfg;
        };
        if let Some(v) = map.get("variant").and_then(Value::as_str) {
            if variants::is_known(v) {
                cfg.variant = v.to_string();
            }
        }
        if let Some(b) = map.get("use_api").and_then(Value::as_bool) {
            cfg.use_api = b;
        }
        if let Some(p) = map.get("position") {
            cfg.position = serde_json::from_value(p.clone()).ok();
        }
        if let Some(l) = map.get("lang").and_then(Value::as_str) {
            // The value is only ever echoed back to the UI: keep it short and plain.
            if l.len() <= 16 && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                cfg.lang = Some(l.to_string());
            }
        }
        cfg
    }

    pub fn load(app_home: &Path) -> Self {
        fs::read_to_string(Self::path(app_home))
            .map(|s| Self::from_json(&s))
            .unwrap_or_default()
    }

    /// Atomic write: temp file in the same directory, then rename.
    pub fn save(&self, app_home: &Path) -> io::Result<()> {
        let path = Self::path(app_home);
        fs::create_dir_all(app_home)?;
        let mut text = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        text.push('\n');
        pet_core::fsutil::write_atomic(&path, text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_gives_defaults() {
        assert_eq!(Config::from_json("not json"), Config::default());
        assert_eq!(Config::from_json("[1,2]"), Config::default());
        assert_eq!(Config::from_json(""), Config::default());
    }

    #[test]
    fn fields_are_validated_independently() {
        let c = Config::from_json(
            r#"{"variant":"../evil","use_api":false,"position":{"x":10,"y":"no"},"lang":"it"}"#,
        );
        assert_eq!(c.variant, variants::default_id());
        assert!(!c.use_api);
        assert_eq!(c.position, None);
        assert_eq!(c.lang.as_deref(), Some("it"));
    }

    #[test]
    fn roundtrip_via_disk() {
        let dir = std::env::temp_dir().join(format!("pet-credits-cfg-{}", std::process::id()));
        let cfg = Config {
            variant: variants::VARIANTS.last().map_or("", |v| v.0).into(),
            use_api: false,
            position: Some(Position { x: -5, y: 40 }),
            lang: None,
        };
        cfg.save(&dir).unwrap();
        assert_eq!(Config::load(&dir), cfg);
        let _ = fs::remove_dir_all(&dir);
    }
}
