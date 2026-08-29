use std::path::PathBuf;
use std::sync::Arc;

use warp_editor::model::CoreEditorModel;
use warpui::elements::{
    Border, Container, CrossAxisAlignment, Expanded, Flex, MainAxisSize, ParentElement, Text,
};
use warpui::fonts::{Properties, Style, Weight};
use warpui::presenter::ChildView;
use warpui::units::Pixels;
use warpui::{
    AppContext, Element, Entity, ModelHandle, SingletonEntity, View, ViewContext, ViewHandle,
};

use super::editor::model::NotebooksEditorModel;
use super::editor::view::{RichTextEditorConfig, RichTextEditorView};
use super::editor::{RichTextStyleProfile, rich_text_styles_for_profile};
use super::link::NotebookLinks;
use crate::appearance::Appearance;
use crate::editor::InteractionState;
use crate::settings::FontSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownDisplayMode {
    Rendered,
    Raw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkdownDocumentProvenance {
    AuthoritativeSource,
    VisualSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkdownReaderCapabilities {
    pub source_mode: bool,
    pub copy_source: bool,
    pub rendered_selection: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownReaderDocument {
    source: Arc<str>,
    rendered_markdown: Arc<str>,
    provenance: MarkdownDocumentProvenance,
}

impl MarkdownReaderDocument {
    pub fn authoritative(source: impl Into<Arc<str>>) -> Self {
        Self::new(
            source.into(),
            MarkdownDocumentProvenance::AuthoritativeSource,
        )
    }

    pub fn visual_snapshot(source: impl Into<Arc<str>>) -> Self {
        Self::new(source.into(), MarkdownDocumentProvenance::VisualSnapshot)
    }

    fn new(source: Arc<str>, provenance: MarkdownDocumentProvenance) -> Self {
        let rendered_markdown = project_markdown_for_reading(&source).into();
        Self {
            source,
            rendered_markdown,
            provenance,
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn rendered_markdown(&self) -> &str {
        &self.rendered_markdown
    }

    pub fn provenance(&self) -> MarkdownDocumentProvenance {
        self.provenance
    }

    pub fn capabilities(&self) -> MarkdownReaderCapabilities {
        match self.provenance {
            MarkdownDocumentProvenance::AuthoritativeSource => MarkdownReaderCapabilities {
                source_mode: true,
                copy_source: true,
                rendered_selection: true,
            },
            MarkdownDocumentProvenance::VisualSnapshot => MarkdownReaderCapabilities {
                source_mode: false,
                copy_source: false,
                rendered_selection: true,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MarkdownReaderConfig {
    pub max_width: Pixels,
    pub show_provenance_header: bool,
}

impl MarkdownReaderConfig {
    pub fn reading_view(show_provenance_header: bool) -> Self {
        Self {
            max_width: super::styles::markdown_reader_max_width(),
            show_provenance_header,
        }
    }
}

pub struct MarkdownReaderView {
    document: MarkdownReaderDocument,
    editor: ViewHandle<RichTextEditorView>,
    show_provenance_header: bool,
}

impl MarkdownReaderView {
    pub fn new(
        document: MarkdownReaderDocument,
        parent_position_id: String,
        links: ModelHandle<NotebookLinks>,
        config: MarkdownReaderConfig,
        ctx: &mut ViewContext<Self>,
    ) -> Self {
        let window_id = ctx.window_id();
        let editor_model = ctx.add_model(|ctx| {
            let styles = rich_text_styles_for_profile(
                RichTextStyleProfile::MarkdownReader,
                Appearance::as_ref(ctx),
                FontSettings::as_ref(ctx),
            );
            let mut model = NotebooksEditorModel::new(styles, window_id, ctx);
            model.set_default_mermaid_display_mode(MarkdownDisplayMode::Rendered, ctx);
            model
        });
        let editor = ctx.add_typed_action_view(|ctx| {
            let mut view = RichTextEditorView::new(
                parent_position_id,
                editor_model,
                links,
                RichTextEditorConfig {
                    max_width: Some(config.max_width),
                    style_profile: RichTextStyleProfile::MarkdownReader,
                    ..Default::default()
                },
                ctx,
            );
            view.set_interaction_state(InteractionState::Selectable, ctx);
            view.reset_with_markdown(document.rendered_markdown(), ctx);
            view
        });

        Self {
            document,
            editor,
            show_provenance_header: config.show_provenance_header,
        }
    }

    pub fn document(&self) -> &MarkdownReaderDocument {
        &self.document
    }

    pub fn editor(&self) -> ViewHandle<RichTextEditorView> {
        self.editor.clone()
    }

    pub fn reset_with_markdown(&mut self, source: &str, ctx: &mut ViewContext<Self>) {
        self.document = MarkdownReaderDocument::authoritative(Arc::<str>::from(source));
        self.editor.update(ctx, |editor, ctx| {
            editor.reset_with_markdown(self.document.rendered_markdown(), ctx);
        });
    }

    pub fn reset_with_ipynb(&mut self, source: &str, ctx: &mut ViewContext<Self>) {
        self.document = MarkdownReaderDocument::authoritative(Arc::<str>::from(source));
        self.editor.update(ctx, |editor, ctx| {
            editor.reset_with_ipynb(source, ctx);
        });
    }

    pub fn set_document_path(&self, document_path: Option<PathBuf>, ctx: &mut ViewContext<Self>) {
        let editor_model = self.editor.as_ref(ctx).model().clone();
        editor_model.update(ctx, |model, ctx| {
            model.set_document_path(document_path, ctx);
        });
    }

    pub fn selected_text(&self, app: &AppContext) -> Option<String> {
        self.editor.as_ref(app).selected_text(app)
    }

    fn render_provenance_header(&self, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let background = theme.surface_1();
        let capabilities = self.document.capabilities();
        let provenance_label = match self.document.provenance() {
            MarkdownDocumentProvenance::AuthoritativeSource => {
                warp_i18n::localize_ui("Source-backed document")
            }
            MarkdownDocumentProvenance::VisualSnapshot => warp_i18n::localize_ui("Visual snapshot"),
        };
        debug_assert_eq!(
            capabilities.source_mode,
            matches!(
                self.document.provenance(),
                MarkdownDocumentProvenance::AuthoritativeSource
            )
        );
        let title = Text::new_inline(
            warp_i18n::localize_ui("Reading view"),
            appearance.ui_font_family(),
            appearance.ui_font_size() - 1.,
        )
        .with_style(Properties {
            style: Style::Normal,
            weight: Weight::Semibold,
        })
        .with_color(theme.main_text_color(background).into())
        .finish();
        let provenance = Text::new_inline(
            provenance_label,
            appearance.ui_font_family(),
            appearance.ui_font_size() - 2.,
        )
        .with_color(theme.sub_text_color(background).into())
        .finish();

        Container::new(
            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(title)
                .with_child(Container::new(provenance).with_padding_left(8.).finish())
                .finish(),
        )
        .with_horizontal_padding(20.)
        .with_vertical_padding(8.)
        .with_background(background)
        .with_border(Border::bottom(1.).with_border_fill(theme.outline()))
        .finish()
    }
}

impl Entity for MarkdownReaderView {
    type Event = ();
}

impl View for MarkdownReaderView {
    fn ui_name() -> &'static str {
        "MarkdownReaderView"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let body = ChildView::new(&self.editor).finish();
        let content = if self.show_provenance_header {
            Flex::column()
                .with_main_axis_size(MainAxisSize::Max)
                .with_cross_axis_alignment(CrossAxisAlignment::Stretch)
                .with_child(self.render_provenance_header(appearance))
                .with_child(Expanded::new(1., body).finish())
                .finish()
        } else {
            body
        };

        Container::new(content)
            .with_background(appearance.theme().background())
            .finish()
    }
}

fn project_markdown_for_reading(source: &str) -> String {
    let mut projected = String::with_capacity(source.len());
    let mut fence: Option<(char, usize)> = None;

    for (index, line) in source.lines().enumerate() {
        if index > 0 {
            projected.push('\n');
        }

        let trimmed = line.trim_start();
        if let Some((marker, minimum_len)) = fence {
            projected.push_str(line);
            if fence_run(trimmed, marker).is_some_and(|len| len >= minimum_len) {
                fence = None;
            }
            continue;
        }

        if let Some((marker, len)) = opening_fence(trimmed) {
            fence = Some((marker, len));
            projected.push_str(line);
            continue;
        }

        projected.push_str(&project_blockquote_line(line));
    }

    if source.ends_with('\n') {
        projected.push('\n');
    }
    projected
}

fn opening_fence(line: &str) -> Option<(char, usize)> {
    for marker in ['`', '~'] {
        let len = line
            .chars()
            .take_while(|character| *character == marker)
            .count();
        if len >= 3 {
            return Some((marker, len));
        }
    }
    None
}

fn fence_run(line: &str, marker: char) -> Option<usize> {
    let len = line
        .chars()
        .take_while(|character| *character == marker)
        .count();
    (len >= 3 && line.chars().skip(len).all(char::is_whitespace)).then_some(len)
}

fn project_blockquote_line(line: &str) -> String {
    let content = line.trim_start_matches([' ', '\t']);
    let indent = &line[..line.len() - content.len()];
    let mut depth = 0;
    let mut remainder = content;

    while let Some(after_marker) = remainder.strip_prefix('>') {
        depth += 1;
        remainder = after_marker.strip_prefix(' ').unwrap_or(after_marker);
    }
    if depth == 0 {
        return line.to_owned();
    }

    let rails = "│ ".repeat(depth);
    if remainder.is_empty() {
        return format!("{indent}{}", rails.trim_end());
    }

    if let Some((heading, text)) = split_heading_marker(remainder) {
        return format!("{indent}{heading} {rails}{text}");
    }
    if let Some((marker, text)) = split_list_marker(remainder) {
        return format!("{indent}{marker}{rails}{text}");
    }

    format!("{indent}{rails}{remainder}")
}

fn split_heading_marker(line: &str) -> Option<(&str, &str)> {
    let marker_len = line
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if !(1..=6).contains(&marker_len) || !line[marker_len..].starts_with(' ') {
        return None;
    }
    Some((&line[..marker_len], &line[marker_len + 1..]))
}

fn split_list_marker(line: &str) -> Option<(&str, &str)> {
    for marker in ["- ", "* ", "+ "] {
        if let Some(text) = line.strip_prefix(marker) {
            return Some((marker, text));
        }
    }

    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        return Some((&line[..digits + 2], &line[digits + 2..]));
    }
    None
}

#[cfg(test)]
#[path = "markdown_reader_tests.rs"]
mod tests;
