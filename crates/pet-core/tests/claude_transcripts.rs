use std::fs::{self, File};
use std::path::Path;
use std::time::{Duration, SystemTime};

use chrono::{TimeZone, Utc};
use pet_core::claude::transcripts::{find_latest_session, summarize};

fn fixture_dir() -> String {
    format!("{}/tests/fixtures/transcripts", env!("CARGO_MANIFEST_DIR"))
}

/// Copy the fixture tree into `<root>/demo-project/`.
fn install_fixture(root: &Path) -> std::path::PathBuf {
    let project = root.join("demo-project");
    fs::create_dir_all(project.join("session-1/subagents")).unwrap();
    fs::copy(
        format!("{}/session-1.jsonl", fixture_dir()),
        project.join("session-1.jsonl"),
    )
    .unwrap();
    fs::copy(
        format!("{}/session-1/subagents/agent-a1.jsonl", fixture_dir()),
        project.join("session-1/subagents/agent-a1.jsonl"),
    )
    .unwrap();
    project.join("session-1.jsonl")
}

#[test]
fn summarize_dedups_and_splits_agents() {
    let dir = tempfile::tempdir().unwrap();
    let file = install_fixture(dir.path());
    let s = summarize(&file).unwrap();

    assert_eq!(s.session_id, "session-1");
    assert_eq!(s.project.as_deref(), Some("demo-project"));
    assert_eq!(
        s.last_activity,
        Some(Utc.with_ymd_and_hms(2026, 10, 11, 10, 20, 1).unwrap())
    );

    let names: Vec<_> = s.agents.iter().map(|a| a.agent.as_str()).collect();
    assert_eq!(names, ["main", "agent-a1", "side1"], "{names:?}");
    // Subagent files are named after their file stem.
    let main = &s.agents[0];
    assert_eq!(main.agent, "main");
    assert_eq!(main.requests, 2, "req_1 counted once, synthetic skipped");
    // req_1 (last line wins) + req_2
    assert_eq!(main.tokens.input, 110);
    assert_eq!(main.tokens.output, 70);
    assert_eq!(main.tokens.cache_creation, 1000);
    assert_eq!(main.tokens.cache_read, 2000);
    assert_eq!(main.model.as_deref(), Some("claude-opus-4-1"));
    assert!(main.cost_usd_est.unwrap() > 0.0);

    let sub = s.agents.iter().find(|a| a.agent.contains("a1")).unwrap();
    assert_eq!(sub.requests, 1);
    assert_eq!(sub.tokens.input, 1000);
    assert_eq!(sub.tokens.output, 200);

    let side = s.agents.iter().find(|a| a.agent == "side1").unwrap();
    assert_eq!(side.tokens.total(), 10);

    let sum: u64 = s.agents.iter().map(|a| a.tokens.total()).sum();
    assert_eq!(s.tokens.total(), sum);
    let cost: f64 = s.agents.iter().filter_map(|a| a.cost_usd_est).sum();
    assert!((s.cost_usd_est.unwrap() - cost).abs() < 1e-12);
}

#[test]
fn find_latest_picks_newest_and_skips_subagents() {
    let dir = tempfile::tempdir().unwrap();
    let projects = dir.path();
    let a = install_fixture(projects);
    let other = projects.join("other-project");
    fs::create_dir_all(&other).unwrap();
    let b = other.join("session-2.jsonl");
    fs::write(&b, "").unwrap();

    let set = |p: &Path, secs_ago: u64| {
        File::options()
            .write(true)
            .open(p)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(secs_ago))
            .unwrap();
    };
    set(&a, 100);
    set(&b, 500);
    // The subagent file is the newest of all but must be ignored.
    set(
        &projects.join("demo-project/session-1/subagents/agent-a1.jsonl"),
        1,
    );
    assert_eq!(find_latest_session(projects), Some(a.clone()));

    set(&b, 10);
    assert_eq!(find_latest_session(projects), Some(b));

    assert_eq!(find_latest_session(&projects.join("missing")), None);
}

#[test]
fn missing_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(summarize(&dir.path().join("p/none.jsonl")).is_err());
}
