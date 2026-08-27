#[cfg(not(target_family = "wasm"))]
use std::fs::File;
#[cfg(not(target_family = "wasm"))]
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;
#[cfg(not(target_family = "wasm"))]
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

const MAX_SESSION_ID_CHARS: usize = 256;
#[cfg(not(target_family = "wasm"))]
const MAX_ACTIVE_SESSIONS_BYTES: u64 = 256 * 1024;
#[cfg(not(target_family = "wasm"))]
const MAX_CODEX_SESSION_META_BYTES: u64 = 256 * 1024;

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
pub(crate) fn active_codex_resume_target(
    command: &str,
    cwd: Option<&str>,
) -> Option<ExternalCliResumeTarget> {
    let cwd = cwd?.trim();
    if cwd.is_empty() {
        return None;
    }

    let home = dirs::home_dir()?;
    let codex_home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    active_codex_resume_target_from_root(command, cwd, &codex_home)
}

#[cfg(not(target_family = "wasm"))]
pub(super) fn active_codex_resume_target_from_root(
    command: &str,
    cwd: &str,
    codex_home: &Path,
) -> Option<ExternalCliResumeTarget> {
    if external_cli_agent_from_command(command) != Some(ExternalCliAgent::Codex) {
        return None;
    }

    if let Some(session_id) = explicit_codex_resume_session_id(command) {
        return ExternalCliResumeTarget::new(
            ExternalCliAgent::Codex,
            session_id,
            Some(cwd.to_owned()),
        );
    }

    let sessions_root = codex_home.join("sessions");
    let (_, session_id) = walkdir::WalkDir::new(sessions_root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy();
            name.starts_with("rollout-") && name.ends_with(".jsonl")
        })
        .filter_map(|entry| codex_session_identity(entry.path(), cwd))
        .max_by(|(left_modified, left_id), (right_modified, right_id)| {
            left_modified
                .cmp(right_modified)
                .then_with(|| left_id.cmp(right_id))
        })?;

    ExternalCliResumeTarget::new(ExternalCliAgent::Codex, session_id, Some(cwd.to_owned()))
}

#[cfg(not(target_family = "wasm"))]
fn explicit_codex_resume_session_id(command: &str) -> Option<String> {
    let tokens = shell_words::split(command).ok()?;
    let resume_index = tokens.iter().position(|token| token == "resume")?;
    tokens
        .get(resume_index + 1)
        .filter(|token| !token.starts_with('-'))
        .filter(|token| is_safe_session_id(token))
        .cloned()
}

#[cfg(not(target_family = "wasm"))]
fn codex_session_identity(path: &Path, cwd: &str) -> Option<(SystemTime, String)> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file).take(MAX_CODEX_SESSION_META_BYTES);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).ok()?;
    let envelope = serde_json::from_str::<serde_json::Value>(&first_line).ok()?;
    if envelope.get("type").and_then(serde_json::Value::as_str) != Some("session_meta") {
        return None;
    }
    let payload = envelope.get("payload")?;
    let recorded_cwd = payload.get("cwd").and_then(serde_json::Value::as_str)?;
    if !equivalent_existing_paths(Path::new(recorded_cwd), Path::new(cwd))
        || payload.get("source").and_then(serde_json::Value::as_str) != Some("cli")
    {
        return None;
    }
    let session_id = payload.get("id")?.as_str()?;
    if !is_safe_session_id(session_id) {
        return None;
    }
    let modified = path.metadata().ok()?.modified().ok()?;
    Some((modified, session_id.to_owned()))
}

#[cfg(not(target_family = "wasm"))]
fn equivalent_existing_paths(left: &Path, right: &Path) -> bool {
    left == right
        || left
            .canonicalize()
            .ok()
            .zip(right.canonicalize().ok())
            .is_some_and(|(left, right)| left == right)
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
