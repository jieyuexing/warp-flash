use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::future::join_all;
use warp_core::send_telemetry_from_ctx;
use warpui::elements::{
    Border, ChildView, Clipped, ClippedScrollStateHandle, ClippedScrollable, ConstrainedBox,
    Container, CornerRadius, CrossAxisAlignment, Element, Fill, Flex, Hoverable, MainAxisAlignment,
    MainAxisSize, MouseStateHandle, ParentElement, Radius, ScrollStateHandle, Scrollable,
    ScrollableElement, ScrollbarWidth, Shrinkable, Text, UniformList, UniformListState,
};
use warpui::platform::Cursor;
use warpui::ui_components::button::ButtonVariant;
use warpui::ui_components::components::{Coords, UiComponent, UiComponentStyles};
use warpui::{
    AppContext, Entity, ModelHandle, SingletonEntity, TypedActionView, View, ViewContext,
    ViewHandle, WeakViewHandle,
};

use super::model::{
    BranchKind, ChangeGroup, DiffTarget, GitBranch, GitChange, GitCommit, RepositorySnapshot,
    checkout_branch, checkout_remote_branch, create_branch, delete_branch, discard_paths,
    discover_repository_roots, fetch, load_commit_diff, load_diff, load_repository_snapshot,
    merge_branch, pop_stash, pull, stage_paths, stash, unstage_paths,
};
use super::{GitOperation, VersionControlTelemetryEvent};
use crate::appearance::Appearance;
use crate::code::buffer_location::LocalOrRemotePath;
use crate::code_review::git_repo_model::{GitRepoModels, GitRepoStatusEvent, GitRepoStatusModel};
use crate::editor::{
    EditorOptions, EditorView, Event as EditorEvent, SingleLineEditorOptions, TextOptions,
};
use crate::util::git::{run_commit, run_push};

const DETAIL_MAX_BYTES: usize = 48_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VersionControlTab {
    Changes,
    Log,
    Branches,
}

#[derive(Clone, Debug)]
pub enum VersionControlAction {
    SetTab(VersionControlTab),
    Refresh,
    CycleRepository,
    SelectChange { path: PathBuf, target: DiffTarget },
    StageSelected,
    UnstageSelected,
    StageAll,
    UnstageAll,
    RequestDiscardSelected,
    SelectCommit(String),
    SelectBranch { name: String, kind: BranchKind },
    CheckoutSelectedBranch,
    MergeSelectedBranch,
    RequestDeleteSelectedBranch,
    CreateBranch,
    ConfirmDestructive,
    CancelDestructive,
    Stash,
    PopStash,
    Fetch,
    Pull,
    Push,
    Commit,
}

#[derive(Clone)]
enum ListRow {
    Section {
        label: String,
        count: usize,
    },
    Change {
        change: GitChange,
        target: DiffTarget,
    },
    Commit(GitCommit),
    Branch(GitBranch),
}

#[derive(Clone)]
enum PendingDestructiveOperation {
    Discard { paths: Vec<PathBuf> },
    DeleteBranch { name: String },
}

#[derive(Default)]
struct StaticMouseStates {
    changes_tab: MouseStateHandle,
    log_tab: MouseStateHandle,
    branches_tab: MouseStateHandle,
    refresh: MouseStateHandle,
    repository: MouseStateHandle,
    fetch: MouseStateHandle,
    pull: MouseStateHandle,
    push: MouseStateHandle,
    stage_selected: MouseStateHandle,
    unstage_selected: MouseStateHandle,
    stage_all: MouseStateHandle,
    unstage_all: MouseStateHandle,
    discard: MouseStateHandle,
    checkout: MouseStateHandle,
    merge: MouseStateHandle,
    delete_branch: MouseStateHandle,
    create_branch: MouseStateHandle,
    commit: MouseStateHandle,
    stash: MouseStateHandle,
    pop_stash: MouseStateHandle,
    confirm_destructive: MouseStateHandle,
    cancel_destructive: MouseStateHandle,
}

pub struct VersionControlView {
    handle: WeakViewHandle<Self>,
    tab: VersionControlTab,
    working_directories: Vec<PathBuf>,
    snapshots: Vec<RepositorySnapshot>,
    selected_repository: usize,
    selected_change: Option<(PathBuf, DiffTarget)>,
    selected_commit: Option<String>,
    selected_branch: Option<(String, BranchKind)>,
    pending_destructive_operation: Option<PendingDestructiveOperation>,
    detail: String,
    status_message: Option<String>,
    error_message: Option<String>,
    loading: bool,
    generation: u64,
    rows: Vec<ListRow>,
    list_state: UniformListState,
    scroll_state: ScrollStateHandle,
    detail_scroll_state: ClippedScrollStateHandle,
    row_mouse_states: HashMap<String, MouseStateHandle>,
    watched_repositories: HashMap<PathBuf, ModelHandle<GitRepoStatusModel>>,
    mouse_states: StaticMouseStates,
    filter_editor: ViewHandle<EditorView>,
    commit_editor: ViewHandle<EditorView>,
    branch_editor: ViewHandle<EditorView>,
}

impl Entity for VersionControlView {
    type Event = ();
}

impl VersionControlView {
    pub fn new(ctx: &mut ViewContext<Self>) -> Self {
        let editor_text = TextOptions {
            font_size_override: Some(12.),
            font_family_override: Some(Appearance::as_ref(ctx).ui_font_family()),
            ..Default::default()
        };
        let filter_editor = ctx.add_typed_action_view(|ctx| {
            let mut editor = EditorView::new(
                SingleLineEditorOptions {
                    text: editor_text.clone(),
                    ..Default::default()
                }
                .into(),
                ctx,
            );
            editor.set_placeholder_text("Filter files, commits, or branches", ctx);
            editor
        });
        let commit_editor = ctx.add_typed_action_view(|ctx| {
            let mut editor = EditorView::new(
                EditorOptions {
                    text: editor_text.clone(),
                    autogrow: true,
                    soft_wrap: true,
                    max_buffer_len: Some(10_000),
                    ..Default::default()
                },
                ctx,
            );
            editor.set_placeholder_text("Commit message", ctx);
            editor
        });
        let branch_editor = ctx.add_typed_action_view(|ctx| {
            let mut editor = EditorView::new(
                SingleLineEditorOptions {
                    text: editor_text,
                    max_buffer_len: Some(255),
                    ..Default::default()
                }
                .into(),
                ctx,
            );
            editor.set_placeholder_text("New branch name", ctx);
            editor
        });
        ctx.subscribe_to_view(&filter_editor, |me, _, event, ctx| match event {
            EditorEvent::Edited(_)
            | EditorEvent::BufferReplaced
            | EditorEvent::BufferReinitialized => {
                me.rebuild_rows(ctx);
                ctx.notify();
            }
            _ => {}
        });
        for editor in [&commit_editor, &branch_editor] {
            ctx.subscribe_to_view(editor, |_me, _, event, ctx| match event {
                EditorEvent::Edited(_)
                | EditorEvent::BufferReplaced
                | EditorEvent::BufferReinitialized => ctx.notify(),
                _ => {}
            });
        }

        Self {
            handle: ctx.handle(),
            tab: VersionControlTab::Changes,
            working_directories: Vec::new(),
            snapshots: Vec::new(),
            selected_repository: 0,
            selected_change: None,
            selected_commit: None,
            selected_branch: None,
            pending_destructive_operation: None,
            detail: String::new(),
            status_message: None,
            error_message: None,
            loading: false,
            generation: 0,
            rows: Vec::new(),
            list_state: UniformListState::new(),
            scroll_state: Arc::new(Mutex::new(Default::default())),
            detail_scroll_state: ClippedScrollStateHandle::default(),
            row_mouse_states: HashMap::new(),
            watched_repositories: HashMap::new(),
            mouse_states: StaticMouseStates::default(),
            filter_editor,
            commit_editor,
            branch_editor,
        }
    }

    pub fn set_working_directories(
        &mut self,
        directories: Vec<PathBuf>,
        ctx: &mut ViewContext<Self>,
    ) {
        if self.working_directories == directories {
            return;
        }
        self.working_directories = directories;
        self.discover_and_refresh(ctx);
    }

    pub fn refresh(&mut self, ctx: &mut ViewContext<Self>) {
        if self.snapshots.is_empty() {
            self.discover_and_refresh(ctx);
        } else {
            self.refresh_snapshots(
                self.snapshots
                    .iter()
                    .map(|snapshot| snapshot.root.clone())
                    .collect(),
                ctx,
            );
        }
    }

    #[cfg(feature = "integration_tests")]
    pub fn repository_count(&self) -> usize {
        self.snapshots.len()
    }

    fn discover_and_refresh(&mut self, ctx: &mut ViewContext<Self>) {
        self.loading = true;
        self.error_message = None;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let directories = self.working_directories.clone();
        ctx.spawn(
            async move { discover_repository_roots(&directories).await },
            move |me, roots, ctx| {
                if me.generation != generation {
                    return;
                }
                me.refresh_snapshots(roots, ctx);
            },
        );
        ctx.notify();
    }

    fn refresh_snapshots(&mut self, roots: Vec<PathBuf>, ctx: &mut ViewContext<Self>) {
        self.loading = true;
        self.error_message = None;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        ctx.spawn(
            async move {
                join_all(roots.into_iter().map(|root| async move {
                    let snapshot = load_repository_snapshot(&root).await;
                    (root, snapshot)
                }))
                .await
            },
            move |me, results, ctx| {
                if me.generation != generation {
                    return;
                }
                me.loading = false;
                let mut failures = Vec::new();
                me.snapshots = results
                    .into_iter()
                    .filter_map(|(root, result)| match result {
                        Ok(snapshot) => Some(snapshot),
                        Err(error) => {
                            failures.push(format!("{}: {error}", root.display()));
                            None
                        }
                    })
                    .collect();
                if me.snapshots.is_empty() && !failures.is_empty() {
                    me.error_message = Some(failures.join("\n"));
                }
                me.selected_repository = me
                    .selected_repository
                    .min(me.snapshots.len().saturating_sub(1));
                me.subscribe_to_repository_updates(ctx);
                me.rebuild_rows(ctx);
                ctx.notify();
            },
        );
        ctx.notify();
    }

    fn current_snapshot(&self) -> Option<&RepositorySnapshot> {
        self.snapshots.get(self.selected_repository)
    }

    fn subscribe_to_repository_updates(&mut self, ctx: &mut ViewContext<Self>) {
        let roots = self
            .snapshots
            .iter()
            .map(|snapshot| snapshot.root.clone())
            .collect::<Vec<_>>();
        for root in roots {
            if self.watched_repositories.contains_key(&root) {
                continue;
            }
            let repo = LocalOrRemotePath::Local(root.clone());
            let result =
                GitRepoModels::handle(ctx).update(ctx, |models, ctx| models.subscribe(&repo, ctx));
            let Ok(model) = result else {
                continue;
            };
            ctx.subscribe_to_model(&model, |me, _, event, ctx| match event {
                GitRepoStatusEvent::MetadataChanged => me.refresh(ctx),
            });
            self.watched_repositories.insert(root, model);
        }
    }

    fn rebuild_rows(&mut self, app: &AppContext) {
        self.rows.clear();
        let filter = self
            .filter_editor
            .as_ref(app)
            .buffer_text(app)
            .to_lowercase();
        let Some(snapshot) = self.current_snapshot().cloned() else {
            self.row_mouse_states.clear();
            return;
        };
        match self.tab {
            VersionControlTab::Changes => {
                for (group, label, target) in [
                    (
                        ChangeGroup::Conflicts,
                        "MERGE CONFLICTS",
                        DiffTarget::Worktree,
                    ),
                    (ChangeGroup::Staged, "STAGED", DiffTarget::Index),
                    (ChangeGroup::Unstaged, "CHANGES", DiffTarget::Worktree),
                    (
                        ChangeGroup::Untracked,
                        "UNVERSIONED FILES",
                        DiffTarget::Worktree,
                    ),
                ] {
                    let changes = snapshot
                        .changes_in_group(group)
                        .filter(|change| matches_filter(&change.path.to_string_lossy(), &filter))
                        .cloned()
                        .collect::<Vec<_>>();
                    if changes.is_empty() {
                        continue;
                    }
                    self.rows.push(ListRow::Section {
                        label: label.to_string(),
                        count: changes.len(),
                    });
                    self.rows.extend(
                        changes
                            .into_iter()
                            .map(|change| ListRow::Change { change, target }),
                    );
                }
            }
            VersionControlTab::Log => {
                self.rows.extend(
                    snapshot
                        .commits
                        .iter()
                        .filter(|commit| {
                            matches_filter(&commit.subject, &filter)
                                || matches_filter(&commit.author_name, &filter)
                                || matches_filter(&commit.hash, &filter)
                                || commit
                                    .decorations
                                    .iter()
                                    .any(|decoration| matches_filter(decoration, &filter))
                        })
                        .cloned()
                        .map(ListRow::Commit),
                );
            }
            VersionControlTab::Branches => {
                for (kind, label) in [(BranchKind::Local, "LOCAL"), (BranchKind::Remote, "REMOTE")]
                {
                    let branches = snapshot
                        .branches
                        .iter()
                        .filter(|branch| {
                            branch.kind == kind && matches_filter(&branch.name, &filter)
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    if branches.is_empty() {
                        continue;
                    }
                    self.rows.push(ListRow::Section {
                        label: label.to_string(),
                        count: branches.len(),
                    });
                    self.rows.extend(branches.into_iter().map(ListRow::Branch));
                }
            }
        }
        self.row_mouse_states.retain(|key, _| {
            self.rows
                .iter()
                .any(|row| row_key(row).as_deref() == Some(key))
        });
        for row in &self.rows {
            if let Some(key) = row_key(row) {
                self.row_mouse_states.entry(key).or_default();
            }
        }
        self.list_state = UniformListState::new();
    }

    fn set_tab(&mut self, tab: VersionControlTab, ctx: &mut ViewContext<Self>) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.detail.clear();
        self.detail_scroll_state = ClippedScrollStateHandle::default();
        self.rebuild_rows(ctx);
        ctx.notify();
    }

    fn select_change(&mut self, path: PathBuf, target: DiffTarget, ctx: &mut ViewContext<Self>) {
        self.selected_change = Some((path.clone(), target));
        self.detail_scroll_state = ClippedScrollStateHandle::default();
        let Some(snapshot) = self.current_snapshot() else {
            return;
        };
        let Some(change) = snapshot
            .changes
            .iter()
            .find(|change| change.path == path)
            .cloned()
        else {
            return;
        };
        let root = snapshot.root.clone();
        self.detail = "Loading diff\u{2026}".to_string();
        ctx.spawn(
            async move { load_diff(&root, &change, target).await },
            |me, result, ctx| {
                me.detail = result
                    .map(|diff| truncate_detail(diff, DETAIL_MAX_BYTES))
                    .unwrap_or_else(|error| error.to_string());
                ctx.notify();
            },
        );
        ctx.notify();
    }

    fn select_commit(&mut self, hash: String, ctx: &mut ViewContext<Self>) {
        self.selected_commit = Some(hash.clone());
        self.detail_scroll_state = ClippedScrollStateHandle::default();
        let Some(root) = self
            .current_snapshot()
            .map(|snapshot| snapshot.root.clone())
        else {
            return;
        };
        self.detail = "Loading commit\u{2026}".to_string();
        ctx.spawn(
            async move { load_commit_diff(&root, &hash).await },
            |me, result, ctx| {
                me.detail = result
                    .map(|diff| truncate_detail(diff, DETAIL_MAX_BYTES))
                    .unwrap_or_else(|error| error.to_string());
                ctx.notify();
            },
        );
        ctx.notify();
    }

    fn selected_change_paths(&self) -> Option<Vec<PathBuf>> {
        let (path, _) = self.selected_change.as_ref()?;
        let change = self
            .current_snapshot()?
            .changes
            .iter()
            .find(|change| &change.path == path)?;
        let mut paths = vec![change.path.clone()];
        if let Some(original) = &change.original_path {
            paths.push(original.clone());
        }
        Some(paths)
    }

    fn run_path_operation(&mut self, stage: bool, all: bool, ctx: &mut ViewContext<Self>) {
        let Some(snapshot) = self.current_snapshot() else {
            return;
        };
        let root = snapshot.root.clone();
        let paths = if all {
            snapshot
                .changes
                .iter()
                .filter(|change| {
                    if stage {
                        change.is_unstaged() || change.is_untracked() || change.is_conflicted()
                    } else {
                        change.is_staged() || change.is_conflicted()
                    }
                })
                .flat_map(|change| {
                    std::iter::once(change.path.clone()).chain(change.original_path.clone())
                })
                .collect::<Vec<_>>()
        } else {
            self.selected_change_paths().unwrap_or_default()
        };
        self.run_git_operation(
            if stage {
                GitOperation::Stage
            } else {
                GitOperation::Unstage
            },
            if stage {
                "Staged changes"
            } else {
                "Unstaged changes"
            },
            async move {
                if stage {
                    stage_paths(&root, &paths, None).await
                } else {
                    unstage_paths(&root, &paths, None).await
                }
            },
            ctx,
        );
    }

    fn run_git_operation<F>(
        &mut self,
        operation: GitOperation,
        success_message: &'static str,
        future: F,
        ctx: &mut ViewContext<Self>,
    ) where
        F: std::future::Future<Output = anyhow::Result<String>> + Send + 'static,
    {
        self.loading = true;
        self.error_message = None;
        self.status_message = None;
        ctx.spawn(future, move |me, result, ctx| {
            me.loading = false;
            match result {
                Ok(output) => {
                    send_telemetry_from_ctx!(
                        VersionControlTelemetryEvent::OperationCompleted {
                            operation,
                            success: true,
                        },
                        ctx
                    );
                    match operation {
                        GitOperation::Commit => me.commit_editor.update(ctx, |editor, ctx| {
                            editor.system_reset_buffer_text("", ctx);
                        }),
                        GitOperation::CreateBranch => {
                            me.branch_editor.update(ctx, |editor, ctx| {
                                editor.system_reset_buffer_text("", ctx);
                            })
                        }
                        GitOperation::Stage
                        | GitOperation::Unstage
                        | GitOperation::Rollback
                        | GitOperation::Stash
                        | GitOperation::PopStash
                        | GitOperation::Fetch
                        | GitOperation::Pull
                        | GitOperation::Push
                        | GitOperation::Checkout
                        | GitOperation::MergeBranch
                        | GitOperation::DeleteBranch => {}
                    }
                    me.status_message = Some(if output.trim().is_empty() {
                        success_message.to_string()
                    } else {
                        truncate_detail(output, 2_000)
                    });
                    me.refresh(ctx);
                }
                Err(error) => {
                    send_telemetry_from_ctx!(
                        VersionControlTelemetryEvent::OperationCompleted {
                            operation,
                            success: false,
                        },
                        ctx
                    );
                    me.error_message = Some(error.to_string());
                    ctx.notify();
                }
            }
        });
        ctx.notify();
    }

    fn render_button(
        &self,
        label: &str,
        variant: ButtonVariant,
        mouse_state: MouseStateHandle,
        action: VersionControlAction,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        appearance
            .ui_builder()
            .button(variant, mouse_state)
            .with_text_label(label.to_string())
            .with_style(UiComponentStyles {
                height: Some(24.),
                font_size: Some(11.),
                padding: Some(Coords::uniform(4.).left(7.).right(7.)),
                ..Default::default()
            })
            .build()
            .on_click(move |ctx, _, _| ctx.dispatch_typed_action(action.clone()))
            .with_cursor(Cursor::PointingHand)
            .finish()
    }

    fn render_editor(editor: &ViewHandle<EditorView>, app: &AppContext) -> Box<dyn Element> {
        let theme = Appearance::as_ref(app).theme();
        Container::new(Clipped::new(ChildView::new(editor).finish()).finish())
            .with_uniform_padding(6.)
            .with_border(Border::all(1.).with_border_fill(theme.surface_3()))
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
            .finish()
    }

    fn render_toolbar(&self, appearance: &Appearance) -> Box<dyn Element> {
        Flex::row()
            .with_spacing(4.)
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(self.render_button(
                "Fetch",
                ButtonVariant::Text,
                self.mouse_states.fetch.clone(),
                VersionControlAction::Fetch,
                appearance,
            ))
            .with_child(self.render_button(
                "Pull",
                ButtonVariant::Text,
                self.mouse_states.pull.clone(),
                VersionControlAction::Pull,
                appearance,
            ))
            .with_child(self.render_button(
                "Push",
                ButtonVariant::Text,
                self.mouse_states.push.clone(),
                VersionControlAction::Push,
                appearance,
            ))
            .with_child(self.render_button(
                "Refresh",
                ButtonVariant::Text,
                self.mouse_states.refresh.clone(),
                VersionControlAction::Refresh,
                appearance,
            ))
            .finish()
    }

    fn render_tabs(&self, appearance: &Appearance) -> Box<dyn Element> {
        Flex::row()
            .with_spacing(2.)
            .with_child(self.render_button(
                "Changes",
                if self.tab == VersionControlTab::Changes {
                    ButtonVariant::Accent
                } else {
                    ButtonVariant::Text
                },
                self.mouse_states.changes_tab.clone(),
                VersionControlAction::SetTab(VersionControlTab::Changes),
                appearance,
            ))
            .with_child(self.render_button(
                "Log",
                if self.tab == VersionControlTab::Log {
                    ButtonVariant::Accent
                } else {
                    ButtonVariant::Text
                },
                self.mouse_states.log_tab.clone(),
                VersionControlAction::SetTab(VersionControlTab::Log),
                appearance,
            ))
            .with_child(self.render_button(
                "Branches",
                if self.tab == VersionControlTab::Branches {
                    ButtonVariant::Accent
                } else {
                    ButtonVariant::Text
                },
                self.mouse_states.branches_tab.clone(),
                VersionControlAction::SetTab(VersionControlTab::Branches),
                appearance,
            ))
            .finish()
    }

    fn render_row(&self, row: &ListRow, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        match row {
            ListRow::Section { label, count } => Container::new(
                Text::new_inline(
                    format!("{label}  {count}"),
                    appearance.ui_font_family(),
                    10.,
                )
                .with_color(theme.sub_text_color(theme.background()).into())
                .finish(),
            )
            .with_horizontal_padding(10.)
            .with_vertical_padding(7.)
            .finish(),
            ListRow::Change { change, target } => {
                let key = row_key(row).expect("change row should have a key");
                let mouse = self.row_mouse_states[&key].clone();
                let path = change.path.clone();
                let target = *target;
                let selected = self.selected_change.as_ref() == Some(&(path.clone(), target));
                let status = if change.is_conflicted() {
                    "!"
                } else if change.is_untracked() {
                    "?"
                } else if target == DiffTarget::Index {
                    "S"
                } else {
                    "M"
                };
                Hoverable::new(mouse, move |hovered| {
                    let row = Flex::row()
                        .with_spacing(8.)
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_child(
                            Text::new_inline(
                                status,
                                appearance.ui_font_family(),
                                appearance.ui_font_size(),
                            )
                            .with_color(theme.accent().into_solid())
                            .finish(),
                        )
                        .with_child(
                            Shrinkable::new(
                                1.,
                                Text::new_inline(
                                    change.path.to_string_lossy().into_owned(),
                                    appearance.ui_font_family(),
                                    appearance.ui_font_size(),
                                )
                                .with_color(theme.main_text_color(theme.background()).into())
                                .finish(),
                            )
                            .finish(),
                        )
                        .finish();
                    let mut container = Container::new(row)
                        .with_horizontal_padding(12.)
                        .with_vertical_padding(6.);
                    if selected || hovered.is_hovered() {
                        container = container.with_background(theme.surface_overlay_1());
                    }
                    container.finish()
                })
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(VersionControlAction::SelectChange {
                        path: path.clone(),
                        target,
                    });
                })
                .with_cursor(Cursor::PointingHand)
                .finish()
            }
            ListRow::Commit(commit) => {
                let key = row_key(row).expect("commit row should have a key");
                let mouse = self.row_mouse_states[&key].clone();
                let hash = commit.hash.clone();
                let selected = self.selected_commit.as_ref() == Some(&hash);
                Hoverable::new(mouse, move |hovered| {
                    let decorations = if commit.decorations.is_empty() {
                        String::new()
                    } else {
                        format!("  [{}]", commit.decorations.join(", "))
                    };
                    let content = Flex::column()
                        .with_child(
                            Text::new_inline(
                                format!("{}{}", commit.subject, decorations),
                                appearance.ui_font_family(),
                                appearance.ui_font_size(),
                            )
                            .with_color(theme.main_text_color(theme.background()).into())
                            .finish(),
                        )
                        .with_child(
                            Text::new_inline(
                                format!(
                                    "{}  {}  {}",
                                    commit.short_hash, commit.author_name, commit.authored_at
                                ),
                                appearance.ui_font_family(),
                                10.,
                            )
                            .with_color(theme.sub_text_color(theme.background()).into())
                            .finish(),
                        )
                        .finish();
                    let mut container = Container::new(content)
                        .with_horizontal_padding(12.)
                        .with_vertical_padding(5.);
                    if selected || hovered.is_hovered() {
                        container = container.with_background(theme.surface_overlay_1());
                    }
                    container.finish()
                })
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(VersionControlAction::SelectCommit(hash.clone()));
                })
                .with_cursor(Cursor::PointingHand)
                .finish()
            }
            ListRow::Branch(branch) => {
                let key = row_key(row).expect("branch row should have a key");
                let mouse = self.row_mouse_states[&key].clone();
                let name = branch.name.clone();
                let kind = branch.kind;
                let selected = self.selected_branch.as_ref() == Some(&(name.clone(), kind));
                Hoverable::new(mouse, move |hovered| {
                    let marker = if branch.is_current { "●" } else { " " };
                    let tracking = branch
                        .tracking
                        .as_ref()
                        .map(|tracking| format!(" {tracking}"))
                        .unwrap_or_default();
                    let content = Flex::column()
                        .with_child(
                            Text::new_inline(
                                format!("{marker} {}{tracking}", branch.name),
                                appearance.ui_font_family(),
                                appearance.ui_font_size(),
                            )
                            .with_color(theme.main_text_color(theme.background()).into())
                            .finish(),
                        )
                        .with_child(
                            Text::new_inline(
                                format!("{}  {}", branch.short_hash, branch.subject),
                                appearance.ui_font_family(),
                                10.,
                            )
                            .with_color(theme.sub_text_color(theme.background()).into())
                            .finish(),
                        )
                        .finish();
                    let mut container = Container::new(content)
                        .with_horizontal_padding(12.)
                        .with_vertical_padding(5.);
                    if selected || hovered.is_hovered() {
                        container = container.with_background(theme.surface_overlay_1());
                    }
                    container.finish()
                })
                .on_click(move |ctx, _, _| {
                    ctx.dispatch_typed_action(VersionControlAction::SelectBranch {
                        name: name.clone(),
                        kind,
                    });
                })
                .with_cursor(Cursor::PointingHand)
                .finish()
            }
        }
    }

    fn render_list(&self, app: &AppContext) -> Box<dyn Element> {
        let theme = Appearance::as_ref(app).theme();
        let rows = self.rows.clone();
        let handle = self.handle.clone();
        let list = UniformList::new(
            self.list_state.clone(),
            rows.len(),
            move |range: Range<usize>, app| {
                let Some(view) = handle.upgrade(app) else {
                    return Vec::<Box<dyn Element>>::new().into_iter();
                };
                range
                    .filter_map(|index| rows.get(index))
                    .map(|row| view.as_ref(app).render_row(row, app))
                    .collect::<Vec<_>>()
                    .into_iter()
            },
        );
        Scrollable::vertical(
            self.scroll_state.clone(),
            list.finish_scrollable(),
            ScrollbarWidth::Auto,
            theme.nonactive_ui_detail().into(),
            theme.active_ui_detail().into(),
            Fill::None,
        )
        .with_overlayed_scrollbar()
        .finish()
    }

    fn render_context_actions(&self, appearance: &Appearance) -> Box<dyn Element> {
        match self.tab {
            VersionControlTab::Changes => {
                let mut row = Flex::row().with_spacing(4.);
                if let Some((path, target)) = &self.selected_change {
                    if *target == DiffTarget::Index {
                        row = row.with_child(self.render_button(
                            "Unstage",
                            ButtonVariant::Secondary,
                            self.mouse_states.unstage_selected.clone(),
                            VersionControlAction::UnstageSelected,
                            appearance,
                        ));
                    } else {
                        row = row.with_child(self.render_button(
                            "Stage",
                            ButtonVariant::Secondary,
                            self.mouse_states.stage_selected.clone(),
                            VersionControlAction::StageSelected,
                            appearance,
                        ));
                        let can_discard = self.current_snapshot().is_some_and(|snapshot| {
                            snapshot
                                .changes
                                .iter()
                                .find(|change| &change.path == path)
                                .is_some_and(|change| {
                                    !change.is_untracked() && !change.is_conflicted()
                                })
                        });
                        if can_discard {
                            row = row.with_child(self.render_button(
                                "Rollback",
                                ButtonVariant::Text,
                                self.mouse_states.discard.clone(),
                                VersionControlAction::RequestDiscardSelected,
                                appearance,
                            ));
                        }
                    }
                }
                row.with_child(self.render_button(
                    "Stage all",
                    ButtonVariant::Text,
                    self.mouse_states.stage_all.clone(),
                    VersionControlAction::StageAll,
                    appearance,
                ))
                .with_child(self.render_button(
                    "Unstage all",
                    ButtonVariant::Text,
                    self.mouse_states.unstage_all.clone(),
                    VersionControlAction::UnstageAll,
                    appearance,
                ))
                .with_child(self.render_button(
                    "Stash",
                    ButtonVariant::Text,
                    self.mouse_states.stash.clone(),
                    VersionControlAction::Stash,
                    appearance,
                ))
                .with_child(self.render_button(
                    "Pop",
                    ButtonVariant::Text,
                    self.mouse_states.pop_stash.clone(),
                    VersionControlAction::PopStash,
                    appearance,
                ))
                .finish()
            }
            VersionControlTab::Log => Flex::row().finish(),
            VersionControlTab::Branches => {
                let Some((name, kind)) = &self.selected_branch else {
                    return Flex::row().finish();
                };
                let is_current = self.current_snapshot().is_some_and(|snapshot| {
                    snapshot.branches.iter().any(|branch| {
                        branch.name == *name && branch.kind == *kind && branch.is_current
                    })
                });
                let mut row = Flex::row().with_spacing(4.);
                if !is_current {
                    row = row
                        .with_child(self.render_button(
                            "Checkout",
                            ButtonVariant::Secondary,
                            self.mouse_states.checkout.clone(),
                            VersionControlAction::CheckoutSelectedBranch,
                            appearance,
                        ))
                        .with_child(self.render_button(
                            "Merge",
                            ButtonVariant::Text,
                            self.mouse_states.merge.clone(),
                            VersionControlAction::MergeSelectedBranch,
                            appearance,
                        ));
                    if *kind == BranchKind::Local {
                        row = row.with_child(self.render_button(
                            "Delete",
                            ButtonVariant::Text,
                            self.mouse_states.delete_branch.clone(),
                            VersionControlAction::RequestDeleteSelectedBranch,
                            appearance,
                        ));
                    }
                }
                row.finish()
            }
        }
    }

    fn render_footer(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let mut footer = Flex::column().with_spacing(6.);
        if !self.detail.is_empty() {
            footer = footer.with_child(
                ConstrainedBox::new(
                    ClippedScrollable::vertical(
                        self.detail_scroll_state.clone(),
                        Container::new(
                            Text::new(self.detail.clone(), appearance.ui_font_family(), 10.)
                                .with_color(theme.main_text_color(theme.background()).into())
                                .finish(),
                        )
                        .with_uniform_padding(8.)
                        .with_background(theme.surface_2())
                        .finish(),
                        ScrollbarWidth::Auto,
                        theme.nonactive_ui_detail().into(),
                        theme.active_ui_detail().into(),
                        Fill::None,
                    )
                    .with_overlayed_scrollbar()
                    .finish(),
                )
                .with_max_height(180.)
                .finish(),
            );
        }
        match self.tab {
            VersionControlTab::Changes => {
                footer = footer
                    .with_child(Self::render_editor(&self.commit_editor, app))
                    .with_child(self.render_button(
                        "Commit staged",
                        ButtonVariant::Accent,
                        self.mouse_states.commit.clone(),
                        VersionControlAction::Commit,
                        appearance,
                    ));
            }
            VersionControlTab::Log => {}
            VersionControlTab::Branches => {
                footer = footer.with_child(
                    Flex::row()
                        .with_spacing(6.)
                        .with_child(
                            Shrinkable::new(1., Self::render_editor(&self.branch_editor, app))
                                .finish(),
                        )
                        .with_child(self.render_button(
                            "Create",
                            ButtonVariant::Secondary,
                            self.mouse_states.create_branch.clone(),
                            VersionControlAction::CreateBranch,
                            appearance,
                        ))
                        .finish(),
                );
            }
        }
        if let Some(pending) = &self.pending_destructive_operation {
            let warning = match pending {
                PendingDestructiveOperation::Discard { paths } => format!(
                    "Discard uncommitted changes in {}? This cannot be undone.",
                    paths
                        .first()
                        .map(|path| path.to_string_lossy())
                        .unwrap_or_default()
                ),
                PendingDestructiveOperation::DeleteBranch { name } => {
                    format!("Delete local branch {name}? Git will refuse if it is unmerged.")
                }
            };
            footer = footer
                .with_child(
                    Text::new(warning, appearance.ui_font_family(), 10.)
                        .with_color(theme.ui_error_color())
                        .finish(),
                )
                .with_child(
                    Flex::row()
                        .with_spacing(6.)
                        .with_child(self.render_button(
                            "Confirm",
                            ButtonVariant::Accent,
                            self.mouse_states.confirm_destructive.clone(),
                            VersionControlAction::ConfirmDestructive,
                            appearance,
                        ))
                        .with_child(self.render_button(
                            "Cancel",
                            ButtonVariant::Text,
                            self.mouse_states.cancel_destructive.clone(),
                            VersionControlAction::CancelDestructive,
                            appearance,
                        ))
                        .finish(),
                );
        }
        if self
            .current_snapshot()
            .is_some_and(|snapshot| snapshot.changes_truncated)
        {
            footer = footer.with_child(
                Text::new_inline(
                    "Change list truncated at 1,000 files.",
                    appearance.ui_font_family(),
                    10.,
                )
                .with_color(theme.sub_text_color(theme.background()).into())
                .finish(),
            );
        }
        if let Some(message) = &self.status_message {
            footer = footer.with_child(
                Text::new_inline(message.clone(), appearance.ui_font_family(), 10.)
                    .with_color(theme.sub_text_color(theme.background()).into())
                    .finish(),
            );
        }
        if let Some(message) = &self.error_message {
            footer = footer.with_child(
                Text::new(message.clone(), appearance.ui_font_family(), 10.)
                    .with_color(theme.ui_error_color())
                    .finish(),
            );
        }
        Container::new(footer.finish())
            .with_uniform_padding(8.)
            .with_border(Border::top(1.).with_border_color(theme.outline().into()))
            .finish()
    }
}

impl TypedActionView for VersionControlView {
    type Action = VersionControlAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            VersionControlAction::SetTab(tab) => self.set_tab(*tab, ctx),
            VersionControlAction::Refresh => self.refresh(ctx),
            VersionControlAction::CycleRepository => {
                if !self.snapshots.is_empty() {
                    self.selected_repository =
                        (self.selected_repository + 1) % self.snapshots.len();
                    self.selected_change = None;
                    self.selected_commit = None;
                    self.selected_branch = None;
                    self.detail.clear();
                    self.detail_scroll_state = ClippedScrollStateHandle::default();
                    self.rebuild_rows(ctx);
                    ctx.notify();
                }
            }
            VersionControlAction::SelectChange { path, target } => {
                self.select_change(path.clone(), *target, ctx)
            }
            VersionControlAction::StageSelected => self.run_path_operation(true, false, ctx),
            VersionControlAction::UnstageSelected => self.run_path_operation(false, false, ctx),
            VersionControlAction::StageAll => self.run_path_operation(true, true, ctx),
            VersionControlAction::UnstageAll => self.run_path_operation(false, true, ctx),
            VersionControlAction::RequestDiscardSelected => {
                let Some((_, target)) = self.selected_change.as_ref() else {
                    return;
                };
                if *target != DiffTarget::Worktree {
                    return;
                }
                let Some(paths) = self.selected_change_paths() else {
                    return;
                };
                let selected = self.current_snapshot().and_then(|snapshot| {
                    snapshot
                        .changes
                        .iter()
                        .find(|change| paths.first() == Some(&change.path))
                });
                if selected.is_some_and(GitChange::is_untracked) {
                    self.error_message = Some(
                        "Unversioned files are not deleted from the Version Control panel"
                            .to_string(),
                    );
                } else if selected.is_some_and(GitChange::is_conflicted) {
                    self.error_message =
                        Some("Resolve the merge conflict before rolling it back".to_string());
                } else {
                    self.pending_destructive_operation =
                        Some(PendingDestructiveOperation::Discard { paths });
                }
                ctx.notify();
            }
            VersionControlAction::SelectCommit(hash) => self.select_commit(hash.clone(), ctx),
            VersionControlAction::SelectBranch { name, kind } => {
                self.selected_branch = Some((name.clone(), *kind));
                ctx.notify();
            }
            VersionControlAction::CheckoutSelectedBranch => {
                let Some(root) = self
                    .current_snapshot()
                    .map(|snapshot| snapshot.root.clone())
                else {
                    return;
                };
                let Some((name, kind)) = self.selected_branch.clone() else {
                    return;
                };
                self.run_git_operation(
                    GitOperation::Checkout,
                    "Checked out branch",
                    async move {
                        match kind {
                            BranchKind::Local => checkout_branch(&root, &name, None).await,
                            BranchKind::Remote => checkout_remote_branch(&root, &name, None).await,
                        }
                    },
                    ctx,
                );
            }
            VersionControlAction::MergeSelectedBranch => {
                let Some(root) = self
                    .current_snapshot()
                    .map(|snapshot| snapshot.root.clone())
                else {
                    return;
                };
                let Some((branch, _)) = self.selected_branch.clone() else {
                    return;
                };
                self.run_git_operation(
                    GitOperation::MergeBranch,
                    "Merged branch",
                    async move { merge_branch(&root, &branch, None).await },
                    ctx,
                );
            }
            VersionControlAction::RequestDeleteSelectedBranch => {
                let Some((name, BranchKind::Local)) = self.selected_branch.clone() else {
                    return;
                };
                let is_current = self.current_snapshot().is_some_and(|snapshot| {
                    snapshot
                        .branches
                        .iter()
                        .any(|branch| branch.name == name && branch.is_current)
                });
                if !is_current {
                    self.pending_destructive_operation =
                        Some(PendingDestructiveOperation::DeleteBranch { name });
                    ctx.notify();
                }
            }
            VersionControlAction::CreateBranch => {
                let Some(root) = self
                    .current_snapshot()
                    .map(|snapshot| snapshot.root.clone())
                else {
                    return;
                };
                let branch = self.branch_editor.as_ref(ctx).buffer_text(ctx);
                let branch = branch.trim().to_string();
                if branch.is_empty() {
                    self.error_message = Some("Enter a branch name".to_string());
                    ctx.notify();
                    return;
                }
                self.run_git_operation(
                    GitOperation::CreateBranch,
                    "Created branch",
                    async move { create_branch(&root, &branch, None).await },
                    ctx,
                );
            }
            VersionControlAction::ConfirmDestructive => {
                let Some(root) = self
                    .current_snapshot()
                    .map(|snapshot| snapshot.root.clone())
                else {
                    return;
                };
                let Some(pending) = self.pending_destructive_operation.take() else {
                    return;
                };
                match pending {
                    PendingDestructiveOperation::Discard { paths } => self.run_git_operation(
                        GitOperation::Rollback,
                        "Rolled back changes",
                        async move { discard_paths(&root, &paths, None).await },
                        ctx,
                    ),
                    PendingDestructiveOperation::DeleteBranch { name } => self.run_git_operation(
                        GitOperation::DeleteBranch,
                        "Deleted branch",
                        async move { delete_branch(&root, &name, None).await },
                        ctx,
                    ),
                }
            }
            VersionControlAction::CancelDestructive => {
                self.pending_destructive_operation = None;
                ctx.notify();
            }
            VersionControlAction::Stash => {
                if let Some(root) = self
                    .current_snapshot()
                    .map(|snapshot| snapshot.root.clone())
                {
                    self.run_git_operation(
                        GitOperation::Stash,
                        "Stashed changes",
                        async move { stash(&root, None).await },
                        ctx,
                    );
                }
            }
            VersionControlAction::PopStash => {
                if let Some(root) = self
                    .current_snapshot()
                    .map(|snapshot| snapshot.root.clone())
                {
                    self.run_git_operation(
                        GitOperation::PopStash,
                        "Applied stash",
                        async move { pop_stash(&root, None).await },
                        ctx,
                    );
                }
            }
            VersionControlAction::Fetch => {
                if let Some(root) = self
                    .current_snapshot()
                    .map(|snapshot| snapshot.root.clone())
                {
                    self.run_git_operation(
                        GitOperation::Fetch,
                        "Fetched remote changes",
                        async move { fetch(&root, None).await },
                        ctx,
                    );
                }
            }
            VersionControlAction::Pull => {
                if let Some(root) = self
                    .current_snapshot()
                    .map(|snapshot| snapshot.root.clone())
                {
                    self.run_git_operation(
                        GitOperation::Pull,
                        "Pulled remote changes",
                        async move { pull(&root, None).await },
                        ctx,
                    );
                }
            }
            VersionControlAction::Push => {
                if let Some(snapshot) = self.current_snapshot() {
                    let root = snapshot.root.clone();
                    let branch = snapshot.branch.clone();
                    self.run_git_operation(
                        GitOperation::Push,
                        "Pushed branch",
                        async move { run_push(&root, &branch, None).await },
                        ctx,
                    );
                }
            }
            VersionControlAction::Commit => {
                if let Some(root) = self
                    .current_snapshot()
                    .map(|snapshot| snapshot.root.clone())
                {
                    let message = self.commit_editor.as_ref(ctx).buffer_text(ctx);
                    let message = message.trim().to_string();
                    if message.is_empty() {
                        self.error_message = Some("Enter a commit message".to_string());
                        ctx.notify();
                        return;
                    }
                    self.run_git_operation(
                        GitOperation::Commit,
                        "Committed staged changes",
                        async move { run_commit(&root, &message, false, None).await },
                        ctx,
                    );
                }
            }
        }
    }
}

impl View for VersionControlView {
    fn ui_name() -> &'static str {
        "VersionControlView"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);
        let theme = appearance.theme();
        let repository_label = self.current_snapshot().map_or_else(
            || {
                if self.loading {
                    "Loading repositories\u{2026}".to_string()
                } else {
                    "No Git repository".to_string()
                }
            },
            |snapshot| {
                let name = snapshot
                    .root
                    .file_name()
                    .map(|name| name.to_string_lossy())
                    .unwrap_or_default();
                let tracking = match (snapshot.ahead, snapshot.behind) {
                    (0, 0) => String::new(),
                    (ahead, 0) => format!("  ↑{ahead}"),
                    (0, behind) => format!("  ↓{behind}"),
                    (ahead, behind) => format!("  ↑{ahead} ↓{behind}"),
                };
                format!("{name}  {}{tracking}", snapshot.branch)
            },
        );
        let repository = self.render_button(
            &repository_label,
            ButtonVariant::Text,
            self.mouse_states.repository.clone(),
            VersionControlAction::CycleRepository,
            appearance,
        );
        let top = Flex::column()
            .with_spacing(6.)
            .with_child(
                Flex::row()
                    .with_main_axis_size(MainAxisSize::Max)
                    .with_main_axis_alignment(MainAxisAlignment::SpaceBetween)
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_child(Shrinkable::new(1., repository).finish())
                    .with_child(self.render_toolbar(appearance))
                    .finish(),
            )
            .with_child(self.render_tabs(appearance))
            .with_child(Self::render_editor(&self.filter_editor, app))
            .with_child(self.render_context_actions(appearance))
            .finish();
        let empty = self.snapshots.is_empty() || self.rows.is_empty();
        let content = if empty {
            Container::new(
                Text::new(
                    if self.snapshots.is_empty() {
                        "Open a directory inside a Git repository to use Version Control."
                    } else {
                        "No matching items."
                    },
                    appearance.ui_font_family(),
                    appearance.ui_font_size(),
                )
                .with_color(theme.sub_text_color(theme.background()).into())
                .finish(),
            )
            .with_uniform_padding(16.)
            .finish()
        } else {
            self.render_list(app)
        };
        Container::new(
            Flex::column()
                .with_main_axis_size(MainAxisSize::Max)
                .with_child(Container::new(top).with_uniform_padding(8.).finish())
                .with_child(Shrinkable::new(1., content).finish())
                .with_child(self.render_footer(app))
                .finish(),
        )
        .finish()
    }
}

fn matches_filter(value: &str, filter: &str) -> bool {
    filter.is_empty() || value.to_lowercase().contains(filter)
}

fn row_key(row: &ListRow) -> Option<String> {
    match row {
        ListRow::Section { .. } => None,
        ListRow::Change { change, target } => Some(format!(
            "change:{}:{target:?}",
            change.path.to_string_lossy()
        )),
        ListRow::Commit(commit) => Some(format!("commit:{}", commit.hash)),
        ListRow::Branch(branch) => Some(format!("branch:{:?}:{}", branch.kind, branch.name)),
    }
}

fn truncate_detail(mut value: String, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value;
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value.push_str("\n\n… output truncated …");
    value
}
