use warpui::{AppContext, EntityId, SingletonEntity};

use super::cli_agent_sessions::{
    CLIAgentSessionActivity, CLIAgentSessionStatus, CLIAgentSessionsModel,
};
use super::view::TerminalView;
use crate::ai::agent::conversation::ConversationStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalSessionStatus {
    Loading,
    Running,
    NeedsInput,
    Waiting,
    Ready,
    Error,
    Cancelled,
    Idle,
}

impl TerminalSessionStatus {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Loading => "Loading",
            Self::Running => "Running",
            Self::NeedsInput => "Needs input",
            Self::Waiting => "Waiting",
            Self::Ready => "Ready",
            Self::Error => "Error",
            Self::Cancelled => "Cancelled",
            Self::Idle => "Idle",
        }
    }

    pub(crate) fn to_conversation_status(self) -> ConversationStatus {
        match self {
            Self::Loading => ConversationStatus::TransientError,
            Self::Running => ConversationStatus::InProgress,
            Self::NeedsInput => ConversationStatus::Blocked {
                blocked_action: String::new(),
            },
            Self::Waiting => ConversationStatus::WaitingForEvents,
            Self::Ready => ConversationStatus::Success,
            Self::Error => ConversationStatus::Error,
            Self::Cancelled => ConversationStatus::Cancelled,
            Self::Idle => ConversationStatus::Cancelled,
        }
    }
}

pub(crate) fn status_for_cli_session(
    activity: CLIAgentSessionActivity,
    outcome: &CLIAgentSessionStatus,
) -> TerminalSessionStatus {
    match activity {
        CLIAgentSessionActivity::Loading => TerminalSessionStatus::Loading,
        CLIAgentSessionActivity::Running => TerminalSessionStatus::Running,
        CLIAgentSessionActivity::WaitingForInput => TerminalSessionStatus::NeedsInput,
        CLIAgentSessionActivity::Idle => match outcome {
            CLIAgentSessionStatus::InProgress => TerminalSessionStatus::Idle,
            CLIAgentSessionStatus::Success => TerminalSessionStatus::Ready,
            CLIAgentSessionStatus::Failed { .. } => TerminalSessionStatus::Error,
            CLIAgentSessionStatus::Blocked { .. } => TerminalSessionStatus::NeedsInput,
            CLIAgentSessionStatus::Cancelled => TerminalSessionStatus::Cancelled,
        },
    }
}

fn status_for_conversation(status: &ConversationStatus) -> TerminalSessionStatus {
    match status {
        ConversationStatus::InProgress => TerminalSessionStatus::Running,
        ConversationStatus::Success => TerminalSessionStatus::Ready,
        ConversationStatus::Error => TerminalSessionStatus::Error,
        ConversationStatus::TransientError => TerminalSessionStatus::Loading,
        ConversationStatus::Cancelled => TerminalSessionStatus::Cancelled,
        ConversationStatus::Blocked { .. } => TerminalSessionStatus::NeedsInput,
        ConversationStatus::WaitingForEvents => TerminalSessionStatus::Waiting,
    }
}

pub(crate) fn terminal_session_status(
    terminal_view: &TerminalView,
    app: &AppContext,
) -> Option<TerminalSessionStatus> {
    if !terminal_view.is_login_shell_bootstrapped() {
        return Some(TerminalSessionStatus::Loading);
    }

    let cli_sessions = CLIAgentSessionsModel::as_ref(app);
    if let Some(session) = cli_sessions.session(terminal_view.id()) {
        let activity = cli_sessions
            .activity(terminal_view.id())
            .unwrap_or(CLIAgentSessionActivity::Loading);
        return Some(status_for_cli_session(activity, &session.status));
    }

    let has_agent_conversation = terminal_view.is_ambient_agent_session(app)
        || terminal_view
            .selected_conversation_display_title(app)
            .is_some();
    if has_agent_conversation
        && let Some(status) = terminal_view.selected_conversation_status_for_display(app)
    {
        return Some(status_for_conversation(&status));
    }

    terminal_view
        .is_long_running()
        .then_some(TerminalSessionStatus::Running)
}

pub(crate) fn should_update_live_command_duration(
    activity: Option<CLIAgentSessionActivity>,
) -> bool {
    activity.is_none_or(|activity| matches!(activity, CLIAgentSessionActivity::Running))
}

pub(crate) fn should_update_terminal_live_command_duration(
    terminal_view_id: EntityId,
    app: &AppContext,
) -> bool {
    let sessions = CLIAgentSessionsModel::as_ref(app);
    if sessions.session(terminal_view_id).is_none() {
        return true;
    }
    should_update_live_command_duration(sessions.activity(terminal_view_id))
}

#[cfg(test)]
#[path = "session_status_tests.rs"]
mod tests;
