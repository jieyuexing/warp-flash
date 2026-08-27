use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;
use walkdir::WalkDir;
use warp_core::features::FeatureFlag;
use warp_core::ui::theme::color::internal_colors;
use warpui::elements::{
    Container, CornerRadius, CrossAxisAlignment, Element, Flex, Hoverable, MainAxisSize,
    MouseStateHandle, Padding, ParentElement, Radius, Shrinkable, Text,
};
use warpui::fonts::{Properties, Weight};
use warpui::platform::Cursor;
use warpui::{AppContext, Entity, ModelContext, SingletonEntity};

use crate::appearance::Appearance;
use crate::external_cli_resume::{ExternalCliAgent, ExternalCliResumeTarget};
use crate::workspace::{Workspace, WorkspaceAction};

const DEFAULT_LIMIT_PER_GROUP: usize = 30;
const MAX_JSON_BYTES: u64 = 256 * 1024;
const MAX_CODEX_TITLE_LINES: usize = 80;
const MAX_TITLE_CHARS: usize = 120;
const MAX_SESSION_ID_CHARS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExternalSessionGroup {
    Codex,
    Grok,
}

impl ExternalSessionGroup {
    fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Grok => "Grok",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Grok => "grok",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExternalSessionTab {
    pub(crate) group: ExternalSessionGroup,
    pub(crate) session_id: String,
    pub(crate) title: String,
    pub(crate) cwd: Option<PathBuf>,
    pub(crate) updated_at: SystemTime,
    pub(crate) archived: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExternalSessionProjection {
    pub(crate) codex: Vec<ExternalSessionTab>,
    pub(crate) grok: Vec<ExternalSessionTab>,
}

impl ExternalSessionProjection {
    fn for_workspace(&self, workspace_cwd: Option<&Path>, limit: usize) -> Self {
        Self {
            codex: project_group(&self.codex, workspace_cwd, limit),
            grok: project_group(&self.grok, workspace_cwd, limit),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExternalSessionIndexState {
    Loading,
    Ready,
    Unavailable,
}

pub(crate) struct ExternalSessionIndexModel {
    state: ExternalSessionIndexState,
    sessions: ExternalSessionProjection,
    started: bool,
    mouse_states: Mutex<HashMap<String, MouseStateHandle>>,
}

impl ExternalSessionIndexModel {
    pub(crate) fn new() -> Self {
        Self {
            state: ExternalSessionIndexState::Loading,
            sessions: ExternalSessionProjection::default(),
            started: false,
            mouse_states: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn ensure_started(&mut self, ctx: &mut ModelContext<Self>) {
        if self.started {
            return;
        }
        self.started = true;

        let roots = external_session_roots();
        let _ = ctx.spawn(
            async move {
                match roots {
                    Some((codex_home, grok_home)) => tokio::task::spawn_blocking(move || {
                        index_external_sessions(&codex_home, &grok_home)
                    })
                    .await
                    .ok(),
                    None => None,
                }
            },
            |model, sessions, ctx| {
                match sessions {
                    Some(sessions) => {
                        model.sessions = sessions;
                        model.state = ExternalSessionIndexState::Ready;
                    }
                    None => model.state = ExternalSessionIndexState::Unavailable,
                }
                ctx.notify();
                ctx.emit(ExternalSessionIndexEvent::Updated);
            },
        );
    }

    fn projection(&self, workspace_cwd: Option<&Path>, limit: usize) -> ExternalSessionProjection {
        self.sessions.for_workspace(workspace_cwd, limit)
    }

    fn mouse_state(&self, group: ExternalSessionGroup, session_id: &str) -> MouseStateHandle {
        let key = format!("{}:{session_id}", group.key());
        self.mouse_states
            .lock()
            .expect("external session mouse-state lock poisoned")
            .entry(key)
            .or_default()
            .clone()
    }
}

#[derive(Debug)]
pub(crate) enum ExternalSessionIndexEvent {
    Updated,
}

impl Entity for ExternalSessionIndexModel {
    type Event = ExternalSessionIndexEvent;
}

impl SingletonEntity for ExternalSessionIndexModel {}

fn external_session_roots() -> Option<(PathBuf, PathBuf)> {
    let home = dirs::home_dir()?;
    let codex_home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"));
    let grok_home = std::env::var_os("GROK_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".grok"));
    Some((codex_home, grok_home))
}

fn index_external_sessions(codex_home: &Path, grok_home: &Path) -> ExternalSessionProjection {
    ExternalSessionProjection {
        codex: index_codex_sessions(codex_home),
        grok: index_grok_sessions(grok_home),
    }
}

fn index_codex_sessions(codex_home: &Path) -> Vec<ExternalSessionTab> {
    let active_root = codex_home.join("sessions");
    if !active_root.is_dir() {
        return Vec::new();
    }

    let mut sessions = WalkDir::new(active_root)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy();
            name.starts_with("rollout-") && name.ends_with(".jsonl")
        })
        .filter_map(|entry| codex_session_from_path(entry.path()))
        .collect::<Vec<_>>();
    sort_by_updated_at(&mut sessions);
    sessions
}

fn codex_session_from_path(path: &Path) -> Option<ExternalSessionTab> {
    let metadata = path.metadata().ok()?;
    let updated_at = metadata.modified().ok()?;
    let first_line = read_first_line_bounded(path)?;
    let envelope = serde_json::from_str::<Value>(&first_line).ok()?;
    if envelope.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    let payload = envelope.get("payload")?.as_object()?;
    let session_id = safe_session_id(payload.get("id")?.as_str()?)?;
    let cwd = payload
        .get("cwd")
        .and_then(Value::as_str)
        .filter(|cwd| !cwd.is_empty())
        .map(PathBuf::from);
    let title = codex_title(path, &session_id);

    Some(ExternalSessionTab {
        group: ExternalSessionGroup::Codex,
        session_id,
        title,
        cwd,
        updated_at,
        archived: false,
    })
}

fn read_first_line_bounded(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file).take(MAX_JSON_BYTES);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).ok()?;
    (!first_line.is_empty()).then_some(first_line)
}

fn codex_title(path: &Path, session_id: &str) -> String {
    let Some(file) = File::open(path).ok() else {
        return short_session_id(session_id);
    };
    let reader = BufReader::new(file).take(MAX_JSON_BYTES);
    for line in reader
        .lines()
        .map_while(Result::ok)
        .take(MAX_CODEX_TITLE_LINES)
    {
        let Ok(envelope) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(text) = extract_codex_user_text(&envelope) {
            return bounded_single_line(&text, MAX_TITLE_CHARS);
        }
    }
    short_session_id(session_id)
}

fn extract_codex_user_text(envelope: &Value) -> Option<String> {
    let payload = envelope.get("payload").unwrap_or(envelope);
    let is_user = payload.get("role").and_then(Value::as_str) == Some("user")
        || payload.get("type").and_then(Value::as_str) == Some("user_message");
    if !is_user {
        return None;
    }

    for key in ["message", "text", "content"] {
        let Some(value) = payload.get(key) else {
            continue;
        };
        if let Some(text) = extract_text(value) {
            return Some(text);
        }
    }
    None
}

fn extract_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_owned()),
        Value::Array(items) => {
            let joined = items
                .iter()
                .filter_map(|item| {
                    let kind = item.get("type").and_then(Value::as_str);
                    matches!(kind, Some("input_text" | "text"))
                        .then(|| item.get("text").and_then(Value::as_str))
                        .flatten()
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                })
                .collect::<Vec<_>>()
                .join(" ");
            (!joined.is_empty()).then_some(joined)
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Object(_) => {
            None
        }
    }
}

fn index_grok_sessions(grok_home: &Path) -> Vec<ExternalSessionTab> {
    let sessions_root = grok_home.join("sessions");
    if !sessions_root.is_dir() {
        return Vec::new();
    }

    let mut sessions = WalkDir::new(sessions_root)
        .min_depth(3)
        .max_depth(3)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && entry.file_name() == "summary.json")
        .filter_map(|entry| grok_session_from_summary(entry.path()))
        .filter(|session| !session.archived)
        .collect::<Vec<_>>();
    sort_by_updated_at(&mut sessions);
    sessions
}

fn grok_session_from_summary(path: &Path) -> Option<ExternalSessionTab> {
    let summary = read_json_bounded(path)?;
    let info = summary.get("info");
    let archived = summary
        .get("archived")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || info
            .and_then(|info| info.get("archived"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let session_id = safe_session_id(path.parent()?.file_name()?.to_str()?)?;
    let encoded_cwd = path.parent()?.parent()?.file_name()?.to_str()?;
    let decoded_cwd = urlencoding::decode(encoded_cwd).ok()?.into_owned();
    let cwd = summary
        .get("git_root_dir")
        .and_then(Value::as_str)
        .filter(|cwd| !cwd.is_empty())
        .unwrap_or(&decoded_cwd);
    let title = ["session_summary", "generated_title"]
        .into_iter()
        .find_map(|key| summary.get(key).and_then(Value::as_str))
        .filter(|title| !title.trim().is_empty())
        .map(|title| bounded_single_line(title, MAX_TITLE_CHARS))
        .unwrap_or_else(|| short_session_id(&session_id));
    let updated_at = ["updated_at", "last_active_at"]
        .into_iter()
        .find_map(|key| summary.get(key).and_then(Value::as_str))
        .and_then(parse_timestamp)
        .or_else(|| path.metadata().ok()?.modified().ok())?;

    Some(ExternalSessionTab {
        group: ExternalSessionGroup::Grok,
        session_id,
        title,
        cwd: Some(PathBuf::from(cwd)),
        updated_at,
        archived,
    })
}

fn read_json_bounded(path: &Path) -> Option<Value> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file).take(MAX_JSON_BYTES);
    let mut contents = String::new();
    reader.read_to_string(&mut contents).ok()?;
    serde_json::from_str(&contents).ok()
}

fn parse_timestamp(timestamp: &str) -> Option<SystemTime> {
    DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Utc).into())
}

fn safe_session_id(session_id: &str) -> Option<String> {
    let session_id = session_id.trim();
    (!session_id.is_empty()
        && session_id.chars().count() <= MAX_SESSION_ID_CHARS
        && session_id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.:".contains(character)))
    .then(|| session_id.to_owned())
}

fn bounded_single_line(text: &str, max_chars: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

fn short_session_id(session_id: &str) -> String {
    session_id.chars().take(13).collect()
}

fn sort_by_updated_at(sessions: &mut [ExternalSessionTab]) {
    sessions.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
}

fn project_group(
    sessions: &[ExternalSessionTab],
    workspace_cwd: Option<&Path>,
    limit: usize,
) -> Vec<ExternalSessionTab> {
    let mut projected = sessions
        .iter()
        .filter(|session| !session.archived)
        .cloned()
        .collect::<Vec<_>>();
    projected.sort_by(|left, right| compare_for_workspace(left, right, workspace_cwd));
    projected.truncate(limit);
    projected
}

fn compare_for_workspace(
    left: &ExternalSessionTab,
    right: &ExternalSessionTab,
    workspace_cwd: Option<&Path>,
) -> Ordering {
    let left_matches =
        workspace_cwd.is_some_and(|workspace| session_matches_workspace(left, workspace));
    let right_matches =
        workspace_cwd.is_some_and(|workspace| session_matches_workspace(right, workspace));
    right_matches
        .cmp(&left_matches)
        .then_with(|| right.updated_at.cmp(&left.updated_at))
        .then_with(|| left.session_id.cmp(&right.session_id))
}

fn session_matches_workspace(session: &ExternalSessionTab, workspace_cwd: &Path) -> bool {
    session.cwd.as_deref().is_some_and(|cwd| {
        cwd == workspace_cwd || cwd.starts_with(workspace_cwd) || workspace_cwd.starts_with(cwd)
    })
}

fn workspace_cwd(workspace: &Workspace, app: &AppContext) -> Option<PathBuf> {
    workspace
        .active_tab_pane_group()
        .as_ref(app)
        .active_session_view(app)
        .and_then(|view| {
            let view = view.as_ref(app);
            view.pwd_if_local(app).or_else(|| view.pwd())
        })
        .map(PathBuf::from)
}

pub(super) fn render_external_session_tabs(
    workspace: &Workspace,
    app: &AppContext,
) -> Box<dyn Element> {
    debug_assert!(FeatureFlag::WarpossExternalSessionTabs.is_enabled());
    let model = ExternalSessionIndexModel::as_ref(app);
    let workspace_cwd = workspace_cwd(workspace, app);
    let projection = model.projection(workspace_cwd.as_deref(), DEFAULT_LIMIT_PER_GROUP);

    let mut groups = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);
    groups.add_child(render_group(
        ExternalSessionGroup::Codex,
        &projection.codex,
        model,
        app,
    ));
    groups.add_child(render_group(
        ExternalSessionGroup::Grok,
        &projection.grok,
        model,
        app,
    ));
    Container::new(groups.finish())
        .with_padding(Padding::uniform(8.))
        .finish()
}

fn render_group(
    group: ExternalSessionGroup,
    sessions: &[ExternalSessionTab],
    model: &ExternalSessionIndexModel,
    app: &AppContext,
) -> Box<dyn Element> {
    let appearance = Appearance::as_ref(app);
    let theme = appearance.theme();
    let mut list = Flex::column()
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_spacing(4.);
    list.add_child(
        Text::new_inline(
            group.label(),
            appearance.ui_font_family(),
            appearance.ui_font_size(),
        )
        .with_color(theme.main_text_color(theme.background()).into())
        .with_style(Properties::default().weight(Weight::Bold))
        .finish(),
    );

    if sessions.is_empty() {
        let label = match model.state {
            ExternalSessionIndexState::Loading => "Indexing local sessions…",
            ExternalSessionIndexState::Ready => "No active sessions",
            ExternalSessionIndexState::Unavailable => "Session index unavailable",
        };
        list.add_child(
            Text::new_inline(
                label,
                appearance.ui_font_family(),
                appearance.ui_font_size() - 2.,
            )
            .with_color(theme.sub_text_color(theme.background()).into())
            .finish(),
        );
    } else {
        for session in sessions {
            list.add_child(render_session_tab(session, model, app));
        }
    }
    Container::new(list.finish())
        .with_padding(Padding::uniform(0.).with_bottom(8.))
        .finish()
}

fn render_session_tab(
    session: &ExternalSessionTab,
    model: &ExternalSessionIndexModel,
    app: &AppContext,
) -> Box<dyn Element> {
    let appearance = Appearance::as_ref(app);
    let theme = appearance.theme();
    let main_color = theme.main_text_color(theme.background());
    let sub_color = theme.sub_text_color(theme.background());
    let font = appearance.ui_font_family();
    let font_size = appearance.ui_font_size();
    let cwd = session
        .cwd
        .as_deref()
        .map(Path::display)
        .map(|cwd| cwd.to_string())
        .unwrap_or_else(|| "—".to_owned());
    let updated_at =
        DateTime::<Utc>::from(session.updated_at).to_rfc3339_opts(SecondsFormat::Secs, true);

    let content = Flex::column()
        .with_main_axis_size(MainAxisSize::Min)
        .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
        .with_child(
            Shrinkable::new(
                1.,
                Text::new_inline(session.title.clone(), font.clone(), font_size)
                    .with_color(main_color.into())
                    .with_style(Properties::default().weight(Weight::Medium))
                    .finish(),
            )
            .finish(),
        )
        .with_child(
            Text::new_inline(
                warp_i18n::localize_format!("ID: {id}", id = session.session_id),
                font.clone(),
                font_size - 2.,
            )
            .with_color(sub_color.into())
            .finish(),
        )
        .with_child(
            Text::new_inline(
                warp_i18n::localize_format!("Working directory: {cwd}", cwd = cwd),
                font.clone(),
                font_size - 2.,
            )
            .with_color(sub_color.into())
            .finish(),
        )
        .with_child(
            Text::new_inline(
                warp_i18n::localize_format!("Updated at: {updated_at}", updated_at = updated_at),
                font,
                font_size - 2.,
            )
            .with_color(sub_color.into())
            .finish(),
        )
        .finish();

    let mouse_state = model.mouse_state(session.group, &session.session_id);
    let session_id = session.session_id.clone();
    let group = session.group;
    Hoverable::new(mouse_state, move |state| {
        let mut container = Container::new(content)
            .with_horizontal_padding(8.)
            .with_padding_top(6.)
            .with_padding_bottom(6.)
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)));
        if state.is_hovered() {
            container = container.with_background(internal_colors::fg_overlay_2(theme));
        }
        container.finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(move |ctx, _, _| {
        ctx.dispatch_typed_action(activation_action(group, &session_id));
    })
    .finish()
}

fn activation_action(group: ExternalSessionGroup, session_id: &str) -> WorkspaceAction {
    let agent = match group {
        ExternalSessionGroup::Codex => ExternalCliAgent::Codex,
        ExternalSessionGroup::Grok => ExternalCliAgent::Grok,
    };
    let content = ExternalCliResumeTarget::new(agent, session_id, None)
        .and_then(|target| target.resume_command())
        .unwrap_or_default();
    WorkspaceAction::InsertInInput {
        content,
        replace_buffer: true,
        ensure_agent_mode: false,
    }
}

#[cfg(test)]
#[path = "external_session_index_tests.rs"]
mod tests;
