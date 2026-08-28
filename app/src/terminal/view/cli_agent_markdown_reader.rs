use std::sync::Arc;

use markdown_parser::{FormattedText, parse_markdown_with_gfm_tables};
use parking_lot::RwLock;
use warpui::Element;
use warpui::elements::{
    Border, ClippedScrollStateHandle, ClippedScrollable, Container, CrossAxisAlignment, Expanded,
    Fill, Flex, FormattedTextElement, HighlightedHyperlink, Hoverable, MainAxisSize,
    MouseStateHandle, ParentElement, SavePosition, ScrollbarWidth, SelectableArea, SelectionHandle,
    Text,
};

use super::TerminalAction;
use crate::appearance::Appearance;
use crate::terminal::TerminalModel;

const MAX_SNAPSHOT_BYTES: usize = 2 * 1024 * 1024;

pub(super) struct CliAgentMarkdownReader {
    formatted_text: Arc<FormattedText>,
    highlighted_link: HighlightedHyperlink,
    scroll_state: ClippedScrollStateHandle,
    mouse_state: MouseStateHandle,
    selection_handle: SelectionHandle,
    selected_text: Arc<RwLock<Option<String>>>,
}

impl CliAgentMarkdownReader {
    pub(super) fn new(formatted_text: Arc<FormattedText>) -> Self {
        Self {
            formatted_text,
            highlighted_link: Default::default(),
            scroll_state: Default::default(),
            mouse_state: Default::default(),
            selection_handle: Default::default(),
            selected_text: Default::default(),
        }
    }

    pub(super) fn selected_text(&self) -> Option<String> {
        self.selected_text
            .read()
            .clone()
            .filter(|selection| !selection.is_empty())
    }

    #[cfg(test)]
    pub(super) fn set_selected_text_for_test(&self, selected_text: Option<String>) {
        *self.selected_text.write() = selected_text;
    }

    pub(super) fn render(
        &self,
        content_element_position_id: &str,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let background = theme.background();
        let text_color = theme.main_text_color(background).into_solid();
        let font_size = appearance.ui_font_size();
        let markdown = FormattedTextElement::new_arc(
            self.formatted_text.clone(),
            font_size,
            appearance.ai_font_family(),
            appearance.monospace_font_family(),
            text_color,
            self.highlighted_link.clone(),
        )
        .set_selectable(true)
        .register_default_click_handlers(|url, ctx, _| {
            ctx.dispatch_typed_action(TerminalAction::HyperlinkClick(url));
        })
        .finish();

        let body = Container::new(markdown)
            .with_horizontal_padding(20.)
            .with_vertical_padding(16.)
            .finish();
        let scrollable = ClippedScrollable::vertical(
            self.scroll_state.clone(),
            body,
            ScrollbarWidth::Auto,
            theme.disabled_text_color(background).into(),
            theme.main_text_color(background).into(),
            Fill::None,
        )
        .finish();
        let selected_text = self.selected_text.clone();
        let selectable = SelectableArea::new(
            self.selection_handle.clone(),
            move |selection_args, _, _| {
                *selected_text.write() = selection_args
                    .selection
                    .filter(|selection| !selection.is_empty());
            },
            scrollable,
        )
        .finish();
        let header = Container::new(
            Text::new_inline(
                warp_i18n::localize_ui("Markdown reader · visual snapshot"),
                appearance.ui_font_family(),
                font_size - 2.,
            )
            .with_color(theme.sub_text_color(theme.surface_1()).into())
            .finish(),
        )
        .with_horizontal_padding(20.)
        .with_vertical_padding(8.)
        .with_background(theme.surface_1())
        .with_border(Border::bottom(1.).with_border_fill(theme.outline()))
        .finish();
        let content = Container::new(
            Flex::column()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_child(header)
                .with_child(Expanded::new(1., selectable).finish())
                .finish(),
        )
        .with_background(background)
        .finish();
        let preview = Hoverable::new(self.mouse_state.clone(), move |_| content)
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

pub(super) fn parse_cli_agent_markdown_snapshot(source: &str) -> Option<Arc<FormattedText>> {
    let source = normalize_terminal_visual_markdown(source)?;
    parse_markdown_with_gfm_tables(&source).ok().map(Arc::new)
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

    if let Some(blockquote) = normalize_terminal_blockquote_line(content) {
        return format!("{indent}{blockquote}");
    }

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

fn normalize_terminal_blockquote_line(content: &str) -> Option<String> {
    let mut depth = 0;
    let mut remainder = content;
    while let Some(after_marker) = remainder.strip_prefix('>') {
        depth += 1;
        remainder = after_marker.strip_prefix(' ').unwrap_or(after_marker);
    }
    if depth == 0 {
        return None;
    }

    let mut quote_prefix = String::new();
    for _ in 0..depth {
        quote_prefix.push('│');
        quote_prefix.push(' ');
    }
    if remainder.trim().is_empty() {
        return Some(quote_prefix.trim_end().to_owned());
    }

    for marker in ["- ", "* ", "+ ", "• ", "● ", "◦ ", "▪ "] {
        if let Some(rest) = remainder.strip_prefix(marker) {
            return Some(format!("{quote_prefix}  • {rest}"));
        }
    }

    Some(format!("{quote_prefix}{remainder}"))
}

#[cfg(test)]
#[path = "cli_agent_markdown_reader_tests.rs"]
mod tests;
