//! `pet-credits --statusline`: the command Claude Code runs for its status line.
//!
//! 1. Records the usage numbers from Claude Code's stdin JSON into the bridge
//!    file (so the pet updates on every API response while Claude works).
//! 2. Prints the user's previous status line (chained, same stdin), or a short
//!    default line if they had none.
//!
//! The chained command is the one the user themselves had configured in
//! Claude Code; it is run through the shell (`sh` on macOS/Linux; on Windows
//! Git Bash if `CLAUDE_CODE_GIT_BASH_PATH` is set, otherwise `cmd`).

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::{bridge, install};
use crate::WindowKind;

const CHAIN_TIMEOUT: Duration = Duration::from_secs(5);
/// Set on the chained command. If the user's previous status line calls
/// `pet-credits --statusline` again (directly or through a script), that
/// nested run sees it and does not chain once more: no endless recursion.
pub const CHAINED_ENV: &str = "PET_CREDITS_CHAINED";

/// Handle one status-line invocation and return what should be printed.
pub fn run(stdin_json: &str, bridge_path: &Path, app_home: &Path) -> String {
    let nested = std::env::var_os(CHAINED_ENV).is_some();
    run_with(stdin_json, bridge_path, app_home, nested)
}

/// [`run`] with the recursion guard made explicit (`nested` = we are already
/// running inside a chained status line).
pub fn run_with(stdin_json: &str, bridge_path: &Path, app_home: &Path, nested: bool) -> String {
    let data = bridge::from_statusline_input(stdin_json).ok();
    if let Some(d) = &data {
        // Best effort: a broken bridge must never break Claude's status line.
        let _ = bridge::write(bridge_path, d);
    }
    if nested {
        return default_line(data.as_ref());
    }
    if let Some(cmd) = install::original_statusline_command(app_home) {
        if let Some(out) = run_chained(&cmd, stdin_json) {
            return out;
        }
    }
    default_line(data.as_ref())
}

fn default_line(data: Option<&bridge::BridgeData>) -> String {
    let Some(d) = data else {
        return String::new();
    };
    let pct = |k| {
        d.windows
            .iter()
            .find(|w| w.kind == k)
            .map(|w| format!("{:.0}%", w.used_pct))
    };
    let mut parts = Vec::new();
    if let Some(m) = &d.model {
        parts.push(m.clone());
    }
    if let Some(p) = pct(WindowKind::Session) {
        parts.push(format!("5h {p}"));
    }
    if let Some(p) = pct(WindowKind::Weekly) {
        parts.push(format!("7d {p}"));
    }
    parts.join(" · ")
}

fn shell_command(cmd: &str) -> Command {
    #[cfg(windows)]
    {
        // Claude Code on Windows runs commands through Git Bash when present.
        let bash = std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH")
            .map(std::path::PathBuf::from)
            .filter(|p| p.exists());
        match bash {
            Some(b) => {
                let mut c = Command::new(b);
                c.args(["-c", cmd]);
                c
            }
            None => {
                let mut c = Command::new("cmd");
                c.args(["/C", cmd]);
                c
            }
        }
    }
    #[cfg(not(windows))]
    {
        let mut c = Command::new("sh");
        c.args(["-c", cmd]);
        c
    }
}

fn run_chained(cmd: &str, stdin_json: &str) -> Option<String> {
    let mut child = shell_command(cmd)
        .env(CHAINED_ENV, "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(stdin_json.as_bytes());
    }
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < CHAIN_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(20))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    reader.join().ok()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::claude::install::InstallState;
    use serde_json::json;

    const INPUT: &str = r#"{"session_id":"s1","model":{"display_name":"Opus"},
        "rate_limits":{"five_hour":{"used_percentage":42.4,"resets_at":1760202000},
                       "seven_day":{"used_percentage":63,"resets_at":1760600000}}}"#;

    #[test]
    fn writes_bridge_and_prints_default_line() {
        let dir = tempfile::tempdir().unwrap();
        let bridge_path = dir.path().join("bridge.json");
        let out = run(INPUT, &bridge_path, dir.path());
        assert_eq!(out, "Opus · 5h 42% · 7d 63%");
        assert!(bridge::read(&bridge_path).unwrap().is_some());
    }

    #[test]
    fn chains_users_previous_statusline_with_same_stdin() {
        let dir = tempfile::tempdir().unwrap();
        let state = InstallState {
            original_statusline: Some(json!({"type": "command", "command": "cat"})),
            installed_at: None,
        };
        std::fs::write(
            dir.path().join("install-state.json"),
            serde_json::to_string(&state).unwrap(),
        )
        .unwrap();
        let out = run(INPUT, &dir.path().join("b.json"), dir.path());
        assert_eq!(out, INPUT);
    }

    #[test]
    fn garbage_input_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(run("not json", &dir.path().join("b.json"), dir.path()), "");
    }
}

#[cfg(test)]
mod recursion_tests {
    use super::*;
    use crate::claude::install::InstallState;
    use serde_json::json;

    fn with_previous(dir: &Path, command: &str) {
        let state = InstallState {
            original_statusline: Some(json!({"type": "command", "command": command})),
            installed_at: None,
        };
        std::fs::write(
            dir.join("install-state.json"),
            serde_json::to_string(&state).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn nested_run_never_chains_again() {
        let dir = tempfile::tempdir().unwrap();
        // `echo` works in sh, Git Bash and cmd alike.
        with_previous(dir.path(), "echo CHAINED");
        let b = dir.path().join("b.json");
        assert!(run_with("{}", &b, dir.path(), false).contains("CHAINED"));
        assert!(!run_with("{}", &b, dir.path(), true).contains("CHAINED"));
    }

    #[test]
    fn chained_command_sees_the_guard_variable() {
        let dir = tempfile::tempdir().unwrap();
        let probe = if cfg!(windows) && std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH").is_none() {
            "echo %PET_CREDITS_CHAINED%"
        } else {
            "echo $PET_CREDITS_CHAINED"
        };
        with_previous(dir.path(), probe);
        let out = run_with("{}", &dir.path().join("b.json"), dir.path(), false);
        assert_eq!(out.trim(), "1");
    }
}
