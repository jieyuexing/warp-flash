use command::blocking::Command;
use warp::integration_testing::step::new_step_with_default_assertions;
use warp::integration_testing::terminal::wait_until_bootstrapped_single_pane_for_tab;
use warp::integration_testing::view_getters::workspace_view;
use warp::workspace::WorkspaceAction;
use warpui_core::{App, async_assert, async_assert_eq};

use super::{Builder, new_builder};
use crate::util::write_all_rc_files_for_test;

fn git(root: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git command should run");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn open_version_control_panel(app: &mut App) {
    let window_id = app.read(|ctx| {
        ctx.windows()
            .active_window()
            .expect("should have active window")
    });
    let workspace = workspace_view(app, window_id);
    app.update(|ctx| {
        ctx.dispatch_typed_action_for_view(
            window_id,
            workspace.id(),
            &WorkspaceAction::ToggleVersionControl,
        );
    });
}

pub fn test_version_control_panel_discovers_git_repository() -> Builder {
    new_builder()
        .with_setup(|utils| {
            let test_dir = utils.test_dir();
            let dir_string = test_dir
                .to_str()
                .expect("test directory should be valid UTF-8");
            write_all_rc_files_for_test(&test_dir, format!("cd {dir_string}"));

            git(&test_dir, &["init", "-b", "main"]);
            git(
                &test_dir,
                &["config", "user.name", "Version Control Integration"],
            );
            git(
                &test_dir,
                &[
                    "config",
                    "user.email",
                    "version-control-integration@example.com",
                ],
            );
            std::fs::write(test_dir.join("tracked.txt"), "tracked\n")
                .expect("tracked file should be written");
            git(&test_dir, &["add", "tracked.txt"]);
            git(&test_dir, &["commit", "-m", "initial"]);
        })
        .with_step(wait_until_bootstrapped_single_pane_for_tab(0))
        .with_step(
            new_step_with_default_assertions("Open Version Control panel")
                .with_action(|app, _, _| open_version_control_panel(app))
                .add_named_assertion("Version Control panel is active", |app, window_id| {
                    let workspace = workspace_view(app, window_id);
                    workspace.read(app, |workspace, ctx| {
                        let (is_active, _) = workspace.version_control_panel_debug_state(ctx);
                        async_assert!(
                            workspace.is_left_panel_open(ctx) && is_active,
                            "Version Control should be the visible Tools Panel view"
                        )
                    })
                })
                .add_named_assertion("Git repository is loaded", |app, window_id| {
                    let workspace = workspace_view(app, window_id);
                    workspace.read(app, |workspace, ctx| {
                        let (_, repository_count) =
                            workspace.version_control_panel_debug_state(ctx);
                        async_assert_eq!(
                            repository_count,
                            1,
                            "Version Control should discover the terminal working directory repository"
                        )
                    })
                }),
        )
}
