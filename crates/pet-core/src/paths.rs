//! Well-known locations. Every function honours an override env var so tests
//! and unusual setups never touch the real home directory.

use std::path::PathBuf;

fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Claude Code config dir: `$CLAUDE_CONFIG_DIR` or `~/.claude`.
pub fn claude_home() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".claude"))
}

/// Codex home: `$CODEX_HOME` or `~/.codex`.
pub fn codex_home() -> PathBuf {
    std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".codex"))
}

/// Our own data dir: `<OS config dir>/pet-credits`
/// (`%APPDATA%\pet-credits` on Windows, `~/.config/pet-credits` on Linux,
/// `~/Library/Application Support/pet-credits` on macOS).
///
/// `$PET_CREDITS_HOME` overrides it in debug builds only: release builds ignore
/// it so that environment set by an untrusted project cannot redirect the file
/// from which `--statusline` reads the command it chains to.
pub fn app_home() -> PathBuf {
    let overridden = if cfg!(debug_assertions) {
        std::env::var_os("PET_CREDITS_HOME")
    } else {
        None
    };
    overridden.map(PathBuf::from).unwrap_or_else(|| {
        dirs::config_dir()
            .unwrap_or_else(|| home().join(".config"))
            .join("pet-credits")
    })
}

/// File the status-line bridge writes and the app reads.
pub fn bridge_file() -> PathBuf {
    app_home().join("statusline-bridge.json")
}

/// Last usage numbers fetched from the API, kept so they survive a restart.
pub fn last_usage_file() -> PathBuf {
    app_home().join("last-usage.json")
}
