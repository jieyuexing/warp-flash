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
        active_grok_resume_target_from_contents(one, "/work/project")
            .map(|target| target.session_id),
        Some("grok-one".to_owned())
    );
    assert!(active_grok_resume_target_from_contents(ambiguous, "/work/project").is_none());
    assert!(active_grok_resume_target_from_contents(one, "/work/other").is_none());
}
