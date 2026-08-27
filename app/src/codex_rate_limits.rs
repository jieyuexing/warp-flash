use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Local, Utc};
use command::r#async::Command;
use futures::future::{Either, select};
use futures_lite::StreamExt;
use futures_lite::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use warpui::r#async::Timer;
use warpui::{Entity, ModelContext, SingletonEntity};

const REFRESH_INTERVAL: Duration = Duration::from_secs(5 * 60);
const QUERY_TIMEOUT: Duration = Duration::from_secs(15);

const INITIALIZE_REQUEST: &str = concat!(
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"warp-oss","version":"0.1"}}}"#,
    "\n"
);
const INITIALIZED_NOTIFICATION: &str = concat!(
    r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#,
    "\n"
);
const RATE_LIMITS_REQUEST: &str = concat!(
    r#"{"jsonrpc":"2.0","id":2,"method":"account/rateLimits/read","params":{}}"#,
    "\n"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CodexQuotaSnapshot {
    remaining_percent: u8,
    resets_at: Option<i64>,
    window_duration_mins: Option<i64>,
}

impl CodexQuotaSnapshot {
    pub(crate) fn remaining_percent(self) -> u8 {
        self.remaining_percent
    }

    #[allow(dead_code)]
    pub(crate) fn resets_at(self) -> Option<i64> {
        self.resets_at
    }

    #[allow(dead_code)]
    pub(crate) fn window_duration_mins(self) -> Option<i64> {
        self.window_duration_mins
    }
}

pub(crate) struct CodexRateLimitsModel {
    state: CodexQuotaState,
    started: bool,
    refresh_in_flight: bool,
    refresh_schedule_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodexQuotaState {
    Loading,
    Available(CodexQuotaSnapshot),
    Unavailable,
}

impl CodexQuotaState {
    fn label(self) -> String {
        match self {
            Self::Loading => "Codex …".to_owned(),
            Self::Available(snapshot) => format!("Codex {}%", snapshot.remaining_percent()),
            Self::Unavailable => "Codex --".to_owned(),
        }
    }

    fn detail_label(self) -> String {
        match self {
            Self::Loading => "Local Codex quota loading".to_owned(),
            Self::Available(snapshot) => {
                let reset = snapshot
                    .resets_at()
                    .and_then(format_reset_epoch)
                    .map(|reset| format!(" · resets {reset}"))
                    .unwrap_or_default();
                format!(
                    "Local Codex · {}% remaining{reset} · click to refresh",
                    snapshot.remaining_percent()
                )
            }
            Self::Unavailable => "Local Codex quota unavailable · click to retry".to_owned(),
        }
    }
}

impl CodexRateLimitsModel {
    pub(crate) fn new() -> Self {
        Self {
            state: CodexQuotaState::Loading,
            started: false,
            refresh_in_flight: false,
            refresh_schedule_generation: 0,
        }
    }

    pub(crate) fn ensure_started(&mut self, ctx: &mut ModelContext<Self>) {
        if self.started {
            return;
        }
        self.started = true;
        self.refresh_now(ctx);
    }

    pub(crate) fn label(&self) -> String {
        self.state.label()
    }

    pub(crate) fn detail_label(&self) -> String {
        self.state.detail_label()
    }

    pub(crate) fn refresh_now(&mut self, ctx: &mut ModelContext<Self>) {
        self.start_refresh(true, ctx);
    }

    fn start_refresh(&mut self, show_loading: bool, ctx: &mut ModelContext<Self>) {
        if self.refresh_in_flight {
            return;
        }

        // Invalidate the previously scheduled timer. Its callback is kept
        // intentionally cheap and will exit without issuing a request.
        self.refresh_schedule_generation = self.refresh_schedule_generation.wrapping_add(1);
        self.refresh_in_flight = true;
        if show_loading && self.state != CodexQuotaState::Loading {
            self.state = CodexQuotaState::Loading;
            ctx.notify();
            ctx.emit(CodexRateLimitsEvent::Updated);
        }

        let _ = ctx.spawn(query_rate_limits_with_timeout(), |model, result, ctx| {
            model.refresh_in_flight = false;
            let next_state = match result {
                Ok(snapshot) => CodexQuotaState::Available(snapshot),
                Err(error) => {
                    if !matches!(model.state, CodexQuotaState::Unavailable) {
                        log::warn!("Codex rate-limit query unavailable: {error:#}");
                    } else {
                        log::debug!("Codex rate-limit query still unavailable: {error:#}");
                    }
                    CodexQuotaState::Unavailable
                }
            };

            if model.state != next_state {
                model.state = next_state;
                ctx.notify();
                ctx.emit(CodexRateLimitsEvent::Updated);
            }

            model.schedule_refresh(ctx);
        });
    }

    fn schedule_refresh(&mut self, ctx: &mut ModelContext<Self>) {
        self.refresh_schedule_generation = self.refresh_schedule_generation.wrapping_add(1);
        let generation = self.refresh_schedule_generation;
        let _ = ctx.spawn(Timer::after(REFRESH_INTERVAL), move |model, _, ctx| {
            if model.refresh_schedule_generation == generation {
                model.start_refresh(false, ctx);
            }
        });
    }
}

fn format_reset_epoch(resets_at: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp(resets_at, 0).map(|reset| {
        reset
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M %Z")
            .to_string()
    })
}

#[derive(Debug)]
pub(crate) enum CodexRateLimitsEvent {
    Updated,
}

impl Entity for CodexRateLimitsModel {
    type Event = CodexRateLimitsEvent;
}

impl SingletonEntity for CodexRateLimitsModel {}

async fn query_rate_limits_with_timeout() -> Result<CodexQuotaSnapshot> {
    let query = Box::pin(query_rate_limits());
    let timeout = Box::pin(Timer::after(QUERY_TIMEOUT));
    match select(query, timeout).await {
        Either::Left((result, _)) => result,
        Either::Right((_, _)) => Err(anyhow!("Codex rate-limit query timed out")),
    }
}

async fn query_rate_limits() -> Result<CodexQuotaSnapshot> {
    let program = codex_program();
    let mut command = Command::new(&program);
    command
        .args(["app-server", "--stdio"])
        .env("PATH", codex_path_env(&program))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    let mut child = command
        .spawn()
        .context("failed to start Codex app-server")?;
    let mut stdin = child
        .stdin
        .take()
        .context("Codex app-server stdin missing")?;
    let stdout = child
        .stdout
        .take()
        .context("Codex app-server stdout missing")?;
    let mut lines = BufReader::new(stdout).lines();

    stdin.write_all(INITIALIZE_REQUEST.as_bytes()).await?;
    stdin.flush().await?;
    read_response_with_id::<serde_json::Value, _>(&mut lines, 1).await?;

    stdin.write_all(INITIALIZED_NOTIFICATION.as_bytes()).await?;
    stdin.write_all(RATE_LIMITS_REQUEST.as_bytes()).await?;
    stdin.flush().await?;

    let response = read_response_with_id::<RateLimitsReadResponse, _>(&mut lines, 2).await?;
    let snapshot = snapshot_from_response(response)?;

    let _ = child.kill();
    let _ = child.status().await;
    Ok(snapshot)
}

async fn read_response_with_id<T: DeserializeOwned, R: futures_lite::io::AsyncRead + Unpin>(
    lines: &mut futures_lite::io::Lines<BufReader<R>>,
    expected_id: u64,
) -> Result<T> {
    while let Some(line) = lines.next().await {
        let line = line.context("failed reading Codex app-server response")?;
        let Ok(envelope) = serde_json::from_str::<ResponseId>(&line) else {
            continue;
        };
        if envelope.id == Some(expected_id) {
            return serde_json::from_str(&line).context("invalid Codex app-server response");
        }
    }
    Err(anyhow!("Codex app-server closed before responding"))
}

fn codex_program() -> PathBuf {
    let path = std::env::var_os("PATH");
    let home = dirs::home_dir();
    codex_program_from(path.as_deref(), home.as_deref())
}

fn codex_program_from(path: Option<&OsStr>, home: Option<&Path>) -> PathBuf {
    let executable = if cfg!(windows) { "codex.exe" } else { "codex" };
    if let Some(path) = path.and_then(|path| {
        std::env::split_paths(&path)
            .map(|directory| directory.join(executable))
            .find(|candidate| candidate.is_file())
    }) {
        return path;
    }

    #[cfg(target_os = "macos")]
    if let Some(candidate) = home.map(|home| home.join(".local/bin").join(executable))
        && candidate.is_file()
    {
        return candidate;
    }

    #[cfg(target_os = "macos")]
    for candidate in ["/opt/homebrew/bin/codex", "/usr/local/bin/codex"] {
        let candidate = PathBuf::from(candidate);
        if candidate.is_file() {
            return candidate;
        }
    }

    PathBuf::from(executable)
}

fn codex_path_env(program: &Path) -> OsString {
    let mut directories = Vec::new();
    if let Some(parent) = program
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        directories.push(parent.to_path_buf());
    }

    #[cfg(target_os = "macos")]
    directories.extend(
        ["/opt/homebrew/bin", "/usr/local/bin"]
            .into_iter()
            .map(PathBuf::from),
    );

    if let Some(path) = std::env::var_os("PATH") {
        directories.extend(std::env::split_paths(&path));
    }

    std::env::join_paths(directories).unwrap_or_else(|_| {
        std::env::var_os("PATH").unwrap_or_else(|| OsString::from("/usr/bin:/bin"))
    })
}

fn snapshot_from_response(response: RateLimitsReadResponse) -> Result<CodexQuotaSnapshot> {
    if let Some(error) = response.error {
        return Err(anyhow!("Codex app-server error: {}", error.message));
    }

    let rate_limits = response
        .result
        .context("Codex rate-limit response had no result")?
        .rate_limits;
    let window = [rate_limits.primary, rate_limits.secondary]
        .into_iter()
        .flatten()
        .min_by_key(|window| 100_i32.saturating_sub(window.used_percent.clamp(0, 100)))
        .context("Codex rate-limit response had no active window")?;

    Ok(CodexQuotaSnapshot {
        remaining_percent: 100_i32.saturating_sub(window.used_percent.clamp(0, 100)) as u8,
        resets_at: window.resets_at,
        window_duration_mins: window.window_duration_mins,
    })
}

#[derive(Deserialize)]
struct ResponseId {
    id: Option<u64>,
}

#[derive(Deserialize)]
struct RateLimitsReadResponse {
    result: Option<GetAccountRateLimitsResponse>,
    error: Option<RpcError>,
}

#[derive(Deserialize)]
struct RpcError {
    message: String,
}

#[derive(Deserialize)]
struct GetAccountRateLimitsResponse {
    #[serde(rename = "rateLimits")]
    rate_limits: RateLimitSnapshot,
}

#[derive(Deserialize)]
struct RateLimitSnapshot {
    primary: Option<RateLimitWindow>,
    secondary: Option<RateLimitWindow>,
}

#[derive(Deserialize)]
struct RateLimitWindow {
    #[serde(rename = "usedPercent")]
    used_percent: i32,
    #[serde(rename = "resetsAt")]
    resets_at: Option<i64>,
    #[serde(rename = "windowDurationMins")]
    window_duration_mins: Option<i64>,
}

#[cfg(test)]
#[path = "codex_rate_limits_tests.rs"]
mod tests;
