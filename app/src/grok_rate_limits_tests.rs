use super::{
    BillingResponse, GrokQuotaSnapshot, GrokQuotaState, format_reset_rfc3339,
    snapshot_from_response,
};

#[test]
fn quota_state_always_has_a_visible_label() {
    assert_eq!(GrokQuotaState::Loading.label(), "Grok …");
    assert_eq!(GrokQuotaState::Unavailable.label(), "Grok --");
    assert_eq!(
        GrokQuotaState::Available(GrokQuotaSnapshot {
            remaining_percent: 78,
            resets_at: None,
        })
        .label(),
        "Grok 78%"
    );
}

#[test]
fn quota_detail_identifies_local_source_and_refresh_action() {
    assert_eq!(
        GrokQuotaState::Loading.detail_label(),
        "Local Grok quota loading"
    );
    assert_eq!(
        GrokQuotaState::Available(GrokQuotaSnapshot {
            remaining_percent: 78,
            resets_at: None,
        })
        .detail_label(),
        "Local Grok · 78% remaining · click to refresh"
    );
    assert_eq!(
        GrokQuotaState::Unavailable.detail_label(),
        "Local Grok quota unavailable · click to retry"
    );
}

#[test]
fn quota_detail_formats_valid_reset_and_ignores_invalid_timestamp() {
    let detail = GrokQuotaState::Available(GrokQuotaSnapshot {
        remaining_percent: 42,
        resets_at: Some("2026-07-22T02:36:20Z".to_owned()),
    })
    .detail_label();

    assert!(detail.starts_with("Local Grok · 42% remaining · resets "));
    assert!(detail.ends_with(" · click to refresh"));
    assert_eq!(format_reset_rfc3339("not-a-timestamp"), None);
}

#[test]
fn parses_remaining_percent_from_credits_config() {
    let response: BillingResponse = serde_json::from_value(serde_json::json!({
        "config": {
            "creditUsagePercent": 33.7,
            "currentPeriod": { "end": "2026-07-22T02:36:20Z" }
        }
    }))
    .unwrap();

    let snapshot = snapshot_from_response(response).unwrap();
    assert_eq!(snapshot.remaining_percent(), 66);
    assert_eq!(snapshot.resets_at(), Some("2026-07-22T02:36:20Z"));
}

#[test]
fn falls_back_to_legacy_limit_and_usage() {
    let response: BillingResponse = serde_json::from_value(serde_json::json!({
        "config": {
            "monthlyLimit": { "val": 10000 },
            "used": { "val": 2500 }
        }
    }))
    .unwrap();

    assert_eq!(
        snapshot_from_response(response)
            .unwrap()
            .remaining_percent(),
        75
    );
}

#[test]
fn missing_usage_matches_grok_zero_usage_behavior() {
    let response: BillingResponse = serde_json::from_value(serde_json::json!({
        "config": {
            "creditUsagePercent": null,
            "monthlyLimit": null,
            "used": null,
            "currentPeriod": { "type": "USAGE_PERIOD_TYPE_WEEKLY" }
        }
    }))
    .unwrap();

    assert_eq!(
        snapshot_from_response(response)
            .unwrap()
            .remaining_percent(),
        100
    );
}

#[test]
fn clamps_over_limit_usage() {
    let response: BillingResponse = serde_json::from_value(serde_json::json!({
        "config": { "creditUsagePercent": 125.0 }
    }))
    .unwrap();

    assert_eq!(
        snapshot_from_response(response)
            .unwrap()
            .remaining_percent(),
        0
    );
}
