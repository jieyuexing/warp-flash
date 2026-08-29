#[cfg(not(target_family = "wasm"))]
use std::fs;

use super::external_cli_resume::*;

#[test]
fn resume_commands_use_explicit_session_ids() {
    let codex = ExternalCliResumeTarget::new(
        ExternalCliAgent::Codex,
        "019f6965-fb39-7101-9a0c-21706ff06d7b",
        Some("/work/project".to_owned()),
    )
    .expect("valid Codex target");
    let grok = ExternalCliResumeTarget::new(
        ExternalCliAgent::Grok,
        "session_123",
        Some("/work/project".to_owned()),
    )
    .expect("valid Grok target");

    assert_eq!(
        codex.resume_command().as_deref(),
        Some("codex resume 019f6965-fb39-7101-9a0c-21706ff06d7b")
    );
    assert_eq!(
        grok.resume_command().as_deref(),
        Some("grok --resume session_123")
    );
}

#[test]
fn unsafe_session_ids_are_rejected() {
    assert!(
        ExternalCliResumeTarget::new(ExternalCliAgent::Codex, "valid-id; touch /tmp/unsafe", None,)
            .is_none()
    );
    assert!(ExternalCliResumeTarget::new(ExternalCliAgent::Grok, "", None).is_none());
}

#[test]
fn command_detection_accepts_paths_but_not_similar_names() {
    assert_eq!(
        external_cli_agent_from_command("/opt/bin/codex --yolo"),
        Some(ExternalCliAgent::Codex)
    );
    assert_eq!(
        external_cli_agent_from_command("grok --resume abc"),
        Some(ExternalCliAgent::Grok)
    );
    assert_eq!(external_cli_agent_from_command("grok-helper"), None);
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn grok_registry_requires_exactly_one_session_for_the_cwd() {
    let one = r#"[
        {"session_id":"grok-one","pid":42,"cwd":"/work/project","opened_at":"now"}
    ]"#;
    let ambiguous = r#"[
        {"session_id":"grok-one","pid":42,"cwd":"/work/project","opened_at":"now"},
        {"session_id":"grok-two","pid":43,"cwd":"/work/project","opened_at":"later"}
    ]"#;

    assert_eq!(
        active_grok_resume_target_from_contents(one, "/work/project", None)
            .map(|target| target.session_id),
        Some("grok-one".to_owned())
    );
    assert!(active_grok_resume_target_from_contents(ambiguous, "/work/project", None).is_none());
    assert!(active_grok_resume_target_from_contents(one, "/work/other", None).is_none());
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn grok_registry_uses_the_latest_session_for_the_foreground_process() {
    let registry = r#"[
        {
            "session_id":"grok-placeholder",
            "pid":42,
            "cwd":"/work/project",
            "opened_at":"2026-08-29T02:04:59.569515Z"
        },
        {
            "session_id":"grok-current",
            "pid":42,
            "cwd":"/work/project",
            "opened_at":"2026-08-29T02:05:02.653535Z"
        },
        {
            "session_id":"grok-other-process",
            "pid":43,
            "cwd":"/work/project",
            "opened_at":"2026-08-29T02:06:00Z"
        }
    ]"#;

    let target = active_grok_resume_target_from_contents(registry, "/work/project", Some(42))
        .expect("foreground Grok process should identify its latest session");

    assert_eq!(target.session_id, "grok-current");
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn grok_registry_rejects_tied_latest_sessions_for_the_foreground_process() {
    let registry = r#"[
        {
            "session_id":"grok-one",
            "pid":42,
            "cwd":"/work/project",
            "opened_at":"2026-08-29T02:05:02Z"
        },
        {
            "session_id":"grok-two",
            "pid":42,
            "cwd":"/work/project",
            "opened_at":"2026-08-29T02:05:02Z"
        }
    ]"#;

    assert!(active_grok_resume_target_from_contents(registry, "/work/project", Some(42)).is_none());
}

#[cfg(unix)]
#[test]
fn grok_registry_accepts_equivalent_cwd_symlinks() {
    use std::os::unix::fs::symlink;

    let workspace = tempfile::tempdir().expect("workspace");
    let actual_cwd = workspace.path().join("actual");
    let alias_cwd = workspace.path().join("alias");
    fs::create_dir(&actual_cwd).expect("actual cwd");
    symlink(&actual_cwd, &alias_cwd).expect("cwd symlink");

    let registry = serde_json::json!([{
        "session_id": "grok-symlink",
        "cwd": actual_cwd,
    }])
    .to_string();
    let target = active_grok_resume_target_from_contents(
        &registry,
        alias_cwd.to_str().expect("alias cwd"),
        None,
    )
    .expect("equivalent cwd target");

    assert_eq!(target.session_id, "grok-symlink");
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn codex_resume_command_preserves_its_explicit_session_id() {
    let codex_home = tempfile::tempdir().expect("codex home");
    let target = active_codex_resume_target_from_root(
        "/opt/bin/codex resume session-123 --yolo",
        "/work/project",
        codex_home.path(),
        &[],
    )
    .expect("explicit resume target");

    assert_eq!(target.session_id, "session-123");
    assert_eq!(target.cwd.as_deref(), Some("/work/project"));
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn plain_codex_launch_does_not_guess_a_session_from_shared_cwd() {
    let codex_home = tempfile::tempdir().expect("codex home");
    let sessions = codex_home.path().join("sessions/2026/08/27");
    fs::create_dir_all(&sessions).expect("session bucket");

    for (name, id, cwd, source) in [
        ("rollout-a.jsonl", "session-old", "/work/project", "cli"),
        ("rollout-b.jsonl", "session-new", "/work/project", "cli"),
        ("rollout-c.jsonl", "session-other", "/work/other", "cli"),
        (
            "rollout-d.jsonl",
            "session-subagent",
            "/work/project",
            "subagent",
        ),
    ] {
        fs::write(
            sessions.join(name),
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"cwd\":\"{cwd}\",\"source\":\"{source}\"}}}}\n"
            ),
        )
        .expect("write rollout");
    }

    assert!(
        active_codex_resume_target_from_root(
            "codex --yolo",
            "/work/project",
            codex_home.path(),
            &[],
        )
        .is_none()
    );
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn plain_codex_launch_uses_the_rollout_opened_by_its_process_group() {
    let codex_home = tempfile::tempdir().expect("codex home");
    let sessions = codex_home.path().join("sessions/2026/08/27");
    fs::create_dir_all(&sessions).expect("session bucket");
    let current = sessions.join("rollout-current.jsonl");
    let unrelated = sessions.join("rollout-unrelated.jsonl");
    fs::write(
        &current,
        r#"{"type":"session_meta","payload":{"id":"session-current","cwd":"/work/project","source":"cli"}}
"#,
    )
    .expect("current rollout");
    fs::write(
        &unrelated,
        r#"{"type":"session_meta","payload":{"id":"session-unrelated","cwd":"/work/project","source":"cli"}}
"#,
    )
    .expect("unrelated rollout");

    let target = active_codex_resume_target_from_root(
        "codex --yolo",
        "/work/project",
        codex_home.path(),
        &[current],
    )
    .expect("process-bound rollout should resolve");

    assert_eq!(target.session_id, "session-current");
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn plain_codex_launch_rejects_multiple_open_cli_rollouts() {
    let codex_home = tempfile::tempdir().expect("codex home");
    let sessions = codex_home.path().join("sessions/2026/08/27");
    fs::create_dir_all(&sessions).expect("session bucket");
    let first = sessions.join("rollout-first.jsonl");
    let second = sessions.join("rollout-second.jsonl");
    for (path, id) in [(&first, "session-first"), (&second, "session-second")] {
        fs::write(
            path,
            format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"cwd\":\"/work/project\",\"source\":\"cli\"}}}}\n"
            ),
        )
        .expect("rollout");
    }

    assert!(
        active_codex_resume_target_from_root(
            "codex",
            "/work/project",
            codex_home.path(),
            &[first, second],
        )
        .is_none()
    );
}
