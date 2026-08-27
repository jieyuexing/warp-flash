use std::path::Path;
#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const MAX_SESSION_ID_CHARS: usize = 256;
#[cfg(not(target_family = "wasm"))]
const MAX_ACTIVE_SESSIONS_BYTES: u64 = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ExternalCliAgent {
    Codex,
    Grok,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ExternalCliResumeTarget {
    pub(crate) agent: ExternalCliAgent,
    pub(crate) session_id: String,
    pub(crate) cwd: Option<String>,
}

impl ExternalCliResumeTarget {
    pub(crate) fn new(
        agent: ExternalCliAgent,
        session_id: impl Into<String>,
        cwd: Option<String>,
    ) -> Option<Self> {
        let session_id = session_id.into();
        is_safe_session_id(&session_id).then_some(Self {
            agent,
            session_id,
            cwd,
        })
    }

    pub(crate) fn resume_command(&self) -> Option<String> {
        if !is_safe_session_id(&self.session_id) {
            return None;
        }

        Some(match self.agent {
            ExternalCliAgent::Codex => format!("codex resume {}", self.session_id),
            ExternalCliAgent::Grok => format!("grok --resume {}", self.session_id),
        })
    }
}

pub(crate) fn external_cli_agent_from_command(command: &str) -> Option<ExternalCliAgent> {
    let executable = command
        .split_whitespace()
        .next()
        .and_then(|token| Path::new(token).file_name())
        .and_then(|name| name.to_str())?;

    match executable {
        "codex" => Some(ExternalCliAgent::Codex),
        "grok" => Some(ExternalCliAgent::Grok),
        _ => None,
    }
}

fn is_safe_session_id(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_SESSION_ID_CHARS
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:".contains(character))
}

#[cfg(not(target_family = "wasm"))]
pub(crate) fn active_grok_resume_target(
    command: &str,
    cwd: Option<&str>,
) -> Option<ExternalCliResumeTarget> {
    if external_cli_agent_from_command(command) != Some(ExternalCliAgent::Grok) {
        return None;
    }

    let cwd = cwd?.trim();
    if cwd.is_empty() {
        return None;
    }

    let home = dirs::home_dir()?;
    let grok_home = std::env::var_os("GROK_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".grok"));
    active_grok_resume_target_from_file(&grok_home.join("active_sessions.json"), cwd)
}

#[cfg(not(target_family = "wasm"))]
fn active_grok_resume_target_from_file(path: &Path, cwd: &str) -> Option<ExternalCliResumeTarget> {
    use std::fs::File;
    use std::io::Read;

    #[derive(Deserialize)]
    struct ActiveGrokSession {
        session_id: String,
        cwd: String,
    }

    let file = File::open(path).ok()?;
    let mut contents = String::new();
    file.take(MAX_ACTIVE_SESSIONS_BYTES)
        .read_to_string(&mut contents)
        .ok()?;
    let sessions = serde_json::from_str::<Vec<ActiveGrokSession>>(&contents).ok()?;

    let mut matches = sessions
        .into_iter()
        .filter(|session| Path::new(&session.cwd) == Path::new(cwd))
        .filter_map(|session| {
            ExternalCliResumeTarget::new(
                ExternalCliAgent::Grok,
                session.session_id,
                Some(session.cwd),
            )
        });
    let target = matches.next()?;
    matches.next().is_none().then_some(target)
}

#[cfg(all(test, not(target_family = "wasm")))]
pub(super) fn active_grok_resume_target_from_contents(
    contents: &str,
    cwd: &str,
) -> Option<ExternalCliResumeTarget> {
    use std::io::Write;

    let directory = tempfile::tempdir().ok()?;
    let path = directory.path().join("active_sessions.json");
    let mut file = std::fs::File::create(&path).ok()?;
    file.write_all(contents.as_bytes()).ok()?;
    active_grok_resume_target_from_file(&path, cwd)
}
