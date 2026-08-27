use super::*;

#[test]
fn cli_agent_title_includes_project_name() {
    let title = TerminalTitleGenerator {
        cli_agent: Some(CLIAgent::Codex),
        task_title: None,
        project: Some("/Users/test/jieyuexing-universe"),
        working_directory: Some("/Users/test/other"),
        osc_title: Some("zsh"),
        shell_title: "zsh",
    }
    .generate();

    assert_eq!(title, "Codex · jieyuexing-universe");
}

#[test]
fn cli_agent_falls_back_to_working_directory() {
    let title = TerminalTitleGenerator {
        cli_agent: Some(CLIAgent::OpenCode),
        task_title: None,
        project: None,
        working_directory: Some("/work/warp"),
        osc_title: None,
        shell_title: "zsh",
    }
    .generate();

    assert_eq!(title, "OpenCode · warp");
}

#[test]
fn osc_title_wins_for_regular_terminal() {
    let title = TerminalTitleGenerator {
        cli_agent: None,
        task_title: None,
        project: None,
        working_directory: Some("/work/warp"),
        osc_title: Some("release build"),
        shell_title: "zsh",
    }
    .generate();

    assert_eq!(title, "release build");
}

#[test]
fn regular_terminal_uses_directory_before_shell_name() {
    let title = TerminalTitleGenerator {
        cli_agent: None,
        task_title: None,
        project: None,
        working_directory: Some("/work/warp/"),
        osc_title: None,
        shell_title: "zsh",
    }
    .generate();

    assert_eq!(title, "warp");
}

#[test]
fn first_prompt_title_wins_over_project_context() {
    let title = TerminalTitleGenerator {
        cli_agent: Some(CLIAgent::Codex),
        task_title: Some("  修复登录流程\n并补充测试  "),
        project: Some("/work/project"),
        working_directory: Some("/work/project"),
        osc_title: None,
        shell_title: "zsh",
    }
    .generate();

    assert_eq!(title, "Codex · 修复登录流程 并补充测试");
}

#[test]
fn prompt_title_is_truncated_on_unicode_character_boundaries() {
    let prompt = "一".repeat(49);
    let title = normalized_prompt_title(&prompt).unwrap();

    assert_eq!(title, format!("{}…", "一".repeat(48)));
}

#[test]
fn contextual_directory_title_is_distinguished_from_manual_title() {
    assert!(is_contextual_directory_title(
        "jieyuexing-universe",
        Some("~/jieyuexing-universe")
    ));
    assert!(!is_contextual_directory_title(
        "发布监控",
        Some("~/jieyuexing-universe")
    ));
}

#[test]
fn cli_agent_is_detected_from_running_command() {
    assert_eq!(
        cli_agent_from_command("codex --yolo"),
        Some(CLIAgent::Codex)
    );
    assert_eq!(
        cli_agent_from_command("/opt/homebrew/bin/opencode"),
        Some(CLIAgent::OpenCode)
    );
    assert_eq!(cli_agent_from_command("cargo test"), None);
}
