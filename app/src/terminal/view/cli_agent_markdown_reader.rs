#[cfg(test)]
use std::sync::Arc;

use markdown_parser::parse_markdown_with_gfm_tables;
#[cfg(test)]
use parking_lot::RwLock;
use warpui::elements::{Hoverable, MouseStateHandle, SavePosition};
use warpui::presenter::ChildView;
use warpui::{AppContext, Element, ViewContext, ViewHandle};

use super::{TerminalAction, TerminalView};
use crate::notebooks::link::{NotebookLinks, SessionSource};
use crate::notebooks::markdown_reader::{
    MarkdownReaderConfig, MarkdownReaderDocument, MarkdownReaderView,
};
use crate::terminal::TerminalModel;

const MAX_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;

pub(super) struct CliAgentMarkdownReader {
    reader: ViewHandle<MarkdownReaderView>,
    mouse_state: MouseStateHandle,
    #[cfg(test)]
    selected_text_override: Arc<RwLock<Option<String>>>,
}

impl CliAgentMarkdownReader {
    pub(super) fn new(
        document: MarkdownReaderDocument,
        ctx: &mut ViewContext<TerminalView>,
    ) -> Self {
        let window_id = ctx.window_id();
        let links = ctx.add_model(|ctx| NotebookLinks::new(SessionSource::Active(window_id), ctx));
        let parent_position_id = format!("cli_agent_markdown_reader_{}", ctx.view_id());
        let reader = ctx.add_view(|ctx| {
            MarkdownReaderView::new(
                document,
                parent_position_id,
                links,
                MarkdownReaderConfig::reading_view(true),
                ctx,
            )
        });

        Self {
            reader,
            mouse_state: Default::default(),
            #[cfg(test)]
            selected_text_override: Default::default(),
        }
    }

    pub(super) fn selected_text(&self, app: &AppContext) -> Option<String> {
        #[cfg(test)]
        if let Some(selected_text) = self.selected_text_override.read().clone() {
            return Some(selected_text);
        }
        self.reader.as_ref(app).selected_text(app)
    }

    #[cfg(test)]
    pub(super) fn set_selected_text_for_test(&self, selected_text: Option<String>) {
        *self.selected_text_override.write() = selected_text;
    }

    pub(super) fn render(&self, content_element_position_id: &str) -> Box<dyn Element> {
        let reader = self.reader.clone();
        let preview = Hoverable::new(self.mouse_state.clone(), move |_| {
            ChildView::new(&reader).finish()
        })
        .on_right_click(|ctx, _, position| {
            ctx.dispatch_typed_action(TerminalAction::AltScreenContextMenu { position });
        })
        .with_defer_events_to_children()
        .finish();

        SavePosition::new(preview, content_element_position_id).finish()
    }
}

pub(super) fn cli_agent_markdown_snapshot_source(
    model: &TerminalModel,
    has_active_cli_agent_session: bool,
) -> Option<String> {
    if !has_active_cli_agent_session || model.is_alt_screen_active() {
        return None;
    }

    let source = model
        .block_list()
        .active_block()
        .output_to_string_force_full_grid_contents();
    (source.len() <= MAX_SNAPSHOT_BYTES && !source.trim().is_empty()).then_some(source)
}

pub(super) fn parse_cli_agent_markdown_snapshot(source: &str) -> Option<MarkdownReaderDocument> {
    let source = normalize_terminal_visual_markdown(source)?;
    parse_markdown_with_gfm_tables(&source).ok()?;
    Some(MarkdownReaderDocument::visual_snapshot(source))
}

fn normalize_terminal_visual_markdown(source: &str) -> Option<String> {
    if source.len() > MAX_SNAPSHOT_BYTES || source.trim().is_empty() {
        return None;
    }

    let mut normalized = String::with_capacity(source.len());
    for (index, line) in source.lines().enumerate() {
        if index > 0 {
            normalized.push('\n');
        }
        normalized.push_str(&normalize_terminal_visual_line(line));
    }

    Some(normalized.trim_matches('\n').to_owned())
}

fn normalize_terminal_visual_line(line: &str) -> String {
    let content = line.trim_start_matches([' ', '\t']);
    let indent = &line[..line.len() - content.len()];

    for marker in ["• ", "● ", "◦ ", "▪ "] {
        if let Some(rest) = content.strip_prefix(marker) {
            return format!("{indent}- {rest}");
        }
    }

    let trimmed = content.trim_end();
    if trimmed.chars().count() >= 3
        && trimmed
            .chars()
            .all(|character| matches!(character, '-' | '—' | '─' | '━'))
    {
        return format!("{indent}---");
    }

    line.to_owned()
}

#[cfg(test)]
#[path = "cli_agent_markdown_reader_tests.rs"]
mod tests;
