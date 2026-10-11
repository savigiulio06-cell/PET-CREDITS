use chrono::{Duration, TimeZone, Utc};
use pet_core::claude::oauth::{
    fetch_usage, load_credentials_from, parse_credentials, parse_usage_response, OAuthCredentials,
};
use pet_core::{Error, WindowKind};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn ts(secs: i64) -> chrono::DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}

#[test]
fn old_shape() {
    let w = parse_usage_response(&fixture("usage_old.json")).unwrap();
    assert_eq!(
        w.len(),
        3,
        "null / null-utilization entries are skipped: {w:?}"
    );

    assert_eq!(w[0].kind, WindowKind::Session);
    assert_eq!(w[0].label, "Session (5h)");
    assert_eq!(w[0].used_pct, 42.0);
    assert_eq!(w[0].window_minutes, Some(300));
    assert_eq!(
        w[0].resets_at,
        Some(Utc.with_ymd_and_hms(2026, 10, 11, 17, 0, 0).unwrap())
    );

    assert_eq!(w[1].kind, WindowKind::Weekly);
    assert_eq!(w[1].window_minutes, Some(10080));
    assert_eq!(w[1].used_pct, 63.5);

    assert_eq!(w[2].kind, WindowKind::WeeklyScoped);
    assert_eq!(w[2].label, "Weekly · Opus");
    // epoch seconds
    assert_eq!(w[2].resets_at, Some(ts(1_792_000_000)));
}

#[test]
fn new_shape_wins_over_old_and_handles_kinds() {
    let w = parse_usage_response(&fixture("usage_new.json")).unwrap();
    assert_eq!(w.len(), 5, "{w:?}");
    assert_eq!(w[0].kind, WindowKind::Session);
    assert_eq!(w[0].used_pct, 42.0, "limits[] preferred over five_hour");
    assert_eq!(w[1].kind, WindowKind::Weekly);
    assert_eq!(w[1].resets_at, Some(ts(1_792_000_000)));
    assert_eq!(w[2].kind, WindowKind::WeeklyScoped);
    assert_eq!(w[2].label, "Weekly · Opus");
    // epoch milliseconds
    assert_eq!(w[2].resets_at, Some(ts(1_792_000_000)));
    assert_eq!(w[3].kind, WindowKind::WeeklyScoped);
    assert_eq!(w[3].label, "Weekly · scoped");
    assert_eq!(w[3].resets_at, None);
    assert_eq!(w[4].kind, WindowKind::Other);
    assert_eq!(w[4].label, "monthly_credits");
    assert_eq!(w[4].window_minutes, None);
}

#[test]
fn small_values_are_not_rescaled() {
    let w = parse_usage_response(r#"{"five_hour":{"utilization":0.4,"resets_at":null}}"#).unwrap();
    assert_eq!(w[0].used_pct, 0.4);
}

#[test]
fn empty_and_garbage() {
    assert!(parse_usage_response("{}").unwrap().is_empty());
    assert!(parse_usage_response(r#"{"limits":[]}"#).unwrap().is_empty());
    assert!(matches!(
        parse_usage_response("nope"),
        Err(Error::Format(_))
    ));
    assert!(matches!(parse_usage_response("[1]"), Err(Error::Format(_))));
}

#[test]
fn credentials_debug_redacts_token() {
    let c = parse_credentials(&fixture("credentials.json")).unwrap();
    let dbg = format!("{c:?}");
    assert!(!dbg.contains("FAKE"), "{dbg}");
    assert!(!dbg.contains("sk-ant"), "{dbg}");
    assert!(dbg.contains("***"));
    assert_eq!(c.subscription_type.as_deref(), Some("max"));
    assert_eq!(c.expires_at, Some(ts(1_792_000_000)));
}

#[test]
fn credentials_errors_do_not_echo_input() {
    let err = parse_credentials(r#"{"claudeAiOauth":{"accessToken":12345}}"#).unwrap_err();
    assert!(!err.to_string().contains("12345"));
    assert!(matches!(
        parse_credentials(r#"{"claudeAiOauth":{}}"#),
        Err(Error::Auth(_))
    ));
    assert!(matches!(parse_credentials("{}"), Err(Error::Auth(_))));
}

#[test]
fn load_credentials_from_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(".credentials.json"),
        fixture("credentials.json"),
    )
    .unwrap();
    let c = load_credentials_from(dir.path()).unwrap();
    assert!(!format!("{c:?}").contains("FAKE"));
}

#[test]
fn expired_token_is_auth_error_without_network() {
    let c = OAuthCredentials::new(
        "sk-ant-oat-FAKE",
        Some(Utc::now() - Duration::minutes(1)),
        None,
    );
    assert!(c.is_expired());
    match fetch_usage(&c) {
        Err(Error::Auth(msg)) => {
            assert!(msg.contains("expired"));
            assert!(!msg.contains("FAKE"));
        }
        other => panic!("expected Auth error, got {other:?}"),
    }
}
