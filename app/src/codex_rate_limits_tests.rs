use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use super::{
    CodexQuotaSnapshot, CodexQuotaState, RateLimitsReadResponse, codex_path_env,
    codex_program_from, format_reset_epoch, snapshot_from_response,
};

#[test]
fn quota_state_always_has_a_visible_label() {
    assert_eq!(CodexQuotaState::Loading.label(), "Codex …");
    assert_eq!(CodexQuotaState::Unavailable.label(), "Codex --");
    assert_eq!(
        CodexQuotaState::Available(CodexQuotaSnapshot {
            remaining_percent: 78,
            resets_at: None,
            window_duration_mins: None,
        })
        .label(),
        "Codex 78%"
    );
}

#[test]
fn quota_detail_identifies_local_source_and_refresh_action() {
    assert_eq!(
        CodexQuotaState::Loading.detail_label(),
        "Local Codex quota loading"
    );
    assert_eq!(
        CodexQuotaState::Available(CodexQuotaSnapshot {
            remaining_percent: 78,
            resets_at: None,
            window_duration_mins: None,
        })
        .detail_label(),
        "Local Codex · 78% remaining · click to refresh"
    );
    assert_eq!(
        CodexQuotaState::Unavailable.detail_label(),
        "Local Codex quota unavailable · click to retry"
    );
}

#[test]
fn quota_detail_formats_valid_reset_and_ignores_invalid_epoch() {
    let detail = CodexQuotaState::Available(CodexQuotaSnapshot {
        remaining_percent: 42,
        resets_at: Some(1_784_949_880),
        window_duration_mins: Some(10_080),
    })
    .detail_label();

    assert!(detail.starts_with("Local Codex · 42% remaining · resets "));
    assert!(detail.ends_with(" · click to refresh"));
    assert_eq!(format_reset_epoch(i64::MAX), None);
}

#[test]
fn codex_program_directory_is_added_to_child_path() {
    let path = codex_path_env(Path::new("/opt/homebrew/bin/codex"));
    assert_eq!(
        std::env::split_paths(&path).next().as_deref(),
        Some(Path::new("/opt/homebrew/bin"))
    );
}

#[cfg(target_os = "macos")]
#[test]
fn discovers_codex_in_user_local_bin_for_gui_launches() {
    let home = tempfile::tempdir().unwrap();
    let codex = home.path().join(".local/bin/codex");
    fs::create_dir_all(codex.parent().unwrap()).unwrap();
    fs::write(&codex, b"test codex executable").unwrap();

    assert_eq!(
        codex_program_from(
            Some(OsStr::new("/usr/bin:/bin:/usr/sbin:/sbin")),
            Some(home.path()),
        ),
        codex
    );
}

#[test]
fn parses_remaining_percent_from_primary_window() {
    let response: RateLimitsReadResponse = serde_json::from_value(serde_json::json!({
        "result": {
            "rateLimits": {
                "primary": {
                    "usedPercent": 22,
                    "windowDurationMins": 10080,
                    "resetsAt": 1784949880
                },
                "secondary": null
            }
        }
    }))
    .unwrap();

    let snapshot = snapshot_from_response(response).unwrap();
    assert_eq!(snapshot.remaining_percent(), 78);
    assert_eq!(snapshot.window_duration_mins(), Some(10080));
    assert_eq!(snapshot.resets_at(), Some(1784949880));
}

#[test]
fn chooses_the_most_constrained_active_window() {
    let response: RateLimitsReadResponse = serde_json::from_value(serde_json::json!({
        "result": {
            "rateLimits": {
                "primary": { "usedPercent": 10 },
                "secondary": { "usedPercent": 75 }
            }
        }
    }))
    .unwrap();

    let snapshot = snapshot_from_response(response).unwrap();
    assert_eq!(snapshot.remaining_percent(), 25);
}

#[test]
fn clamps_out_of_range_percentages() {
    let response: RateLimitsReadResponse = serde_json::from_value(serde_json::json!({
        "result": {
            "rateLimits": {
                "primary": { "usedPercent": 120 },
                "secondary": null
            }
        }
    }))
    .unwrap();

    let snapshot = snapshot_from_response(response).unwrap();
    assert_eq!(snapshot.remaining_percent(), 0);
}
