use std::path::Path;

use super::CLIAgent;

/// Local, deterministic title generation for terminal sessions.
///
/// Explicit tab renames are applied outside this generator and therefore keep
/// precedence over generated titles. Within generated titles, a detected CLI
/// agent is more useful than a shell-provided directory title.
pub(crate) struct TerminalTitleGenerator<'a> {
    pub cli_agent: Option<CLIAgent>,
    pub task_title: Option<&'a str>,
    pub project: Option<&'a str>,
    pub working_directory: Option<&'a str>,
    pub osc_title: Option<&'a str>,
    pub shell_title: &'a str,
}

impl TerminalTitleGenerator<'_> {
    pub fn generate(&self) -> String {
        if let Some(agent) = self.cli_agent {
            if let Some(task_title) = self.task_title.and_then(normalized_prompt_title) {
                return format!("{} · {task_title}", agent.display_name());
            }
            let context = self
                .project
                .and_then(normalized_leaf_name)
                .or_else(|| self.working_directory.and_then(normalized_leaf_name));
            return match context {
                Some(context) => format!("{} · {context}", agent.display_name()),
                None => agent.display_name().to_owned(),
            };
        }

        self.osc_title
            .map(str::trim)
            .filter(|title| !title.is_empty())
            .map(str::to_owned)
            .or_else(|| self.working_directory.and_then(normalized_leaf_name))
            .unwrap_or_else(|| self.shell_title.to_owned())
    }
}

const MAX_PROMPT_TITLE_CHARS: usize = 48;

/// Converts the first user prompt into a compact, single-line terminal title.
/// This is deliberately local and deterministic: no Warp AI service is used.
pub(crate) fn normalized_prompt_title(value: &str) -> Option<String> {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        return None;
    }

    let mut chars = compact.chars();
    let title: String = chars.by_ref().take(MAX_PROMPT_TITLE_CHARS).collect();
    Some(if chars.next().is_some() {
        format!("{title}…")
    } else {
        title
    })
}

pub(crate) fn cli_agent_from_command(command: &str) -> Option<CLIAgent> {
    let executable = command
        .split_whitespace()
        .next()
        .and_then(|token| Path::new(token).file_name())
        .and_then(|name| name.to_str())?;

    match executable {
        "claude" => Some(CLIAgent::Claude),
        "gemini" => Some(CLIAgent::Gemini),
        "codex" => Some(CLIAgent::Codex),
        "amp" => Some(CLIAgent::Amp),
        "droid" => Some(CLIAgent::Droid),
        "opencode" => Some(CLIAgent::OpenCode),
        "copilot" => Some(CLIAgent::Copilot),
        "pi" => Some(CLIAgent::Pi),
        "auggie" => Some(CLIAgent::Auggie),
        "agent" => Some(CLIAgent::CursorCli),
        "goose" => Some(CLIAgent::Goose),
        "hermes" => Some(CLIAgent::Hermes),
        "vibe" => Some(CLIAgent::Vibe),
        "agy" => Some(CLIAgent::Antigravity),
        _ => None,
    }
}

fn normalized_leaf_name(value: &str) -> Option<String> {
    let value = value.trim().trim_end_matches(['/', '\\']);
    if value.is_empty() {
        return None;
    }

    Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .or_else(|| (value == "~").then(|| "~".to_owned()))
}

pub(crate) fn is_contextual_directory_title(title: &str, working_directory: Option<&str>) -> bool {
    let title = title.trim();
    !title.is_empty()
        && working_directory
            .and_then(normalized_leaf_name)
            .is_some_and(|directory| directory == title)
}

#[cfg(test)]
#[path = "title_generator_tests.rs"]
mod tests;
