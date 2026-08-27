use super::*;

#[test]
fn cli_activity_is_distinct_from_last_turn_outcome() {
    assert_eq!(
        status_for_cli_session(
            CLIAgentSessionActivity::Running,
            &CLIAgentSessionStatus::Success,
        ),
        TerminalSessionStatus::Running
    );
    assert_eq!(
        status_for_cli_session(
            CLIAgentSessionActivity::Idle,
            &CLIAgentSessionStatus::Success,
        ),
        TerminalSessionStatus::Ready
    );
    assert_eq!(
        status_for_cli_session(
            CLIAgentSessionActivity::WaitingForInput,
            &CLIAgentSessionStatus::InProgress,
        ),
        TerminalSessionStatus::NeedsInput
    );
}

#[test]
fn live_command_duration_only_updates_for_active_agent_work() {
    assert!(should_update_live_command_duration(None));
    assert!(should_update_live_command_duration(Some(
        CLIAgentSessionActivity::Running
    )));
    assert!(!should_update_live_command_duration(Some(
        CLIAgentSessionActivity::Loading
    )));
    assert!(!should_update_live_command_duration(Some(
        CLIAgentSessionActivity::WaitingForInput
    )));
    assert!(!should_update_live_command_duration(Some(
        CLIAgentSessionActivity::Idle
    )));
}
