use std::fs;
use std::path::Path;

use chrono::{Duration, Utc};
use pet_core::claude::bridge::{from_statusline_input_at, write};
use pet_core::claude::last_usage;
use pet_core::claude::provider::{ClaudeConfig, ClaudeProvider};
use pet_core::{Issue, Source, UsageProvider, UsageWindow, WindowKind};

fn fixture_path(rel: &str) -> String {
    format!("{}/tests/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn setup(home: &Path) -> ClaudeConfig {
    let project = home.join("projects/demo-project");
    fs::create_dir_all(&project).unwrap();
    fs::copy(
        fixture_path("transcripts/session-1.jsonl"),
        project.join("session-1.jsonl"),
    )
    .unwrap();
    ClaudeConfig {
        use_api: false,
        claude_home: home.to_path_buf(),
        bridge_path: home.join("pet/bridge.json"),
        last_usage_path: home.join("pet/last-usage.json"),
    }
}

fn bridge_input(home: &Path, with_limits: bool) -> String {
    let name = if with_limits {
        "statusline_full.json"
    } else {
        "statusline_no_limits.json"
    };
    let text = fs::read_to_string(fixture_path(name)).unwrap();
    // Point the transcript at the temp home instead of the fake path.
    text.replace(
        "/fake/home/.claude/projects/demo/session-1.jsonl",
        &home
            .join("projects/demo-project/session-1.jsonl")
            .to_string_lossy()
            .replace('\\', "\\\\"),
    )
}

#[test]
fn without_any_source_degrades_gracefully() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = ClaudeConfig {
        use_api: false,
        claude_home: dir.path().join("empty"),
        bridge_path: dir.path().join("none.json"),
        last_usage_path: dir.path().join("none-usage.json"),
    };
    let mut p = ClaudeProvider::new(cfg);
    let snap = p.poll().unwrap();
    assert_eq!(snap.provider, "claude");
    assert_eq!(snap.source, Source::None);
    assert!(snap.windows.is_empty() && snap.stale && snap.session.is_none());
    assert!(snap.note.is_some());
    assert!(!p.is_active());
}

#[test]
fn bridge_windows_and_session_from_bridge_transcript() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = setup(dir.path());
    let data = from_statusline_input_at(&bridge_input(dir.path(), true), Utc::now()).unwrap();
    write(&cfg.bridge_path, &data).unwrap();

    let mut p = ClaudeProvider::new(cfg);
    let snap = p.poll().unwrap();
    assert_eq!(snap.source, Source::Statusline);
    assert!(!snap.stale);
    assert_eq!(snap.window(WindowKind::Session).unwrap().used_pct, 23.5);
    assert_eq!(snap.session.as_ref().unwrap().session_id, "session-1");
    assert!(snap.note.is_none());
    assert!(p.is_active(), "bridge was just written");
}

#[test]
fn old_bridge_windows_are_stale() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = setup(dir.path());
    let old = Utc::now() - Duration::minutes(30);
    let data = from_statusline_input_at(&bridge_input(dir.path(), true), old).unwrap();
    write(&cfg.bridge_path, &data).unwrap();
    // A newer input without limits keeps the windows but not their freshness.
    let newer = from_statusline_input_at(&bridge_input(dir.path(), false), Utc::now()).unwrap();
    write(&cfg.bridge_path, &newer).unwrap();

    let snap = ClaudeProvider::new(cfg).poll().unwrap();
    assert_eq!(snap.source, Source::Statusline);
    assert_eq!(snap.windows.len(), 2);
    assert!(snap.stale);
}

#[test]
fn falls_back_to_latest_session_without_bridge() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = setup(dir.path());
    let snap = ClaudeProvider::new(cfg).poll().unwrap();
    assert_eq!(snap.source, Source::None);
    assert_eq!(snap.session.as_ref().unwrap().session_id, "session-1");
    assert_eq!(
        snap.session.unwrap().project.as_deref(),
        Some("demo-project")
    );
}

#[test]
fn bridge_transcript_outside_projects_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = setup(dir.path());
    let outside = dir.path().join("outside.jsonl");
    fs::write(&outside, "").unwrap();
    let input = format!(
        r#"{{"transcript_path":{}}}"#,
        serde_json::to_string(&outside.to_string_lossy()).unwrap()
    );
    write(
        &cfg.bridge_path,
        &from_statusline_input_at(&input, Utc::now()).unwrap(),
    )
    .unwrap();
    let snap = ClaudeProvider::new(cfg).poll().unwrap();
    // Falls back to the latest session inside projects/.
    assert_eq!(snap.session.unwrap().session_id, "session-1");
}

// ---- login problems and the persisted last-usage cache ----

fn api_config(dir: &Path) -> ClaudeConfig {
    ClaudeConfig {
        use_api: true,
        claude_home: dir.join("claude"),
        bridge_path: dir.join("pet/bridge.json"),
        last_usage_path: dir.join("pet/last-usage.json"),
    }
}

fn cached_window(pct: f64) -> UsageWindow {
    UsageWindow {
        kind: WindowKind::Session,
        label: "Session (5h)".into(),
        used_pct: pct,
        resets_at: None,
        window_minutes: Some(300),
    }
}

/// Fake credentials whose expiry is in the past: `fetch_usage` rejects them
/// locally, so no network request is made.
fn write_expired_credentials(claude_home: &Path) {
    fs::create_dir_all(claude_home).unwrap();
    fs::write(
        claude_home.join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"fake-test-token","refreshToken":"fake-refresh","expiresAt":1000000000000}}"#,
    )
    .unwrap();
}

#[test]
fn expired_token_reports_auth_expired_and_keeps_cached_numbers_stale() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = api_config(dir.path());
    write_expired_credentials(&cfg.claude_home);
    // Fresh timestamp: only the issue (not age) can make the data stale.
    last_usage::save(&cfg.last_usage_path, &[cached_window(42.0)], Utc::now()).unwrap();

    let mut p = ClaudeProvider::new(cfg);
    let snap = p.poll().unwrap();
    assert_eq!(snap.issue, Some(Issue::AuthExpired));
    assert!(snap.stale);
    assert_eq!(snap.source, Source::Api);
    assert_eq!(snap.windows.len(), 1);
    assert_eq!(snap.windows[0].used_pct, 42.0);
    assert!(!snap.note.unwrap_or_default().contains("fake-test-token"));
    // Local refreshes keep the issue.
    assert_eq!(p.poll_local().unwrap().issue, Some(Issue::AuthExpired));
}

#[test]
fn fresh_status_line_hides_the_login_problem() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = setup(dir.path());
    cfg.use_api = true;
    write_expired_credentials(&cfg.claude_home);
    let data = from_statusline_input_at(&bridge_input(dir.path(), true), Utc::now()).unwrap();
    write(&cfg.bridge_path, &data).unwrap();

    let snap = ClaudeProvider::new(cfg).poll().unwrap();
    assert_eq!(snap.source, Source::Statusline);
    assert!(!snap.stale);
    assert_eq!(snap.issue, None);
}

#[test]
fn missing_credentials_report_login_missing() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = api_config(dir.path());
    fs::create_dir_all(&cfg.claude_home).unwrap();
    let snap = ClaudeProvider::new(cfg).poll().unwrap();
    assert_eq!(snap.issue, Some(Issue::LoginMissing));
    assert!(snap.windows.is_empty() && snap.stale);
}

#[test]
fn no_issue_when_the_api_is_off() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = api_config(dir.path());
    cfg.use_api = false;
    assert_eq!(ClaudeProvider::new(cfg).poll().unwrap().issue, None);
}

#[test]
fn last_usage_cache_is_reloaded_by_a_new_provider() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = api_config(dir.path());
    let at = Utc::now() - Duration::hours(2);
    last_usage::save(&cfg.last_usage_path, &[cached_window(64.0)], at).unwrap();

    let text = fs::read_to_string(&cfg.last_usage_path).unwrap();
    assert!(!text.to_lowercase().contains("token"));

    let mut p = ClaudeProvider::new(cfg.clone());
    let snap = p.poll_local().unwrap();
    assert_eq!(snap.source, Source::Api);
    assert_eq!(snap.windows[0].used_pct, 64.0);
    assert!(snap.stale, "two hours old");
    assert_eq!(snap.issue, None, "no API attempt yet");

    // Switching the API off does not delete the file.
    p.set_use_api(false);
    assert!(cfg.last_usage_path.exists());
}

#[test]
fn corrupt_or_foreign_cache_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = api_config(dir.path());
    fs::create_dir_all(cfg.last_usage_path.parent().unwrap()).unwrap();
    let cases = [
        "not json at all".to_string(),
        r#"{"version":99,"fetched_at":"2026-01-01T00:00:00Z","windows":[]}"#.to_string(),
        r#"{"version":1,"windows":3}"#.to_string(),
        " ".repeat(200_000),
    ];
    for c in cases {
        fs::write(&cfg.last_usage_path, c).unwrap();
        let snap = ClaudeProvider::new(cfg.clone()).poll_local().unwrap();
        assert!(snap.windows.is_empty());
        assert_eq!(snap.source, Source::None);
    }
}
