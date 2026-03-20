use crate::display::*;
use crate::types::*;

#[test]
fn test_parse_stdin_full() {
    let json = r#"{
        "cwd": "/home/user",
        "session_id": "abc123",
        "transcript_path": "/tmp/transcript.jsonl",
        "model": {
            "id": "claude-opus-4-6",
            "display_name": "Opus"
        },
        "cost": {
            "total_cost_usd": 0.12345,
            "total_duration_ms": 120000
        },
        "context_window": {
            "used_percentage": 42,
            "context_window_size": 200000,
            "remaining_percentage": 58
        }
    }"#;

    let data: StdinData = serde_json::from_str(json).unwrap();
    assert_eq!(data.session_id.as_deref(), Some("abc123"));
    assert_eq!(
        data.model.as_ref().unwrap().display_name.as_deref(),
        Some("Opus")
    );
    assert!((data.cost.as_ref().unwrap().total_cost_usd.unwrap() - 0.12345).abs() < f64::EPSILON);
    assert_eq!(data.cost.as_ref().unwrap().total_duration_ms, Some(120000));
    assert_eq!(
        data.context_window.as_ref().unwrap().used_percentage,
        Some(42.0)
    );
}

#[test]
fn test_parse_stdin_minimal() {
    let json = r#"{}"#;
    let data: StdinData = serde_json::from_str(json).unwrap();
    assert!(data.session_id.is_none());
    assert!(data.model.is_none());
    assert!(data.cost.is_none());
    assert!(data.context_window.is_none());
}

#[test]
fn test_parse_stdin_null_fields() {
    let json = r#"{
        "session_id": "sess1",
        "model": { "id": "claude-opus-4-6", "display_name": "Opus" },
        "cost": { "total_cost_usd": null },
        "context_window": {
            "used_percentage": null,
            "context_window_size": 200000,
            "remaining_percentage": null
        }
    }"#;

    let data: StdinData = serde_json::from_str(json).unwrap();
    assert!(data.cost.as_ref().unwrap().total_cost_usd.is_none());
    assert!(data
        .context_window
        .as_ref()
        .unwrap()
        .used_percentage
        .is_none());
}

#[test]
fn test_parse_usage_response() {
    let json = r#"{
        "five_hour": {
            "utilization": 58.0,
            "resets_at": "2026-03-19T14:30:00Z"
        },
        "seven_day": {
            "utilization": 22.0,
            "resets_at": "2026-03-25T00:00:00Z"
        }
    }"#;

    let resp: UsageResponse = serde_json::from_str(json).unwrap();
    assert!((resp.five_hour.as_ref().unwrap().utilization.unwrap() - 58.0).abs() < f64::EPSILON);
    assert!((resp.seven_day.as_ref().unwrap().utilization.unwrap() - 22.0).abs() < f64::EPSILON);
}

#[test]
fn test_parse_usage_response_nulls() {
    let json = r#"{ "five_hour": null, "seven_day": null }"#;
    let resp: UsageResponse = serde_json::from_str(json).unwrap();
    assert!(resp.five_hour.is_none());
    assert!(resp.seven_day.is_none());
}

#[test]
fn test_output_format_segments() {
    // Simulate building the output segments (matches main.rs identity format)
    let model = "Opus";
    let plan = "Pro";
    let ctx_pct = 42.0;
    let session_cost = 0.12;
    let daily_total = 3.45;

    let mut segments = Vec::new();

    let dim = dim();
    let cyan = cyan();
    segments.push(format!(
        "{dim}[{RESET}{cyan}{model}{RESET}{dim}{NBSP}|{NBSP}{plan}]{RESET}"
    ));

    let color = context_color(ctx_pct);
    let ctx_bar = bar(ctx_pct, 5, color);
    segments.push(format!(
        "{ctx_bar}{NBSP}{color}{ctx_pct:.0}%{RESET}{NBSP}{dim}ctx{RESET}"
    ));

    segments.push(format!(
        "{}{NBSP}{dim}sess{RESET}",
        format_cost(session_cost)
    ));

    segments.push(format!(
        "{}{NBSP}{dim}today{RESET}",
        format_cost(daily_total)
    ));

    let line = segments.join(&sep());
    let output = nbsp(&line);

    // Verify key content is present (ignoring ANSI codes)
    assert!(output.contains("Opus"));
    assert!(output.contains("Pro"));
    assert!(output.contains("42%"));
    assert!(output.contains("ctx"));
    assert!(output.contains("$0.12"));
    assert!(output.contains("sess"));
    assert!(output.contains("$3.45"));
    assert!(output.contains("today"));
    // Non-breaking spaces should be used
    assert!(output.contains(NBSP));
}

#[test]
fn test_progress_bar_boundary_values() {
    // 0% should be all empty
    let b = bar(0.0, 5, blue());
    assert_eq!(b.matches('\u{2588}').count(), 0);
    assert_eq!(b.matches('\u{2591}').count(), 5);

    // 100% should be all filled
    let b = bar(100.0, 5, blue());
    assert_eq!(b.matches('\u{2588}').count(), 5);
    assert_eq!(b.matches('\u{2591}').count(), 0);

    // 20% of 5 = 1 filled
    let b = bar(20.0, 5, blue());
    assert_eq!(b.matches('\u{2588}').count(), 1);
    assert_eq!(b.matches('\u{2591}').count(), 4);

    // Negative clamped to 0
    let b = bar(-10.0, 5, blue());
    assert_eq!(b.matches('\u{2588}').count(), 0);

    // Over 100 clamped
    let b = bar(150.0, 5, blue());
    assert_eq!(b.matches('\u{2588}').count(), 5);
}

#[test]
fn test_daily_cost_cache_roundtrip() {
    use std::collections::HashMap;

    let cache = DailyCostCache {
        date: "2026-03-19".to_string(),
        sessions: {
            let mut m = HashMap::new();
            m.insert("sess_a".to_string(), 1.5);
            m.insert("sess_b".to_string(), 2.5);
            m
        },
        total: 4.0,
    };

    let json = serde_json::to_string(&cache).unwrap();
    let parsed: DailyCostCache = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.date, "2026-03-19");
    assert!((parsed.total - 4.0).abs() < f64::EPSILON);
    assert_eq!(parsed.sessions.len(), 2);
}

#[test]
fn test_cache_entry_roundtrip() {
    let entry = CacheEntry {
        data: Some(serde_json::json!({"five_hour": {"utilization": 50.0}})),
        timestamp: 1234567890,
        ok: true,
        rate_limited: false,
        retry_after: 0,
    };

    let json = serde_json::to_string(&entry).unwrap();
    let parsed: CacheEntry = serde_json::from_str(&json).unwrap();
    assert!(parsed.ok);
    assert!(!parsed.rate_limited);
    assert!(parsed.data.is_some());
}

#[test]
fn test_parse_profile_response() {
    let json = r#"{
        "account": {
            "email": "user@example.com",
            "display_name": "Test User",
            "full_name": "Test User"
        },
        "organization": {
            "name": "TestOrg"
        }
    }"#;
    let resp: ProfileResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.account.email, "user@example.com");
    assert_eq!(resp.account.display_name.as_deref(), Some("Test User"));
    assert_eq!(resp.organization.name.as_deref(), Some("TestOrg"));
}

#[test]
fn test_parse_profile_cache_entry_roundtrip() {
    let profile = ProfileResponse {
        account: ProfileAccount {
            email: "user@example.com".to_string(),
            display_name: Some("Test".to_string()),
            full_name: None,
        },
        organization: ProfileOrganization { name: None },
    };
    let entry = ProfileCacheEntry {
        data: Some(serde_json::to_value(&profile).unwrap()),
        timestamp: 1234567890,
        ok: true,
    };
    let json = serde_json::to_string(&entry).unwrap();
    let parsed: ProfileCacheEntry = serde_json::from_str(&json).unwrap();
    assert!(parsed.ok);
    assert_eq!(parsed.timestamp, 1234567890);
    let restored: ProfileResponse = serde_json::from_value(parsed.data.unwrap()).unwrap();
    assert_eq!(restored.account.email, "user@example.com");
}

#[test]
fn test_parse_stdin_with_duration() {
    let json = r#"{
        "session_id": "sess1",
        "cost": {
            "total_cost_usd": 0.50,
            "total_duration_ms": 65000
        }
    }"#;
    let data: StdinData = serde_json::from_str(json).unwrap();
    assert_eq!(data.cost.as_ref().unwrap().total_duration_ms, Some(65000));
    assert_eq!(format_duration(65000), format!("1m{NBSP}5s"));
}
