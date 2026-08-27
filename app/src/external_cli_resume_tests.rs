use super::external_cli_resume::*;

#[cfg(not(target_family = "wasm"))]
use std::fs;

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
        active_grok_resume_target_from_contents(one, "/work/project")
            .map(|target| target.session_id),
        Some("grok-one".to_owned())
    );
    assert!(active_grok_resume_target_from_contents(ambiguous, "/work/project").is_none());
    assert!(active_grok_resume_target_from_contents(one, "/work/other").is_none());
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
    let target =
        active_grok_resume_target_from_contents(&registry, alias_cwd.to_str().expect("alias cwd"))
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
    )
    .expect("explicit resume target");

    assert_eq!(target.session_id, "session-123");
    assert_eq!(target.cwd.as_deref(), Some("/work/project"));
}

#[cfg(not(target_family = "wasm"))]
#[test]
fn codex_session_lookup_uses_latest_cli_rollout_for_exact_cwd() {
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

    let target =
        active_codex_resume_target_from_root("codex --yolo", "/work/project", codex_home.path())
            .expect("latest matching Codex session");

    assert_eq!(target.session_id, "session-new");
}

#[cfg(unix)]
#[test]
fn codex_session_lookup_accepts_equivalent_cwd_symlinks() {
    use std::os::unix::fs::symlink;

    let codex_home = tempfile::tempdir().expect("codex home");
    let workspace = tempfile::tempdir().expect("workspace");
    let actual_cwd = workspace.path().join("actual");
    let alias_cwd = workspace.path().join("alias");
    fs::create_dir(&actual_cwd).expect("actual cwd");
    symlink(&actual_cwd, &alias_cwd).expect("cwd symlink");

    let sessions = codex_home.path().join("sessions/2026/08/27");
    fs::create_dir_all(&sessions).expect("session bucket");
    fs::write(
        sessions.join("rollout-symlink.jsonl"),
        format!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"session-symlink\",\"cwd\":\"{}\",\"source\":\"cli\"}}}}\n",
            actual_cwd.display()
        ),
    )
    .expect("write rollout");

    let target = active_codex_resume_target_from_root(
        "codex",
        alias_cwd.to_str().expect("alias cwd"),
        codex_home.path(),
    )
    .expect("equivalent cwd target");

    assert_eq!(target.session_id, "session-symlink");
}
