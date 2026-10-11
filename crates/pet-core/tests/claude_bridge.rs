use chrono::{Duration, TimeZone, Utc};
use pet_core::claude::bridge::{from_statusline_input_at, read, write};
use pet_core::{Error, WindowKind};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn parses_full_input() {
    let now = Utc::now();
    let d = from_statusline_input_at(&fixture("statusline_full.json"), now).unwrap();
    assert_eq!(d.session_id.as_deref(), Some("session-1"));
    assert_eq!(d.model.as_deref(), Some("Opus"));
    assert_eq!(d.session_cost_usd, Some(1.25));
    assert_eq!(d.windows.len(), 2);
    assert_eq!(d.windows[0].kind, WindowKind::Session);
    assert_eq!(d.windows[0].used_pct, 23.5);
    assert_eq!(
        d.windows[0].resets_at,
        Some(Utc.timestamp_opt(1_792_000_000, 0).unwrap())
    );
    assert_eq!(d.windows[1].kind, WindowKind::Weekly);
    assert_eq!(
        d.windows[1].resets_at,
        Some(Utc.with_ymd_and_hms(2026, 10, 14, 9, 0, 0).unwrap())
    );
    assert_eq!(d.windows_at, Some(now));
}

#[test]
fn tolerates_missing_everything() {
    let d = from_statusline_input_at("{}", Utc::now()).unwrap();
    assert!(d.windows.is_empty());
    assert!(d.session_id.is_none() && d.model.is_none() && d.session_cost_usd.is_none());
    let d = from_statusline_input_at(
        r#"{"model":"x","rate_limits":{"five_hour":{"used_percentage":null}}}"#,
        Utc::now(),
    )
    .unwrap();
    assert_eq!(d.model.as_deref(), Some("x"));
    assert!(d.windows.is_empty());
    assert!(matches!(
        from_statusline_input_at("not json", Utc::now()),
        Err(Error::Format(_))
    ));
}

#[test]
fn write_read_roundtrip_and_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/dir/bridge.json");
    assert!(read(&path).unwrap().is_none());
    let d = from_statusline_input_at(&fixture("statusline_full.json"), Utc::now()).unwrap();
    write(&path, &d).unwrap();
    assert_eq!(read(&path).unwrap().unwrap(), d);
    // no temp files left behind
    let left: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().collect();
    assert_eq!(left.len(), 1);
}

#[test]
fn input_without_limits_keeps_previous_windows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bridge.json");
    let t0 = Utc::now() - Duration::minutes(5);
    let first = from_statusline_input_at(&fixture("statusline_full.json"), t0).unwrap();
    write(&path, &first).unwrap();

    let t1 = Utc::now();
    let second = from_statusline_input_at(&fixture("statusline_no_limits.json"), t1).unwrap();
    assert!(second.windows.is_empty());
    write(&path, &second).unwrap();

    let stored = read(&path).unwrap().unwrap();
    assert_eq!(stored.windows, first.windows, "last good windows preserved");
    assert_eq!(stored.windows_at, Some(t0), "their age is preserved too");
    assert_eq!(stored.written_at, t1);
    assert_eq!(stored.session_cost_usd, Some(2.5), "rest is updated");
}

#[test]
fn malformed_bridge_file_is_format_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bridge.json");
    std::fs::write(&path, "{ broken").unwrap();
    assert!(matches!(read(&path), Err(Error::Format(_))));
}
