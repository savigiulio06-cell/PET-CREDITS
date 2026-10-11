//! Wire PET-CREDITS into Claude Code's `settings.json`.
//!
//! `install` adds two entries and nothing else:
//! - a `SessionStart` hook running `<exe> --ensure-running` (the pet appears
//!   whenever a Claude Code session starts);
//! - a `statusLine` running `<exe> --statusline`, which records the usage %
//!   Claude Code sends and then prints the user's previous status line, if any.
//!
//! The user's previous `statusLine` is kept in our own state file so `uninstall`
//! can put it back. Every write is preceded by a timestamped backup of
//! `settings.json` and done atomically. Both operations are idempotent and only
//! ever touch entries whose command contains [`MARKER_STATUSLINE`] or
//! [`MARKER_ENSURE`] together with our executable name.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{Error, Result};

pub const MARKER_STATUSLINE: &str = "--statusline";
pub const MARKER_ENSURE: &str = "--ensure-running";
const EXE_STEM: &str = "pet-credits";
const STATE_FILE: &str = "install-state.json";

/// What we remember between `install` and `uninstall`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InstallState {
    /// The `statusLine` value the user had before we replaced it (None = none).
    pub original_statusline: Option<Value>,
    pub installed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InstallReport {
    pub settings_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub changed: bool,
}

/// Build the command string Claude Code will run. Forward slashes and double
/// quotes work in cmd.exe, PowerShell and POSIX shells alike.
pub fn command_for(exe: &Path, flag: &str) -> String {
    let exe = exe.to_string_lossy().replace('\\', "/");
    format!("\"{exe}\" {flag}")
}

/// True only for commands we generate: `"<path to pet-credits[.exe]>" <flag>`.
/// A user's own command that merely mentions "pet-credits" is not ours.
fn is_ours(command: &str, flag: &str) -> bool {
    let Some(quoted) = command.trim().strip_suffix(flag).map(str::trim_end) else {
        return false;
    };
    let Some(path) = quoted.strip_prefix('"').and_then(|q| q.strip_suffix('"')) else {
        return false;
    };
    let name = path.rsplit('/').next().unwrap_or(path);
    name == EXE_STEM || name.eq_ignore_ascii_case(&format!("{EXE_STEM}.exe"))
}

/// Characters a shell could interpret inside double quotes (or that would end
/// them). Claude Code runs our commands through a shell, so an executable path
/// containing any of these could turn a folder name into a command.
const UNSAFE_PATH_CHARS: &[char] = &['"', '$', '`', '%', '!', '\n', '\r', '\0'];

/// Refuse to write a command into settings.json unless the executable lives in
/// a stable place that other users cannot replace.
pub fn validate_exe(exe: &Path) -> Result<()> {
    let s = exe.to_string_lossy();
    if !exe.is_absolute() {
        return Err(Error::Unavailable("executable path is not absolute".into()));
    }
    if s.contains(UNSAFE_PATH_CHARS) {
        return Err(Error::Unavailable(
            "the executable path contains characters a shell would interpret; move PET-CREDITS to a plain folder".into(),
        ));
    }
    let norm = |p: &Path| p.to_string_lossy().replace('\\', "/").to_ascii_lowercase();
    let lower = norm(exe);
    let temp = std::env::temp_dir();
    let temps = [
        norm(&temp),
        temp.canonicalize().map(|t| norm(&t)).unwrap_or_default(),
    ];
    let in_temp = temps
        .iter()
        .any(|t| !t.is_empty() && lower.starts_with(&format!("{}/", t.trim_end_matches('/'))));
    let volatile_prefix = ["/tmp/", "/var/tmp/", "/dev/shm/", "/volumes/"]
        .iter()
        .any(|v| lower.starts_with(v));
    let volatile_part = ["/.mount_", "/apptranslocation/"]
        .iter()
        .any(|v| lower.contains(v));
    if in_temp || volatile_prefix || volatile_part {
        return Err(Error::Unavailable(
            "PET-CREDITS is running from a temporary or removable location; install it first"
                .into(),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // The binary and every folder above it must not be writable by others.
        let mut p = Some(exe);
        while let Some(cur) = p {
            if let Ok(m) = fs::metadata(cur) {
                if m.permissions().mode() & 0o022 != 0 {
                    return Err(Error::Unavailable(format!(
                        "{} is writable by other users",
                        cur.display()
                    )));
                }
            }
            p = cur.parent();
        }
    }
    Ok(())
}

/// The executable to register: the AppImage file itself when running from one
/// (the binary inside lives on a temporary mount), otherwise `current`.
pub fn install_target(current: &Path) -> PathBuf {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute() && p.is_file())
        .unwrap_or_else(|| current.to_path_buf())
}

fn statusline_is_ours(v: &Value) -> bool {
    v.get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| is_ours(c, MARKER_STATUSLINE))
}

/// Pure: return the new settings and state for an install.
pub fn plan_install(
    settings: &Value,
    exe: &Path,
    previous: Option<&InstallState>,
) -> Result<(Value, InstallState)> {
    let mut root = match settings {
        Value::Object(m) => m.clone(),
        Value::Null => Map::new(),
        _ => return Err(Error::Format("settings.json is not a JSON object".into())),
    };

    // Status line: remember the user's own one, unless it is already ours.
    let current = root.get("statusLine").cloned();
    let original = match &current {
        Some(v) if statusline_is_ours(v) => previous.and_then(|s| s.original_statusline.clone()),
        other => other.clone(),
    };
    let mut statusline = json!({
        "type": "command",
        "command": command_for(exe, MARKER_STATUSLINE),
    });
    // Keep the user's padding so the line looks the same as before.
    if let Some(p) = original.as_ref().and_then(|o| o.get("padding")) {
        statusline["padding"] = p.clone();
    }
    root.insert("statusLine".into(), statusline);

    // SessionStart hook: drop any stale entry of ours, then add one.
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| Error::Format("`hooks` is not an object".into()))?;
    let groups = hooks
        .entry("SessionStart")
        .or_insert_with(|| Value::Array(Vec::new()));
    let groups = groups
        .as_array_mut()
        .ok_or_else(|| Error::Format("`hooks.SessionStart` is not an array".into()))?;
    remove_our_hooks(groups, MARKER_ENSURE);
    groups.push(json!({
        "hooks": [{ "type": "command", "command": command_for(exe, MARKER_ENSURE) }]
    }));

    let state = InstallState {
        original_statusline: original,
        installed_at: Some(Utc::now()),
    };
    Ok((Value::Object(root), state))
}

/// Pure: return the settings with our entries removed and the user's
/// original status line restored.
pub fn plan_uninstall(settings: &Value, state: Option<&InstallState>) -> Result<Value> {
    let mut root = match settings {
        Value::Object(m) => m.clone(),
        Value::Null => return Ok(Value::Null),
        _ => return Err(Error::Format("settings.json is not a JSON object".into())),
    };

    if root.get("statusLine").is_some_and(statusline_is_ours) {
        match state.and_then(|s| s.original_statusline.clone()) {
            Some(orig) => root.insert("statusLine".into(), orig),
            None => root.shift_remove("statusLine"),
        };
    }

    let mut drop_hooks = false;
    if let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) {
        let mut drop_event = false;
        if let Some(groups) = hooks.get_mut("SessionStart").and_then(Value::as_array_mut) {
            remove_our_hooks(groups, MARKER_ENSURE);
            drop_event = groups.is_empty();
        }
        if drop_event {
            hooks.shift_remove("SessionStart");
        }
        drop_hooks = hooks.is_empty();
    }
    if drop_hooks {
        root.shift_remove("hooks");
    }
    Ok(Value::Object(root))
}

/// Remove hook commands of ours from a list of matcher groups, dropping groups
/// left empty. Other users' hooks are never touched.
fn remove_our_hooks(groups: &mut Vec<Value>, flag: &str) {
    for group in groups.iter_mut() {
        if let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) {
            list.retain(|h| {
                !h.get("command")
                    .and_then(Value::as_str)
                    .is_some_and(|c| is_ours(c, flag))
            });
        }
    }
    groups.retain(|g| {
        g.get("hooks")
            .and_then(Value::as_array)
            .is_none_or(|l| !l.is_empty())
    });
}

fn settings_path(claude_home: &Path) -> PathBuf {
    claude_home.join("settings.json")
}

fn read_json(path: &Path) -> Result<Option<Value>> {
    match fs::read_to_string(path) {
        Ok(s) if s.trim().is_empty() => Ok(None),
        Ok(s) => Ok(Some(serde_json::from_str(&s)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    Ok(crate::fsutil::write_atomic(path, contents)?)
}

fn backup(path: &Path) -> Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let stamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    // Never overwrite an earlier backup, even within the same second.
    let mut backup = path.with_file_name(format!("settings.json.pet-credits-backup-{stamp}"));
    let mut n = 1;
    while backup.exists() {
        backup = path.with_file_name(format!("settings.json.pet-credits-backup-{stamp}-{n}"));
        n += 1;
    }
    fs::copy(path, &backup)?;
    Ok(Some(backup))
}

fn write_settings(path: &Path, old: &Value, new: &Value) -> Result<InstallReport> {
    if old == new {
        return Ok(InstallReport {
            settings_path: path.to_path_buf(),
            backup_path: None,
            changed: false,
        });
    }
    let backup_path = backup(path)?;
    let mut text = serde_json::to_string_pretty(new)?;
    text.push('\n');
    write_atomic(path, text.as_bytes())?;
    Ok(InstallReport {
        settings_path: path.to_path_buf(),
        backup_path,
        changed: true,
    })
}

pub fn load_state(app_home: &Path) -> Result<Option<InstallState>> {
    Ok(read_json(&app_home.join(STATE_FILE))?
        .map(serde_json::from_value)
        .transpose()?)
}

/// Install into `<claude_home>/settings.json`. `exe` is the absolute path of
/// the pet-credits executable.
pub fn install(claude_home: &Path, app_home: &Path, exe: &Path) -> Result<InstallReport> {
    validate_exe(exe)?;
    install_unchecked(claude_home, app_home, exe)
}

fn install_unchecked(claude_home: &Path, app_home: &Path, exe: &Path) -> Result<InstallReport> {
    let path = settings_path(claude_home);
    let old = read_json(&path)?.unwrap_or(Value::Null);
    let previous = load_state(app_home)?;
    let (new, state) = plan_install(&old, exe, previous.as_ref())?;
    // State first: if the settings write fails we still know the original.
    let mut state_text = serde_json::to_string_pretty(&state)?;
    state_text.push('\n');
    write_atomic(&app_home.join(STATE_FILE), state_text.as_bytes())?;
    write_settings(&path, &old, &new)
}

/// Undo [`install`]. Leaves every other setting as it is now.
pub fn uninstall(claude_home: &Path, app_home: &Path) -> Result<InstallReport> {
    let path = settings_path(claude_home);
    let Some(old) = read_json(&path)? else {
        return Ok(InstallReport {
            settings_path: path,
            backup_path: None,
            changed: false,
        });
    };
    let state = load_state(app_home)?;
    let new = plan_uninstall(&old, state.as_ref())?;
    let report = write_settings(&path, &old, &new)?;
    let _ = fs::remove_file(app_home.join(STATE_FILE));
    Ok(report)
}

/// The user's previous status-line command, to be chained after ours.
pub fn original_statusline_command(app_home: &Path) -> Option<String> {
    let state = load_state(app_home).ok()??;
    let orig = state.original_statusline?;
    let cmd = orig.get("command")?.as_str()?.trim().to_string();
    // Never chain to ourselves (would recurse).
    (!cmd.is_empty() && !is_ours(&cmd, MARKER_STATUSLINE)).then_some(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exe() -> PathBuf {
        PathBuf::from(r"C:\Program Files\PET-CREDITS\pet-credits.exe")
    }

    fn user_settings() -> Value {
        json!({
            "model": "opus",
            "statusLine": { "type": "command", "command": "~/.claude/my-line.sh", "padding": 1 },
            "hooks": {
                "SessionStart": [
                    { "matcher": "startup", "hooks": [{ "type": "command", "command": "echo hi" }] }
                ],
                "Stop": [ { "hooks": [{ "type": "command", "command": "notify" }] } ]
            }
        })
    }

    #[test]
    fn only_our_exact_commands_are_ours() {
        assert!(is_ours(
            "\"/opt/x/pet-credits\" --statusline",
            MARKER_STATUSLINE
        ));
        assert!(is_ours(
            "\"C:/A B/PET-CREDITS.exe\" --statusline",
            MARKER_STATUSLINE
        ));
        assert!(!is_ours(
            "my-line.sh --pet-credits --statusline",
            MARKER_STATUSLINE
        ));
        assert!(!is_ours(
            "\"/opt/x/pet-credits-fork\" --statusline",
            MARKER_STATUSLINE
        ));
        assert!(!is_ours(
            "\"/opt/x/pet-credits\" --ensure-running",
            MARKER_STATUSLINE
        ));
    }

    #[test]
    fn rejects_unsafe_executables() {
        for bad in [
            "/home/u/x$(id)/pet-credits",
            "/home/u/a`b`/pet-credits",
            "/home/u/a\"b/pet-credits",
            "/tmp/.mount_abc/usr/bin/pet-credits",
            "/var/tmp/pet-credits",
            "/Volumes/PET-CREDITS/PET-CREDITS.app/Contents/MacOS/pet-credits",
            "relative/pet-credits",
        ] {
            assert!(validate_exe(Path::new(bad)).is_err(), "{bad}");
        }
        let tmp = std::env::temp_dir().join("pet-credits");
        assert!(validate_exe(&tmp).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_group_writable_folder() {
        use std::os::unix::fs::PermissionsExt;
        // Uses the home dir (not /tmp, which is rejected anyway).
        let base = dirs::home_dir().unwrap().join(".pet-credits-test-perm");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let exe = base.join("pet-credits");
        fs::write(&exe, "").unwrap();
        fs::set_permissions(&base, fs::Permissions::from_mode(0o775)).unwrap();
        let home_ok = validate_exe(&dirs::home_dir().unwrap().join("x")).is_ok();
        let r = validate_exe(&exe);
        fs::remove_dir_all(&base).unwrap();
        if home_ok {
            assert!(r.is_err());
        }
    }

    #[test]
    fn command_quotes_and_uses_forward_slashes() {
        assert_eq!(
            command_for(&exe(), MARKER_STATUSLINE),
            "\"C:/Program Files/PET-CREDITS/pet-credits.exe\" --statusline"
        );
    }

    #[test]
    fn install_keeps_user_settings_and_remembers_statusline() {
        let (new, state) = plan_install(&user_settings(), &exe(), None).unwrap();
        assert_eq!(new["model"], "opus");
        assert!(statusline_is_ours(&new["statusLine"]));
        assert_eq!(new["statusLine"]["padding"], 1);
        assert_eq!(
            state.original_statusline.unwrap()["command"],
            "~/.claude/my-line.sh"
        );
        let ss = new["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(ss.len(), 2);
        assert_eq!(ss[0]["hooks"][0]["command"], "echo hi");
        assert_eq!(new["hooks"]["Stop"], user_settings()["hooks"]["Stop"]);
    }

    #[test]
    fn install_is_idempotent() {
        let (once, s1) = plan_install(&user_settings(), &exe(), None).unwrap();
        let (twice, s2) = plan_install(&once, &exe(), Some(&s1)).unwrap();
        assert_eq!(once, twice);
        assert_eq!(s1.original_statusline, s2.original_statusline);
    }

    #[test]
    fn install_on_empty_settings() {
        let (new, state) = plan_install(&Value::Null, &exe(), None).unwrap();
        assert!(state.original_statusline.is_none());
        assert_eq!(new["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn uninstall_restores_exactly() {
        let orig = user_settings();
        let (installed, state) = plan_install(&orig, &exe(), None).unwrap();
        assert_eq!(plan_uninstall(&installed, Some(&state)).unwrap(), orig);

        let (installed, state) = plan_install(&json!({"x": 1}), &exe(), None).unwrap();
        assert_eq!(
            plan_uninstall(&installed, Some(&state)).unwrap(),
            json!({"x": 1})
        );
    }

    #[test]
    fn uninstall_keeps_the_order_of_the_other_keys() {
        // `remove` on an ordered map would move the last key into the hole.
        let (mut installed, state) = plan_install(&json!({"a": 1, "b": 2}), &exe(), None).unwrap();
        installed["c"] = json!(3);
        installed["d"] = json!(4);
        let out = plan_uninstall(&installed, Some(&state)).unwrap();
        assert_eq!(
            serde_json::to_string(&out).unwrap(),
            r#"{"a":1,"b":2,"c":3,"d":4}"#
        );
    }

    #[test]
    fn uninstall_leaves_foreign_statusline_alone() {
        let s = json!({ "statusLine": { "type": "command", "command": "other" } });
        assert_eq!(plan_uninstall(&s, None).unwrap(), s);
    }

    #[test]
    fn rejects_non_object_settings() {
        assert!(plan_install(&json!([1, 2]), &exe(), None).is_err());
        assert!(plan_install(&json!({"hooks": 3}), &exe(), None).is_err());
    }

    #[test]
    fn install_uninstall_on_disk() {
        let claude = tempfile::tempdir().unwrap();
        let app = tempfile::tempdir().unwrap();
        let path = claude.path().join("settings.json");
        let orig = serde_json::to_string_pretty(&user_settings()).unwrap();
        fs::write(&path, &orig).unwrap();

        // An unsafe executable is refused before anything is written.
        assert!(install(
            claude.path(),
            app.path(),
            Path::new("/tmp/x$(id)/pet-credits")
        )
        .is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), orig);

        let r = install_unchecked(claude.path(), app.path(), &exe()).unwrap();
        assert!(r.changed);
        assert_eq!(fs::read_to_string(r.backup_path.unwrap()).unwrap(), orig);
        assert_eq!(
            original_statusline_command(app.path()).as_deref(),
            Some("~/.claude/my-line.sh")
        );
        // Second install changes nothing and makes no new backup.
        let r2 = install_unchecked(claude.path(), app.path(), &exe()).unwrap();
        assert!(!r2.changed && r2.backup_path.is_none());

        uninstall(claude.path(), app.path()).unwrap();
        let back: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back, user_settings());
        assert!(original_statusline_command(app.path()).is_none());
    }
}
