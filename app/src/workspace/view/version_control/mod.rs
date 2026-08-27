pub mod model;
mod telemetry;
mod view;

pub(crate) use telemetry::{GitOperation, VersionControlTelemetryEvent};
pub use view::VersionControlView;
