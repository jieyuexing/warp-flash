mod config;
mod state;

use std::fmt;

pub use config::*;
pub use state::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    /// The official/first-party stable release.
    Stable,
    /// The official/first-party feature preview release.
    Preview,

    /// The internal-only nightly build.
    Dev,
    /// The internal-only HEAD build.
    Local,

    /// The open-source build of Warp.
    Oss,

    /// The integration test build.
    Integration,
}

impl Channel {
    /// Whether this product channel permits Warp-provided AI features.
    ///
    /// The open-source fork is terminal-only. Keeping this as a channel policy,
    /// rather than a mutable user preference, makes the restriction effective at
    /// every UI and request boundary that consults the shared AI enablement gate.
    pub fn allows_ai(&self) -> bool {
        !matches!(self, Channel::Oss)
    }

    /// Whether this product channel offers account login and account-backed services.
    pub fn allows_account_login(&self) -> bool {
        !matches!(self, Channel::Oss)
    }

    /// Whether or not this channel is for internal use only
    pub fn is_dogfood(&self) -> bool {
        match self {
            Channel::Dev | Channel::Local => true,
            Channel::Stable | Channel::Preview | Channel::Integration | Channel::Oss => false,
        }
    }

    /// Whether this channel honors the `--server-root-url` / `--ws-server-url` /
    /// `--session-sharing-server-url` flags (and their `WARP_*` env-var equivalents).
    ///
    /// Release channels (`Stable`, `Preview`, `Oss`) ignore these overrides so shipped
    /// builds can't be redirected away from their baked-in server URLs. Internal-only channels
    /// (`Dev`, `Local`, `Integration`) continue to honor them for local development and testing.
    pub fn allows_server_url_overrides(&self) -> bool {
        match self {
            Channel::Dev | Channel::Local | Channel::Integration => true,
            Channel::Stable | Channel::Preview | Channel::Oss => false,
        }
    }

    /// Returns the CLI command name corresponding to this channel.
    pub fn cli_command_name(&self) -> &'static str {
        match self {
            Channel::Stable => "oz",
            Channel::Dev => "oz-dev",
            Channel::Preview => "oz-preview",
            Channel::Local => "oz-local",
            Channel::Integration => "oz-integration",
            Channel::Oss => "warp-oss",
        }
    }

    /// Returns the Warp Control CLI command name corresponding to this channel.
    pub fn warpctrl_command_name(&self) -> &'static str {
        match self {
            Channel::Stable => "warpctrl",
            Channel::Dev => "warpctrl-dev",
            Channel::Preview => "warpctrl-preview",
            Channel::Local => "warpctrl-local",
            Channel::Integration => "warpctrl-integration",
            Channel::Oss => "warpctrl-oss",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Channel;

    #[test]
    fn oss_is_terminal_only() {
        assert!(!Channel::Oss.allows_ai());
        assert!(!Channel::Oss.allows_account_login());
        assert!(Channel::Stable.allows_ai());
        assert!(Channel::Stable.allows_account_login());
        assert!(Channel::Preview.allows_ai());
        assert!(Channel::Dev.allows_ai());
        assert!(Channel::Local.allows_ai());
        assert!(Channel::Integration.allows_ai());
    }
}

impl fmt::Display for Channel {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match self {
            Channel::Stable => "stable",
            Channel::Preview => "preview",
            Channel::Dev => "dev",
            Channel::Integration => "integration",
            Channel::Local => "local",
            Channel::Oss => "warp-oss",
        })
    }
}
