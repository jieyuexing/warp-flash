use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Arc;

use ai::workspace::WorkspaceMetadata;
use chrono::{Local, Utc};
use cloud_object_persistence::to_cloud_object_permissions;
use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel_migrations::MigrationHarness;
use pathfinder_geometry::rect::RectF;
use pathfinder_geometry::vector::Vector2F;
use warp_core::features::FeatureFlag;
use warp_graphql::scalars::time::ServerTimestamp;

use super::{
    app_database_file_path, database_file_path_for_current_scope, database_file_path_for_scope,
    decode_path, deduplicate_events, encode_path, get_all_codebase_index_metadata,
    read_sqlite_data, save_app_state, save_codebase_index_metadata, setup_database, start_writer,
};
use crate::app_state::{
    AppState, ArchivedTabSnapshot, CodePaneSnapShot, CodePaneTabSnapshot, LeafContents,
    LeafSnapshot, PaneNodeSnapshot, TabGroupSnapshot, TabSnapshot, TerminalPaneSnapshot,
    WindowSnapshot,
};
use crate::auth::UserUid;
use crate::cloud_object::{CloudObjectPermissions, Owner};
use crate::code::editor_management::CodeSource;
use crate::external_cli_resume::{ExternalCliAgent, ExternalCliResumeTarget};
use crate::notebooks::{CloudNotebook, CloudNotebookModel};
use crate::persistence::model::ObjectPermissions;
use crate::persistence::{
    BlockCompleted, ModelEvent, PersistedDataScope, PersistenceScope, StartedCommandMetadata,
};
use crate::server::ids::{ClientId, ServerId};
use crate::tab::SelectedTabColor;
use crate::terminal::ShellLaunchData;
use crate::terminal::model::block::SerializedBlock;
use crate::terminal::model::session::SessionId;
use crate::themes::theme::AnsiColorIdentifier;
use crate::workspace::tab_group::TabGroupId;
use crate::workspaces::team::{MembershipRole, Team, TeamMember};
use crate::workspaces::user_profiles::UserProfileWithUID;
use crate::workspaces::workspace::Workspace;

#[test]
fn app_scope_database_path_matches_app_database_path() {
    assert_eq!(
        database_file_path_for_scope(&PersistenceScope::App),
        app_database_file_path()
    );
}

#[test]
fn tui_scope_database_path_is_tui_subdirectory_of_app_database_dir() {
    let tui_path = database_file_path_for_scope(&PersistenceScope::Tui);
    let app_path = database_file_path_for_scope(&PersistenceScope::App);

    assert_ne!(tui_path, app_path);
    assert_eq!(
        tui_path,
        warp_core::paths::tui_state_dir().join("warp.sqlite")
    );

    // The TUI database lives in a `tui` subdirectory of the same base
    // directory that holds the GUI database, so the two front-ends never
    // share (or migrate) each other's database.
    let tui_dir = tui_path
        .parent()
        .expect("TUI database path should have a parent");
    assert_eq!(tui_dir.file_name(), Some(OsStr::new("tui")));
    assert_eq!(tui_dir.parent(), app_path.parent());
}

#[test]
fn database_path_for_current_scope_defaults_to_app_scope() {
    // Unit tests never call `persistence::initialize`, so the process-wide
    // scope defaults to `App` and ad-hoc read-only connections resolve to
    // the GUI database. (nextest runs each test in its own process, so no
    // other test can have set the scope.)
    assert_eq!(
        database_file_path_for_current_scope(),
        app_database_file_path()
    );
}

#[test]
fn remote_server_daemon_scope_database_path_uses_identity_data_dir() {
    let path = database_file_path_for_scope(&PersistenceScope::RemoteServerDaemon {
        identity_key: "user@example.com/ssh host".to_string(),
    });
    let expected_data_dir =
        remote_server::setup::remote_server_daemon_data_dir("user@example.com/ssh host");

    assert!(path.is_absolute());
    assert_eq!(
        path,
        PathBuf::from(shellexpand::tilde(&expected_data_dir).into_owned()).join("warp.sqlite")
    );
}

#[test]
fn remote_server_daemon_scope_database_path_handles_empty_identity_key() {
    let path = database_file_path_for_scope(&PersistenceScope::RemoteServerDaemon {
        identity_key: String::new(),
    });
    let expected_data_dir = remote_server::setup::remote_server_daemon_data_dir("");

    assert_eq!(
        path,
        PathBuf::from(shellexpand::tilde(&expected_data_dir).into_owned()).join("warp.sqlite")
    );
}

#[cfg(unix)]
#[test]
fn remote_server_daemon_database_permissions_are_owner_only() {
    use std::fs::Permissions;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let daemon_dir = tempdir.path().join("daemon");
    let database_path = daemon_dir.join("warp.sqlite");

    std::fs::create_dir_all(&daemon_dir).expect("daemon dir should be created");
    std::fs::set_permissions(&daemon_dir, Permissions::from_mode(0o755))
        .expect("daemon dir permissions should be set");
    std::fs::write(&database_path, b"").expect("database file should be created");
    std::fs::set_permissions(&database_path, Permissions::from_mode(0o644))
        .expect("database file permissions should be set");

    super::ensure_owner_only_dir(&daemon_dir).expect("daemon dir should be owner-only");
    super::ensure_owner_only_file(&database_path).expect("database file should be owner-only");

    assert_eq!(daemon_dir.metadata().unwrap().mode() & 0o777, 0o700);
    assert_eq!(database_path.metadata().unwrap().mode() & 0o777, 0o600);
}

fn test_codebase_metadata(path: &str) -> WorkspaceMetadata {
    WorkspaceMetadata {
        path: PathBuf::from(path),
        navigated_ts: Some(Utc::now()),
        modified_ts: None,
        queried_ts: None,
    }
}

#[test]
fn sqlite_read_restores_app_state_and_codebase_metadata() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let app_state = AppState {
        windows: vec![test_terminal_window_snapshot(false)],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };
    save_app_state(&mut conn, &app_state).expect("app state should save");

    let metadata = test_codebase_metadata("/tmp/remote-repo");
    save_codebase_index_metadata(&mut conn, metadata.clone())
        .expect("codebase index metadata should save");
    let restored = read_sqlite_data(&mut conn, None, PersistedDataScope::Full)
        .expect("persisted data should load");
    let restored_app_state = restored
        .app_state
        .expect("app state should be present for the full scope");
    assert_eq!(restored_app_state.windows.len(), 1);
    assert_eq!(restored.codebase_indices.len(), 1);
    assert_eq!(restored.codebase_indices[0].path, metadata.path);
}

/// Mirrors `init_db(&PersistenceScope::Tui)` in an isolated tempdir: the TUI
/// database lives in a `tui/` subdirectory, runs the same migrations, and
/// round-trips a write+read using the TUI's `PersistedDataScope`.
#[test]
fn tui_database_in_tui_subdirectory_round_trips_data() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("tui").join("warp.sqlite");
    std::fs::create_dir_all(
        database_path
            .parent()
            .expect("database path should have a parent"),
    )
    .expect("tui subdirectory should be created");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let metadata = test_codebase_metadata("/tmp/tui-repo");
    save_codebase_index_metadata(&mut conn, metadata.clone())
        .expect("codebase index metadata should save");
    let writer = start_writer(conn, database_path.clone()).expect("writer should start");
    writer
        .sender
        .send(ModelEvent::InsertCommand {
            metadata: StartedCommandMetadata {
                command: "ls".to_owned(),
                start_ts: Some(Local::now()),
                pwd: Some("/tmp/tui-repo".to_owned()),
                shell: Some("zsh".to_owned()),
                username: Some("test-user".to_owned()),
                hostname: Some("test-host".to_owned()),
                session_id: Some(SessionId::from(1)),
                git_branch: None,
                cloud_workflow_id: None,
                workflow_command: None,
                is_agent_executed: false,
            },
        })
        .expect("insert command event should send");
    writer
        .sender
        .send(ModelEvent::UpsertUserProfiles {
            profiles: vec![UserProfileWithUID {
                firebase_uid: UserUid::new("creator-uid"),
                display_name: Some("MCP Creator".to_owned()),
                email: "creator@example.com".to_owned(),
                photo_url: String::new(),
            }],
        })
        .expect("user profile event should send");
    writer
        .sender
        .send(ModelEvent::Terminate)
        .expect("terminate event should send");
    writer.handle.join().expect("writer should terminate");

    let mut conn = setup_database(&database_path).expect("database should reopen");

    let restored = read_sqlite_data(&mut conn, None, PersistedDataScope::TuiFrontend)
        .expect("persisted data should load");
    // The TUI data scope skips GUI session restoration...
    assert!(restored.app_state.is_none());
    // ...but restores command history and shared data like creator profiles and
    // codebase index metadata.
    assert_eq!(restored.command_history.len(), 1);
    assert_eq!(restored.command_history[0].command, "ls");
    assert_eq!(restored.user_profiles.len(), 1);
    assert_eq!(
        restored.user_profiles[0].display_name.as_deref(),
        Some("MCP Creator")
    );
    assert_eq!(restored.codebase_indices.len(), 1);
    assert_eq!(restored.codebase_indices[0].path, metadata.path);
}

#[test]
fn sqlite_writer_reuses_codebase_index_metadata_events() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let conn = setup_database(&database_path).expect("database should initialize");

    let writer = start_writer(conn, database_path.clone()).expect("writer should start");
    let metadata = test_codebase_metadata("/tmp/writer-repo");
    writer
        .sender
        .send(ModelEvent::UpsertCodebaseIndexMetadata {
            index_metadata: Box::new(metadata.clone()),
        })
        .expect("upsert event should send");
    writer
        .sender
        .send(ModelEvent::Terminate)
        .expect("terminate event should send");
    writer.handle.join().expect("writer should terminate");

    let mut conn = setup_database(&database_path).expect("database should reopen");
    let restored = get_all_codebase_index_metadata(&mut conn).expect("metadata should load");
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].path, metadata.path);

    let writer = start_writer(conn, database_path.clone()).expect("writer should restart");
    writer
        .sender
        .send(ModelEvent::DeleteCodebaseIndexMetadata {
            repo_path: metadata.path,
        })
        .expect("delete event should send");
    writer
        .sender
        .send(ModelEvent::Terminate)
        .expect("terminate event should send");
    writer.handle.join().expect("writer should terminate");

    let mut conn = setup_database(&database_path).expect("database should reopen");
    let restored = get_all_codebase_index_metadata(&mut conn).expect("metadata should load");
    assert!(restored.is_empty());
}
#[test]
fn test_deduplicate_snapshots() {
    let local_notebook = CloudNotebook::new_local(
        CloudNotebookModel {
            title: "Hello".to_string(),
            data: "World".to_string(),
            ai_document_id: None,
            conversation_id: None,
        },
        Owner::mock_current_user(),
        None,
        ClientId::new(),
    );
    let completed_block_1 = BlockCompleted {
        pane_id: vec![1, 2, 3],
        block: Arc::new(SerializedBlock::default()),
        is_local: true,
    };
    let completed_block_2 = BlockCompleted {
        pane_id: vec![4, 5, 6],
        block: Arc::new(SerializedBlock::default()),
        is_local: true,
    };
    let snapshot_1 = AppState {
        active_window_index: Some(1),
        block_lists: Default::default(),
        windows: Default::default(),
        running_mcp_servers: Default::default(),
    };
    let snapshot_2 = AppState {
        active_window_index: Some(2),
        block_lists: Default::default(),
        windows: Default::default(),
        running_mcp_servers: Default::default(),
    };
    let snapshot_3 = AppState {
        active_window_index: Some(3),
        block_lists: Default::default(),
        windows: Default::default(),
        running_mcp_servers: Default::default(),
    };

    let original_events = vec![
        ModelEvent::UpsertNotebook {
            notebook: local_notebook.clone(),
        },
        ModelEvent::Snapshot(snapshot_1.clone()),
        ModelEvent::SaveBlock(completed_block_1.clone()),
        ModelEvent::Snapshot(snapshot_2.clone()),
        ModelEvent::SaveBlock(completed_block_2.clone()),
        ModelEvent::Snapshot(snapshot_3.clone()),
        ModelEvent::UpsertNotebook {
            notebook: local_notebook.clone(),
        },
    ];

    let filtered_events = deduplicate_events(original_events);
    assert_eq!(filtered_events.len(), 5);

    assert!(matches!(
        &filtered_events[0],
        &ModelEvent::UpsertNotebook { .. }
    ));
    // The first snapshot should have been filtered out.
    assert!(matches!(&filtered_events[1], &ModelEvent::SaveBlock(_)));
    // The second snapshot should have been filtered out.
    assert!(matches!(&filtered_events[2], &ModelEvent::SaveBlock(_)));
    // The third snapshot should be preserved.
    match &filtered_events[3] {
        ModelEvent::Snapshot(snapshot) => assert_eq!(snapshot, &snapshot_3),
        other => panic!("Expected ModelEvent::Snapshot, got {other:?}"),
    }
    assert!(matches!(
        &filtered_events[4],
        &ModelEvent::UpsertNotebook { .. }
    ));
}

#[test]
fn test_deduplicate_no_snapshots() {
    let original_events = vec![ModelEvent::SaveBlock(BlockCompleted {
        pane_id: vec![1, 2, 3],
        block: Default::default(),
        is_local: true,
    })];
    let filtered_events = deduplicate_events(original_events);
    assert_eq!(filtered_events.len(), 1);
    assert!(matches!(&filtered_events[0], &ModelEvent::SaveBlock(_)));
}

fn test_terminal_window_snapshot(vertical_tabs_panel_open: bool) -> WindowSnapshot {
    WindowSnapshot {
        tabs: vec![TabSnapshot {
            id: uuid::Uuid::new_v4(),
            custom_title: None,
            root: PaneNodeSnapshot::Leaf(LeafSnapshot {
                is_focused: true,
                custom_vertical_tabs_title: None,
                contents: LeafContents::Terminal(TerminalPaneSnapshot {
                    uuid: vec![u8::from(vertical_tabs_panel_open) + 1],
                    cwd: Some("/tmp".to_string()),
                    shell_launch_data: Some(ShellLaunchData::Executable {
                        executable_path: PathBuf::from("/bin/zsh"),
                        shell_type: crate::terminal::shell::ShellType::Zsh,
                    }),
                    is_active: true,
                    is_read_only: false,
                    input_config: None,
                    llm_model_override: None,
                    active_profile_id: None,
                    conversation_ids_to_restore: vec![],
                    external_cli_resume_target: None,
                    active_conversation_id: None,
                }),
            }),
            default_directory_color: None,
            selected_color: SelectedTabColor::default(),
            left_panel: None,
            right_panel: None,
            group_id: None,
            pinned: false,
        }],
        archived_tabs: vec![],
        active_tab_index: 0,
        team_uid: None,
        bounds: None,
        fullscreen_state: Default::default(),
        quake_mode: false,
        universal_search_width: None,
        warp_ai_width: None,
        voltron_width: None,
        warp_drive_index_width: None,
        left_panel_open: false,
        vertical_tabs_panel_open,
        vertical_tabs_panel_width: None,
        archived_tabs_expanded: false,
        left_panel_width: None,
        right_panel_width: None,
        agent_management_filters: None,
        tab_groups: vec![],
    }
}

#[test]
fn test_sqlite_round_trips_vertical_tabs_panel_state_and_tab_identity() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let mut collapsed = test_terminal_window_snapshot(false);
    collapsed.vertical_tabs_panel_width = Some(56.0);
    let collapsed_tab_id = collapsed.tabs[0].id;
    let mut expanded = test_terminal_window_snapshot(true);
    expanded.vertical_tabs_panel_width = Some(248.0);
    expanded.archived_tabs_expanded = true;
    let expanded_tab_id = expanded.tabs[0].id;
    let app_state = AppState {
        windows: vec![collapsed, expanded],
        active_window_index: Some(1),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    let restored = read_sqlite_data(&mut conn, None, PersistedDataScope::Full)
        .expect("app state should load")
        .app_state
        .expect("app state should be present for the full scope");

    assert_eq!(restored.active_window_index, Some(1));
    assert_eq!(
        restored
            .windows
            .iter()
            .map(|window| window.vertical_tabs_panel_open)
            .collect::<Vec<_>>(),
        vec![false, true]
    );
    assert_eq!(restored.windows[0].vertical_tabs_panel_width, Some(56.0));
    assert_eq!(restored.windows[1].vertical_tabs_panel_width, Some(248.0));
    assert!(!restored.windows[0].archived_tabs_expanded);
    assert!(restored.windows[1].archived_tabs_expanded);
    assert_eq!(restored.windows[0].tabs[0].id, collapsed_tab_id);
    assert_eq!(restored.windows[1].tabs[0].id, expanded_tab_id);

    save_app_state(&mut conn, &restored).expect("restored app state should save again");
    let restored_again = read_sqlite_data(&mut conn, None, PersistedDataScope::Full)
        .expect("app state should load a second time")
        .app_state
        .expect("app state should remain present");
    assert_eq!(restored_again.windows[0].tabs[0].id, collapsed_tab_id);
    assert_eq!(restored_again.windows[1].tabs[0].id, expanded_tab_id);
}

#[test]
fn tab_identity_migration_preserves_existing_archive_ids() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    conn.revert_last_migration(persistence::MIGRATIONS)
        .expect("latest migration should revert");
    conn.batch_execute(
        "INSERT INTO windows (active_tab_index, quake_mode, fullscreen_state)
         VALUES (0, FALSE, 0);
         INSERT INTO tabs (window_id, pinned, archive_id, archived, archived_at)
         VALUES (
             1,
             FALSE,
             '00000000-0000-0000-0000-000000000042',
             TRUE,
             42
         );",
    )
    .expect("pre-migration archived tab should insert");

    conn.run_pending_migrations(persistence::MIGRATIONS)
        .expect("latest migration should reapply");

    let identities = crate::persistence::schema::tabs::table
        .select(crate::persistence::schema::tabs::persistent_id)
        .load::<Option<String>>(&mut conn)
        .expect("migrated tab identities should load");
    assert_eq!(
        identities,
        vec![Some("00000000-0000-0000-0000-000000000042".to_string())]
    );

    let sidebar_state = crate::persistence::schema::windows::table
        .select((
            crate::persistence::schema::windows::vertical_tabs_panel_width,
            crate::persistence::schema::windows::archived_tabs_expanded,
        ))
        .first::<(Option<f32>, bool)>(&mut conn)
        .expect("migrated sidebar state should load");
    assert_eq!(sidebar_state, (None, false));
}

#[test]
fn test_sqlite_round_trips_archived_tab_with_pane_tree() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");
    let archive_id = uuid::Uuid::new_v4();
    let group_id = TabGroupId::new();
    let mut window = test_terminal_window_snapshot(true);
    let mut archived_tab = window.tabs[0].clone();
    archived_tab.id = archive_id;
    archived_tab.custom_title = Some("Archived deployment".to_string());
    archived_tab.group_id = Some(group_id);
    if let PaneNodeSnapshot::Leaf(LeafSnapshot {
        contents: LeafContents::Terminal(terminal),
        ..
    }) = &mut archived_tab.root
    {
        terminal.uuid = vec![99];
        terminal.cwd = Some("/work/deployment".to_string());
    }
    window.archived_tabs = vec![ArchivedTabSnapshot {
        tab: archived_tab,
        archived_at: 1_788_000_000_000,
    }];
    window.tab_groups = vec![TabGroupSnapshot {
        id: group_id,
        name: Some("Deployments".to_string()),
        color: SelectedTabColor::default(),
        collapsed: false,
        pinned: false,
    }];
    let app_state = AppState {
        windows: vec![window],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");
    let restored = read_sqlite_data(&mut conn, None, PersistedDataScope::Full)
        .expect("app state should load")
        .app_state
        .expect("app state should be present for the full scope");

    let restored_window = &restored.windows[0];
    assert_eq!(restored_window.tabs.len(), 1);
    assert_eq!(restored_window.archived_tabs.len(), 1);
    let archived = &restored_window.archived_tabs[0];
    assert_eq!(archived.tab.id, archive_id);
    assert_eq!(archived.archived_at, 1_788_000_000_000);
    assert_eq!(
        archived.tab.custom_title.as_deref(),
        Some("Archived deployment")
    );
    assert_eq!(
        archived.tab.group_id,
        Some(restored_window.tab_groups[0].id)
    );
    assert_eq!(restored_window.tab_groups[0].id, group_id);
    let PaneNodeSnapshot::Leaf(LeafSnapshot {
        contents: LeafContents::Terminal(terminal),
        ..
    }) = &archived.tab.root
    else {
        panic!("archived tab should restore its terminal pane");
    };
    assert_eq!(terminal.cwd.as_deref(), Some("/work/deployment"));
}

#[test]
fn test_sqlite_round_trips_window_team_uid() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");
    let team_uid = ServerId::from(123);
    let mut assigned_window = test_terminal_window_snapshot(false);
    assigned_window.team_uid = Some(team_uid);

    let app_state = AppState {
        windows: vec![assigned_window, test_terminal_window_snapshot(true)],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    let restored = read_sqlite_data(&mut conn, None, PersistedDataScope::Full)
        .expect("app state should load")
        .app_state
        .expect("app state should be present for the full scope");

    assert_eq!(restored.windows[0].team_uid, Some(team_uid));
    assert_eq!(restored.windows[1].team_uid, None);
}

#[test]
fn test_sqlite_round_trips_external_cli_resume_target() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");
    let mut window = test_terminal_window_snapshot(false);
    let target = ExternalCliResumeTarget::new(
        ExternalCliAgent::Codex,
        "019f6965-fb39-7101-9a0c-21706ff06d7b",
        Some("/tmp".to_owned()),
    )
    .expect("valid resume target");

    let PaneNodeSnapshot::Leaf(leaf) = &mut window.tabs[0].root else {
        panic!("test window should contain a terminal leaf");
    };
    let LeafContents::Terminal(terminal) = &mut leaf.contents else {
        panic!("test window should contain a terminal snapshot");
    };
    terminal.external_cli_resume_target = Some(target.clone());

    let app_state = AppState {
        windows: vec![window],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };
    save_app_state(&mut conn, &app_state).expect("app state should save");

    let restored = read_sqlite_data(&mut conn, None, PersistedDataScope::Full)
        .expect("app state should load")
        .app_state
        .expect("app state should be present for the full scope");
    let PaneNodeSnapshot::Leaf(leaf) = &restored.windows[0].tabs[0].root else {
        panic!("restored window should contain a terminal leaf");
    };
    let LeafContents::Terminal(terminal) = &leaf.contents else {
        panic!("restored window should contain a terminal snapshot");
    };
    assert_eq!(terminal.external_cli_resume_target, Some(target));
}

#[test]
fn test_sqlite_round_trips_custom_vertical_tabs_title() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let app_state = AppState {
        windows: vec![WindowSnapshot {
            tabs: vec![TabSnapshot {
                id: uuid::Uuid::new_v4(),
                custom_title: None,
                root: PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: true,
                    custom_vertical_tabs_title: Some("Production API".to_string()),
                    contents: LeafContents::Terminal(TerminalPaneSnapshot {
                        uuid: vec![42],
                        cwd: Some("/tmp".to_string()),
                        shell_launch_data: Some(ShellLaunchData::Executable {
                            executable_path: PathBuf::from("/bin/zsh"),
                            shell_type: crate::terminal::shell::ShellType::Zsh,
                        }),
                        is_active: true,
                        is_read_only: false,
                        input_config: None,
                        llm_model_override: None,
                        active_profile_id: None,
                        conversation_ids_to_restore: vec![],
                        external_cli_resume_target: None,
                        active_conversation_id: None,
                    }),
                }),
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                left_panel: None,
                right_panel: None,
                group_id: None,
                pinned: false,
            }],
            archived_tabs: vec![],
            active_tab_index: 0,
            team_uid: None,
            bounds: None,
            fullscreen_state: Default::default(),
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            vertical_tabs_panel_width: None,
            archived_tabs_expanded: false,
            left_panel_width: None,
            right_panel_width: None,
            agent_management_filters: None,
            tab_groups: vec![],
        }],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    let restored = read_sqlite_data(&mut conn, None, PersistedDataScope::Full)
        .expect("app state should load")
        .app_state
        .expect("app state should be present for the full scope");

    let PaneNodeSnapshot::Leaf(LeafSnapshot {
        custom_vertical_tabs_title,
        ..
    }) = &restored.windows[0].tabs[0].root
    else {
        panic!("Expected terminal pane leaf");
    };
    assert_eq!(
        custom_vertical_tabs_title.as_deref(),
        Some("Production API")
    );
}

#[test]
fn test_sqlite_round_trips_code_pane_with_multiple_tabs() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let app_state = AppState {
        windows: vec![WindowSnapshot {
            tabs: vec![TabSnapshot {
                id: uuid::Uuid::new_v4(),
                custom_title: None,
                root: PaneNodeSnapshot::Leaf(LeafSnapshot {
                    is_focused: true,
                    custom_vertical_tabs_title: None,
                    contents: LeafContents::Code(CodePaneSnapShot::Local {
                        tabs: vec![
                            CodePaneTabSnapshot {
                                path: Some(PathBuf::from("/tmp/main.rs")),
                            },
                            CodePaneTabSnapshot {
                                path: Some(PathBuf::from("/tmp/lib.rs")),
                            },
                            CodePaneTabSnapshot { path: None },
                        ],
                        active_tab_index: 1,
                        source: Some(CodeSource::FileTree {
                            location: crate::code::buffer_location::LocalOrRemotePath::Local(
                                PathBuf::from("/tmp/main.rs"),
                            ),
                        }),
                    }),
                }),
                default_directory_color: None,
                selected_color: SelectedTabColor::default(),
                left_panel: None,
                right_panel: None,
                group_id: None,
                pinned: false,
            }],
            archived_tabs: vec![],
            active_tab_index: 0,
            team_uid: None,
            bounds: None,
            fullscreen_state: Default::default(),
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            vertical_tabs_panel_width: None,
            archived_tabs_expanded: false,
            left_panel_width: None,
            right_panel_width: None,
            agent_management_filters: None,
            tab_groups: vec![],
        }],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    let restored = read_sqlite_data(&mut conn, None, PersistedDataScope::Full)
        .expect("app state should load")
        .app_state
        .expect("app state should be present for the full scope");

    assert_eq!(restored.windows.len(), 1);
    let restored_tab = &restored.windows[0].tabs[0];
    let PaneNodeSnapshot::Leaf(LeafSnapshot {
        contents:
            LeafContents::Code(CodePaneSnapShot::Local {
                tabs,
                active_tab_index,
                source,
            }),
        ..
    }) = &restored_tab.root
    else {
        panic!("Expected code pane leaf");
    };

    assert_eq!(tabs.len(), 3);
    assert_eq!(*active_tab_index, 1);
    assert_eq!(tabs[0].path, Some(PathBuf::from("/tmp/main.rs")));
    assert_eq!(tabs[1].path, Some(PathBuf::from("/tmp/lib.rs")));
    assert_eq!(tabs[2].path, None);
    assert!(matches!(source, Some(CodeSource::FileTree { .. })));
}

/// Verifies that a tab group and its membership round-trip through save/restore.
#[test]
fn test_sqlite_round_trips_tab_groups() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let group_id = TabGroupId::new();
    let tab_in_group = TabSnapshot {
        id: uuid::Uuid::new_v4(),
        custom_title: None,
        root: PaneNodeSnapshot::Leaf(LeafSnapshot {
            is_focused: true,
            custom_vertical_tabs_title: None,
            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: vec![1],
                cwd: Some("/tmp/grouped".to_string()),
                shell_launch_data: Some(ShellLaunchData::Executable {
                    executable_path: PathBuf::from("/bin/zsh"),
                    shell_type: crate::terminal::shell::ShellType::Zsh,
                }),
                is_active: true,
                is_read_only: false,
                input_config: None,
                llm_model_override: None,
                active_profile_id: None,
                conversation_ids_to_restore: vec![],
                external_cli_resume_target: None,
                active_conversation_id: None,
            }),
        }),
        default_directory_color: None,
        selected_color: SelectedTabColor::default(),
        left_panel: None,
        right_panel: None,
        group_id: Some(group_id),
        pinned: false,
    };
    let tab_outside_group = TabSnapshot {
        id: uuid::Uuid::new_v4(),
        custom_title: None,
        root: PaneNodeSnapshot::Leaf(LeafSnapshot {
            is_focused: false,
            custom_vertical_tabs_title: None,
            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: vec![2],
                cwd: Some("/tmp/ungrouped".to_string()),
                shell_launch_data: Some(ShellLaunchData::Executable {
                    executable_path: PathBuf::from("/bin/zsh"),
                    shell_type: crate::terminal::shell::ShellType::Zsh,
                }),
                is_active: false,
                is_read_only: false,
                input_config: None,
                llm_model_override: None,
                active_profile_id: None,
                conversation_ids_to_restore: vec![],
                external_cli_resume_target: None,
                active_conversation_id: None,
            }),
        }),
        default_directory_color: None,
        selected_color: SelectedTabColor::default(),
        left_panel: None,
        right_panel: None,
        group_id: None,
        pinned: false,
    };

    let app_state = AppState {
        windows: vec![WindowSnapshot {
            tabs: vec![tab_in_group, tab_outside_group],
            archived_tabs: vec![],
            active_tab_index: 0,
            team_uid: None,
            bounds: None,
            fullscreen_state: Default::default(),
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            vertical_tabs_panel_width: None,
            archived_tabs_expanded: false,
            left_panel_width: None,
            right_panel_width: None,
            agent_management_filters: None,
            tab_groups: vec![TabGroupSnapshot {
                id: group_id,
                name: Some("Backend".to_string()),
                color: SelectedTabColor::Color(AnsiColorIdentifier::Blue),
                collapsed: true,
                pinned: false,
            }],
        }],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    let restored = read_sqlite_data(&mut conn, None, PersistedDataScope::Full)
        .expect("app state should load")
        .app_state
        .expect("app state should be present for the full scope");

    assert_eq!(restored.windows.len(), 1);
    let restored_window = &restored.windows[0];
    assert_eq!(restored_window.tab_groups.len(), 1);
    let restored_group = &restored_window.tab_groups[0];
    assert_eq!(restored_group.name.as_deref(), Some("Backend"));
    assert_eq!(
        restored_group.color,
        SelectedTabColor::Color(AnsiColorIdentifier::Blue)
    );
    assert!(restored_group.collapsed);

    assert_eq!(restored_group.id, group_id);
    assert_eq!(restored_window.tabs.len(), 2);
    assert_eq!(restored_window.tabs[0].group_id, Some(restored_group.id));
    assert_eq!(restored_window.tabs[1].group_id, None);
}

/// Verifies that the `pinned` flag on tabs and tab groups round-trips through
/// save/restore so the user's pinned layout survives an app restart.
#[test]
fn test_sqlite_round_trips_pinned_state() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let pinned_group_id = TabGroupId::new();
    let unpinned_group_id = TabGroupId::new();

    let pinned_tab = TabSnapshot {
        id: uuid::Uuid::new_v4(),
        custom_title: None,
        root: PaneNodeSnapshot::Leaf(LeafSnapshot {
            is_focused: true,
            custom_vertical_tabs_title: None,
            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: vec![10],
                cwd: Some("/tmp/pinned".to_string()),
                shell_launch_data: Some(ShellLaunchData::Executable {
                    executable_path: PathBuf::from("/bin/zsh"),
                    shell_type: crate::terminal::shell::ShellType::Zsh,
                }),
                is_active: true,
                is_read_only: false,
                input_config: None,
                llm_model_override: None,
                active_profile_id: None,
                conversation_ids_to_restore: vec![],
                external_cli_resume_target: None,
                active_conversation_id: None,
            }),
        }),
        default_directory_color: None,
        selected_color: SelectedTabColor::default(),
        left_panel: None,
        right_panel: None,
        group_id: None,
        pinned: true,
    };
    let unpinned_tab = TabSnapshot {
        id: uuid::Uuid::new_v4(),
        custom_title: None,
        root: PaneNodeSnapshot::Leaf(LeafSnapshot {
            is_focused: false,
            custom_vertical_tabs_title: None,
            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: vec![11],
                cwd: Some("/tmp/unpinned".to_string()),
                shell_launch_data: Some(ShellLaunchData::Executable {
                    executable_path: PathBuf::from("/bin/zsh"),
                    shell_type: crate::terminal::shell::ShellType::Zsh,
                }),
                is_active: false,
                is_read_only: false,
                input_config: None,
                llm_model_override: None,
                active_profile_id: None,
                conversation_ids_to_restore: vec![],
                external_cli_resume_target: None,
                active_conversation_id: None,
            }),
        }),
        default_directory_color: None,
        selected_color: SelectedTabColor::default(),
        left_panel: None,
        right_panel: None,
        group_id: Some(unpinned_group_id),
        pinned: false,
    };
    let tab_in_pinned_group = TabSnapshot {
        id: uuid::Uuid::new_v4(),
        custom_title: None,
        root: PaneNodeSnapshot::Leaf(LeafSnapshot {
            is_focused: false,
            custom_vertical_tabs_title: None,
            contents: LeafContents::Terminal(TerminalPaneSnapshot {
                uuid: vec![12],
                cwd: Some("/tmp/pinned-group".to_string()),
                shell_launch_data: Some(ShellLaunchData::Executable {
                    executable_path: PathBuf::from("/bin/zsh"),
                    shell_type: crate::terminal::shell::ShellType::Zsh,
                }),
                is_active: false,
                is_read_only: false,
                input_config: None,
                llm_model_override: None,
                active_profile_id: None,
                conversation_ids_to_restore: vec![],
                external_cli_resume_target: None,
                active_conversation_id: None,
            }),
        }),
        default_directory_color: None,
        selected_color: SelectedTabColor::default(),
        left_panel: None,
        right_panel: None,
        group_id: Some(pinned_group_id),
        pinned: false,
    };

    let app_state = AppState {
        windows: vec![WindowSnapshot {
            tabs: vec![pinned_tab, tab_in_pinned_group, unpinned_tab],
            archived_tabs: vec![],
            active_tab_index: 0,
            team_uid: None,
            bounds: None,
            fullscreen_state: Default::default(),
            quake_mode: false,
            universal_search_width: None,
            warp_ai_width: None,
            voltron_width: None,
            warp_drive_index_width: None,
            left_panel_open: false,
            vertical_tabs_panel_open: false,
            vertical_tabs_panel_width: None,
            archived_tabs_expanded: false,
            left_panel_width: None,
            right_panel_width: None,
            agent_management_filters: None,
            tab_groups: vec![
                TabGroupSnapshot {
                    id: pinned_group_id,
                    name: Some("Pinned".to_string()),
                    color: SelectedTabColor::default(),
                    collapsed: false,
                    pinned: true,
                },
                TabGroupSnapshot {
                    id: unpinned_group_id,
                    name: Some("Loose".to_string()),
                    color: SelectedTabColor::default(),
                    collapsed: false,
                    pinned: false,
                },
            ],
        }],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    let restored = read_sqlite_data(&mut conn, None, PersistedDataScope::Full)
        .expect("app state should load")
        .app_state
        .expect("app state should be present for the full scope");

    assert_eq!(restored.windows.len(), 1);
    let restored_window = &restored.windows[0];

    // Tabs come back in insertion order; pinned flag should match what we saved.
    assert_eq!(restored_window.tabs.len(), 3);
    assert!(restored_window.tabs[0].pinned);
    assert!(!restored_window.tabs[1].pinned);
    assert!(!restored_window.tabs[2].pinned);

    // Both groups round-trip with their identity and pinned state preserved.
    assert_eq!(restored_window.tab_groups.len(), 2);
    let restored_pinned_group = restored_window
        .tab_groups
        .iter()
        .find(|group| group.name.as_deref() == Some("Pinned"))
        .expect("pinned group should restore");
    let restored_loose_group = restored_window
        .tab_groups
        .iter()
        .find(|group| group.name.as_deref() == Some("Loose"))
        .expect("unpinned group should restore");
    assert!(restored_pinned_group.pinned);
    assert!(!restored_loose_group.pinned);
    assert_eq!(restored_pinned_group.id, pinned_group_id);
    assert_eq!(restored_loose_group.id, unpinned_group_id);
}

fn assert_encode_then_decode_preserves_original_path(original_path: PathBuf) {
    let bytes = encode_path(original_path.clone());
    let decoded_path = decode_path(bytes);
    assert_eq!(original_path, decoded_path);
}

/// Test that a local path can be encoded and decoded. We use this when persisting a local
/// file path for notebooks in sqlite. We need this test because Windows `OsString`s are
/// often arbitrary sequences of 16-bit values, unlike Unix which uses sequences of 8-bit
/// values (bytes). Since `diesel::sql_types::Binary` deals with sequences of bytes (`u8`)
/// we need to perform special casting on `OsString`s on Windows.
#[test]
fn test_path_encode_decode() {
    // Empty path
    assert_encode_then_decode_preserves_original_path(PathBuf::new());

    // Windows-style paths
    assert_encode_then_decode_preserves_original_path(PathBuf::from(r"C:\windows\system32.dll"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from("c:temp"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from(r"\temp"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from(r"\temp\emoji\🙈.txt"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from(r"\temp\ñoñàscii\temp.txt"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from(r"\temp\hindi\हिन्दी"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from(r"\temp\cjk\狗没有耐心"));

    // Unix-style paths
    assert_encode_then_decode_preserves_original_path(PathBuf::from(
        "/home/persistence/example.sql",
    ));
    assert_encode_then_decode_preserves_original_path(PathBuf::from("./database/log.txt"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from("/temp/emoji/🙈.txt"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from("/temp/ñoñàscii/temp.txt"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from("/temp/hindi/हिन्दी"));
    assert_encode_then_decode_preserves_original_path(PathBuf::from("/temp/cjk/狗没有耐心"));
}

#[test]
fn test_deserialize_corrupted_guests() {
    let _ = FeatureFlag::SharedWithMe.override_enabled(true);
    // Use a hardcoded timestamp to ensure this test works on systems with more-than-microsecond
    // precision.
    let permissions_ts_micros = 123456;
    let permissions_ts =
        ServerTimestamp::from_unix_timestamp_micros(permissions_ts_micros).unwrap();

    let db_permissions = ObjectPermissions {
        id: 42,
        object_metadata_id: 10,
        subject_type: "TEAM".to_string(),
        subject_id: Some("7".to_string()),
        subject_uid: "team_uid12345678912345".to_string(),
        permissions_last_updated_at: Some(permissions_ts_micros),
        // This is not a valid set of encoded object guests.
        object_guests: Some(vec![1, 2, 3]),
        anyone_with_link_access_level: None,
        anyone_with_link_source: None,
    };

    // The overall permissions should successfully convert, minus the object guests.
    let cloud_permissions = to_cloud_object_permissions(&db_permissions, None);
    assert_eq!(
        cloud_permissions,
        Some(CloudObjectPermissions {
            owner: Owner::Team {
                team_uid: crate::server::ids::ServerId::from_string_lossy("team_uid12345678912345"),
            },
            permissions_last_updated_ts: Some(permissions_ts),
            anyone_with_link: None,
            guests: vec![],
        })
    );
}

// Regression: GH#10083. The macOS green-tile button could leave a 1px-wide
// window bound in `AppContext::window_bounds`, which previously round-tripped
// through SQLite and restored as an unusable 1px sliver. Bounds below the
// platform minimum window size must be dropped on save.
#[test]
fn test_sqlite_drops_too_small_bounds_on_save() {
    use diesel::prelude::*;

    use crate::persistence::schema::windows;

    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    let mut snapshot = test_terminal_window_snapshot(false);
    snapshot.bounds = Some(RectF::new(
        Vector2F::new(0.0, -1410.0),
        Vector2F::new(1.0, 1410.0),
    ));

    let app_state = AppState {
        windows: vec![snapshot],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };

    save_app_state(&mut conn, &app_state).expect("app state should save");

    // Query the row directly so the assertion isolates the save guard and is
    // not masked by the read-side guard in `read_sqlite_data`.
    let row: (Option<f32>, Option<f32>, Option<f32>, Option<f32>) = windows::dsl::windows
        .select((
            windows::columns::window_width,
            windows::columns::window_height,
            windows::columns::origin_x,
            windows::columns::origin_y,
        ))
        .first(&mut conn)
        .expect("a windows row should have been inserted");

    assert_eq!(
        row,
        (None, None, None, None),
        "save-path guard must persist NULL bound columns for sub-minimum geometry"
    );
}

// Regression: GH#10083. Users whose warp.sqlite already contains a 1px row
// (because they hit the bug on an earlier build) must still recover to default
// geometry on next launch rather than restoring the sliver.
#[test]
fn test_sqlite_drops_too_small_bounds_on_read() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let mut conn = setup_database(&database_path).expect("database should initialize");

    // Save with no bounds so a row exists, then corrupt it directly to bypass
    // the save-path guard and simulate a pre-existing bad row.
    let app_state = AppState {
        windows: vec![test_terminal_window_snapshot(false)],
        active_window_index: Some(0),
        block_lists: Default::default(),
        running_mcp_servers: Default::default(),
    };
    save_app_state(&mut conn, &app_state).expect("app state should save");

    conn.batch_execute(
        "UPDATE windows \
         SET window_width = 1.0, window_height = 1410.0, \
             origin_x = 0.0, origin_y = -1410.0",
    )
    .expect("corrupting update should succeed");

    let restored = read_sqlite_data(&mut conn, None, PersistedDataScope::Full)
        .expect("app state should load")
        .app_state
        .expect("app state should be present for the full scope");

    assert_eq!(restored.windows.len(), 1);
    assert!(
        restored.windows[0].bounds.is_none(),
        "tiny persisted bounds must be discarded on read so users recover from a corrupt DB"
    );
}

#[test]
fn team_member_is_disabled_round_trips_through_sqlite_cache() {
    let tempdir = tempfile::tempdir().expect("tempdir should be created");
    let database_path = tempdir.path().join("warp.sqlite");
    let conn = setup_database(&database_path).expect("database should initialize");

    let team = Team::from_local_cache(
        ServerId::from_string_lossy(format!("{:0>22}", "team")),
        "Team".to_string(),
        None,
        None,
        Some(vec![
            TeamMember {
                uid: UserUid::new("active-user"),
                email: "active@example.com".to_string(),
                role: MembershipRole::User,
                is_disabled: false,
            },
            TeamMember {
                uid: UserUid::new("disabled-user"),
                email: "disabled@example.com".to_string(),
                role: MembershipRole::User,
                is_disabled: true,
            },
        ]),
        None,
    );
    let workspace = Workspace::from_local_cache(
        format!("{:0>22}", "workspace").into(),
        "Workspace".to_string(),
        Some(vec![team]),
        None,
    );

    let writer = start_writer(conn, database_path.clone()).expect("writer should start");
    writer
        .sender
        .send(ModelEvent::UpsertWorkspaces {
            workspaces: vec![workspace],
        })
        .expect("upsert workspaces event should send");
    writer
        .sender
        .send(ModelEvent::Terminate)
        .expect("terminate event should send");
    writer.handle.join().expect("writer should terminate");

    let mut conn = setup_database(&database_path).expect("database should reopen");
    let restored = read_sqlite_data(&mut conn, None, PersistedDataScope::Full)
        .expect("persisted data should load");

    let members = &restored.workspaces[0].teams[0].members;
    let active_member = members
        .iter()
        .find(|member| member.email == "active@example.com")
        .expect("active member should be present");
    let disabled_member = members
        .iter()
        .find(|member| member.email == "disabled@example.com")
        .expect("disabled member should be present");
    assert!(!active_member.is_disabled);
    assert!(disabled_member.is_disabled);
}
