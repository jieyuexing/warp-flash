use serde_json::{Value, json};
use strum_macros::{EnumDiscriminants, EnumIter};
use warp_core::telemetry::{EnablementState, TelemetryEvent, TelemetryEventDesc};

#[derive(Clone, Copy, Debug)]
pub(crate) enum GitOperation {
    Stage,
    Unstage,
    Rollback,
    Commit,
    Stash,
    PopStash,
    Fetch,
    Pull,
    Push,
    Checkout,
    CreateBranch,
    MergeBranch,
    DeleteBranch,
}

impl GitOperation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Stage => "stage",
            Self::Unstage => "unstage",
            Self::Rollback => "rollback",
            Self::Commit => "commit",
            Self::Stash => "stash",
            Self::PopStash => "pop_stash",
            Self::Fetch => "fetch",
            Self::Pull => "pull",
            Self::Push => "push",
            Self::Checkout => "checkout",
            Self::CreateBranch => "create_branch",
            Self::MergeBranch => "merge_branch",
            Self::DeleteBranch => "delete_branch",
        }
    }
}

#[derive(Debug, EnumDiscriminants)]
#[strum_discriminants(derive(EnumIter))]
pub(crate) enum VersionControlTelemetryEvent {
    Opened,
    OperationCompleted {
        operation: GitOperation,
        success: bool,
    },
}

impl TelemetryEvent for VersionControlTelemetryEvent {
    fn name(&self) -> &'static str {
        VersionControlTelemetryEventDiscriminants::from(self).name()
    }

    fn payload(&self) -> Option<Value> {
        match self {
            Self::Opened => None,
            Self::OperationCompleted { operation, success } => Some(json!({
                "operation": operation.as_str(),
                "success": success,
            })),
        }
    }

    fn description(&self) -> &'static str {
        VersionControlTelemetryEventDiscriminants::from(self).description()
    }

    fn enablement_state(&self) -> EnablementState {
        VersionControlTelemetryEventDiscriminants::from(self).enablement_state()
    }

    fn contains_ugc(&self) -> bool {
        match self {
            Self::Opened | Self::OperationCompleted { .. } => false,
        }
    }

    fn event_descs() -> impl Iterator<Item = Box<dyn TelemetryEventDesc>> {
        warp_core::telemetry::enum_events::<Self>()
    }
}

impl TelemetryEventDesc for VersionControlTelemetryEventDiscriminants {
    fn name(&self) -> &'static str {
        match self {
            Self::Opened => "VersionControl.Panel.Opened",
            Self::OperationCompleted => "VersionControl.GitOperation.Completed",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Self::Opened => "Opened the Version Control tool panel",
            Self::OperationCompleted => "Completed a Git operation from the Version Control panel",
        }
    }

    fn enablement_state(&self) -> EnablementState {
        match self {
            Self::Opened | Self::OperationCompleted => EnablementState::Always,
        }
    }
}

warp_core::register_telemetry_event!(VersionControlTelemetryEvent);
