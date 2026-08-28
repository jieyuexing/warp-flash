#[cfg(not(target_family = "wasm"))]
use std::fs::File;
#[cfg(not(target_family = "wasm"))]
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
#[cfg(not(target_family = "wasm"))]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
use command::blocking::Command;
use serde::{Deserialize, Serialize};

const MAX_SESSION_ID_CHARS: usize = 256;
#[cfg(not(target_family = "wasm"))]
const MAX_ACTIVE_SESSIONS_BYTES: u64 = 256 * 1024;
#[cfg(not(target_family = "wasm"))]
const MAX_SESSION_META_BYTES: u64 = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    process_group_id: Option<u32>,
) -> Option<ExternalCliResumeTarget> {
    let cwd = cwd?.trim();
    if cwd.is_empty() {
        return None;
    }

    let home = dirs::home_dir()?;
    let codex_home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    let open_session_paths = process_group_id
        .map(open_files_for_process_group)
        .unwrap_or_default();
    active_codex_resume_target_from_root(command, cwd, &codex_home, &open_session_paths)
}

#[cfg(not(target_family = "wasm"))]
pub(super) fn active_codex_resume_target_from_root(
    command: &str,
    cwd: &str,
    codex_home: &Path,
    open_session_paths: &[PathBuf],
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

    codex_resume_target_from_open_session_paths(cwd, codex_home, open_session_paths)
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
fn equivalent_existing_paths(left: &Path, right: &Path) -> bool {
    left == right
        || left
            .canonicalize()
            .ok()
            .zip(right.canonicalize().ok())
            .is_some_and(|(left, right)| left == right)
}

#[cfg(not(target_family = "wasm"))]
fn codex_resume_target_from_open_session_paths(
    cwd: &str,
    codex_home: &Path,
    open_session_paths: &[PathBuf],
) -> Option<ExternalCliResumeTarget> {
    let sessions_root = codex_home.join("sessions").canonicalize().ok()?;
    let mut targets = open_session_paths
        .iter()
        .filter_map(|path| path.canonicalize().ok())
        .filter(|path| path.starts_with(&sessions_root))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "jsonl")
        })
        .filter_map(|path| codex_resume_target_from_session_file(&path, cwd));

    let target = targets.next()?;
    targets
        .all(|candidate| candidate.session_id == target.session_id)
        .then_some(target)
}

#[cfg(not(target_family = "wasm"))]
fn codex_resume_target_from_session_file(
    path: &Path,
    cwd: &str,
) -> Option<ExternalCliResumeTarget> {
    #[derive(Deserialize)]
    struct SessionMetaRecord {
        #[serde(rename = "type")]
        record_type: String,
        payload: SessionMetaPayload,
    }

    #[derive(Deserialize)]
    struct SessionMetaPayload {
        id: String,
        cwd: String,
        source: serde_json::Value,
    }

    let file = File::open(path).ok()?;
    let mut line = String::new();
    BufReader::new(file.take(MAX_SESSION_META_BYTES))
        .read_line(&mut line)
        .ok()?;
    let record = serde_json::from_str::<SessionMetaRecord>(&line).ok()?;
    if record.record_type != "session_meta"
        || record.payload.source.as_str() != Some("cli")
        || !equivalent_existing_paths(Path::new(&record.payload.cwd), Path::new(cwd))
    {
        return None;
    }

    ExternalCliResumeTarget::new(
        ExternalCliAgent::Codex,
        record.payload.id,
        Some(cwd.to_owned()),
    )
}

#[cfg(target_os = "macos")]
fn open_files_for_process_group(process_group_id: u32) -> Vec<PathBuf> {
    let Ok(output) = Command::new("/usr/sbin/lsof")
        .args(["-a", "-g", &process_group_id.to_string(), "-Fn"])
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix('n'))
        .map(PathBuf::from)
        .collect()
}

#[cfg(target_os = "linux")]
fn open_files_for_process_group(process_group_id: u32) -> Vec<PathBuf> {
    let Ok(processes) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };

    processes
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok()?.parse::<u32>().ok())
        .filter(|process_id| linux_process_group_id(*process_id) == Some(process_group_id))
        .flat_map(|process_id| {
            std::fs::read_dir(format!("/proc/{process_id}/fd"))
                .into_iter()
                .flatten()
                .flatten()
        })
        .filter_map(|entry| std::fs::read_link(entry.path()).ok())
        .collect()
}

#[cfg(target_os = "linux")]
fn linux_process_group_id(process_id: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{process_id}/stat")).ok()?;
    let fields = stat.rsplit_once(") ")?.1.split_whitespace();
    fields.skip(2).next()?.parse().ok()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn open_files_for_process_group(_process_group_id: u32) -> Vec<PathBuf> {
    Vec::new()
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
        .filter(|session| equivalent_existing_paths(Path::new(&session.cwd), Path::new(cwd)))
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
