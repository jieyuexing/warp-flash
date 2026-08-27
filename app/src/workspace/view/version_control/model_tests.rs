use std::path::{Path, PathBuf};

use command::blocking::Command;
use tempfile::TempDir;

use super::{
    BranchKind, ChangeGroup, DiffTarget, checkout_branch, checkout_remote_branch, create_branch,
    delete_branch, discard_paths, discover_repository_roots, load_commit_diff, load_diff,
    load_repository_snapshot, merge_branch, pop_stash, stage_paths, stash, unstage_paths,
};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git command should run");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn repository() -> TempDir {
    let temp = TempDir::new().expect("temporary repository should be created");
    git(temp.path(), &["init", "-b", "main"]);
    git(
        temp.path(),
        &["config", "user.name", "Version Control Test"],
    );
    git(
        temp.path(),
        &["config", "user.email", "version-control@example.com"],
    );
    std::fs::write(temp.path().join("tracked.txt"), "first\n")
        .expect("tracked file should be written");
    git(temp.path(), &["add", "tracked.txt"]);
    git(temp.path(), &["commit", "-m", "initial commit"]);
    temp
}

#[tokio::test]
async fn snapshot_groups_staged_unstaged_and_untracked_changes() {
    let temp = repository();
    std::fs::write(temp.path().join("tracked.txt"), "first\nsecond\n")
        .expect("tracked file should be edited");
    git(temp.path(), &["add", "tracked.txt"]);
    std::fs::write(temp.path().join("tracked.txt"), "first\nsecond\nthird\n")
        .expect("tracked file should be edited again");
    std::fs::write(temp.path().join("new.txt"), "new\n").expect("untracked file should be written");

    let snapshot = load_repository_snapshot(temp.path())
        .await
        .expect("snapshot should load");

    assert_eq!(snapshot.branch, "main");
    assert_eq!(
        snapshot
            .changes_in_group(ChangeGroup::Staged)
            .map(|change| change.path.clone())
            .collect::<Vec<_>>(),
        vec![PathBuf::from("tracked.txt")]
    );
    assert_eq!(
        snapshot
            .changes_in_group(ChangeGroup::Unstaged)
            .map(|change| change.path.clone())
            .collect::<Vec<_>>(),
        vec![PathBuf::from("tracked.txt")]
    );
    assert_eq!(
        snapshot
            .changes_in_group(ChangeGroup::Untracked)
            .map(|change| change.path.clone())
            .collect::<Vec<_>>(),
        vec![PathBuf::from("new.txt")]
    );
}

#[tokio::test]
async fn snapshot_preserves_rename_source_and_destination() {
    let temp = repository();
    git(temp.path(), &["mv", "tracked.txt", "renamed.txt"]);

    let snapshot = load_repository_snapshot(temp.path())
        .await
        .expect("snapshot should load");
    let change = snapshot.changes.first().expect("rename should be present");

    assert_eq!(change.path, PathBuf::from("renamed.txt"));
    assert_eq!(change.original_path, Some(PathBuf::from("tracked.txt")));
    assert_eq!(
        change.groups().collect::<Vec<_>>(),
        vec![ChangeGroup::Staged]
    );
}

#[tokio::test]
async fn stage_and_unstage_paths_update_the_index() {
    let temp = repository();
    std::fs::write(temp.path().join("tracked.txt"), "changed\n")
        .expect("tracked file should be edited");
    let paths = vec![PathBuf::from("tracked.txt")];

    stage_paths(temp.path(), &paths, None)
        .await
        .expect("path should stage");
    let staged = load_repository_snapshot(temp.path())
        .await
        .expect("snapshot should load");
    assert_eq!(staged.changes_in_group(ChangeGroup::Staged).count(), 1);

    unstage_paths(temp.path(), &paths, None)
        .await
        .expect("path should unstage");
    let unstaged = load_repository_snapshot(temp.path())
        .await
        .expect("snapshot should load");
    assert_eq!(unstaged.changes_in_group(ChangeGroup::Unstaged).count(), 1);
    assert_eq!(unstaged.changes_in_group(ChangeGroup::Staged).count(), 0);
}

#[tokio::test]
async fn snapshot_loads_log_and_local_branches() {
    let temp = repository();
    create_branch(temp.path(), "feature", None)
        .await
        .expect("branch should be created");

    let snapshot = load_repository_snapshot(temp.path())
        .await
        .expect("snapshot should load");

    assert_eq!(snapshot.commits[0].subject, "initial commit");
    assert_eq!(snapshot.commits[0].author_name, "Version Control Test");
    assert!(snapshot.branches.iter().any(|branch| {
        branch.kind == BranchKind::Local && branch.name == "feature" && branch.is_current
    }));
    assert!(snapshot.branches.iter().any(|branch| {
        branch.kind == BranchKind::Local && branch.name == "main" && !branch.is_current
    }));
}

#[tokio::test]
async fn snapshot_loads_repository_before_first_commit() {
    let temp = TempDir::new().expect("temporary repository should be created");
    git(temp.path(), &["init", "-b", "main"]);
    let path = temp.path().join("first.txt");
    std::fs::write(&path, "first\n").expect("first file should be written");
    git(temp.path(), &["add", "first.txt"]);

    let snapshot = load_repository_snapshot(temp.path())
        .await
        .expect("snapshot should load");

    assert_eq!(snapshot.branch, "main");
    assert_eq!(snapshot.commits, Vec::new());
    assert_eq!(snapshot.branches, Vec::new());
    assert!(snapshot.changes[0].is_staged());

    unstage_paths(temp.path(), &[PathBuf::from("first.txt")], None)
        .await
        .expect("path should unstage before the first commit");
    let snapshot = load_repository_snapshot(temp.path())
        .await
        .expect("snapshot should reload");
    assert!(snapshot.changes[0].is_untracked());
}

#[tokio::test]
async fn discovers_and_deduplicates_repository_roots() {
    let temp = repository();
    let nested = temp.path().join("nested");
    std::fs::create_dir(&nested).expect("nested directory should be created");

    let roots = discover_repository_roots(&[temp.path().to_path_buf(), nested]).await;

    assert_eq!(
        roots,
        vec![
            temp.path()
                .canonicalize()
                .expect("repository root should canonicalize")
        ]
    );
}

#[tokio::test]
async fn loads_worktree_and_commit_diffs() {
    let temp = repository();
    std::fs::write(temp.path().join("tracked.txt"), "first\nsecond\n")
        .expect("tracked file should be edited");
    let snapshot = load_repository_snapshot(temp.path())
        .await
        .expect("snapshot should load");
    let change = snapshot.changes.first().expect("change should be present");

    let worktree_diff = load_diff(temp.path(), change, DiffTarget::Worktree)
        .await
        .expect("worktree diff should load");
    stage_paths(temp.path(), &[PathBuf::from("tracked.txt")], None)
        .await
        .expect("path should stage");
    let staged = load_repository_snapshot(temp.path())
        .await
        .expect("staged snapshot should load");
    let index_diff = load_diff(temp.path(), &staged.changes[0], DiffTarget::Index)
        .await
        .expect("index diff should load");
    let commit_diff = load_commit_diff(temp.path(), &snapshot.commits[0].hash)
        .await
        .expect("commit diff should load");

    assert!(worktree_diff.contains("+second"));
    assert!(index_diff.contains("+second"));
    assert!(commit_diff.contains("initial commit"));
    assert!(commit_diff.contains("tracked.txt"));
}

#[tokio::test]
async fn snapshot_groups_merge_conflicts_separately() {
    let temp = repository();
    create_branch(temp.path(), "feature", None)
        .await
        .expect("feature branch should be created");
    std::fs::write(temp.path().join("tracked.txt"), "feature\n")
        .expect("feature content should be written");
    git(temp.path(), &["add", "tracked.txt"]);
    git(temp.path(), &["commit", "-m", "feature change"]);
    checkout_branch(temp.path(), "main", None)
        .await
        .expect("main should be checked out");
    std::fs::write(temp.path().join("tracked.txt"), "main\n")
        .expect("main content should be written");
    git(temp.path(), &["add", "tracked.txt"]);
    git(temp.path(), &["commit", "-m", "main change"]);
    merge_branch(temp.path(), "feature", None)
        .await
        .expect_err("conflicting merge should report failure");

    let snapshot = load_repository_snapshot(temp.path())
        .await
        .expect("snapshot should load");

    assert_eq!(
        snapshot
            .changes_in_group(ChangeGroup::Conflicts)
            .map(|change| change.path.clone())
            .collect::<Vec<_>>(),
        vec![PathBuf::from("tracked.txt")]
    );
    assert_eq!(snapshot.changes_in_group(ChangeGroup::Staged).count(), 0);
    assert_eq!(snapshot.changes_in_group(ChangeGroup::Unstaged).count(), 0);
}

#[tokio::test]
async fn discard_paths_restores_tracked_content() {
    let temp = repository();
    std::fs::write(temp.path().join("tracked.txt"), "changed\n")
        .expect("tracked file should be edited");

    discard_paths(temp.path(), &[PathBuf::from("tracked.txt")], None)
        .await
        .expect("change should be discarded");

    assert_eq!(
        std::fs::read_to_string(temp.path().join("tracked.txt"))
            .expect("tracked file should be readable"),
        "first\n"
    );
}

#[tokio::test]
async fn branch_merge_delete_and_stash_round_trip() {
    let temp = repository();
    create_branch(temp.path(), "feature", None)
        .await
        .expect("feature branch should be created");
    std::fs::write(temp.path().join("feature.txt"), "feature\n")
        .expect("feature file should be written");
    git(temp.path(), &["add", "feature.txt"]);
    git(temp.path(), &["commit", "-m", "feature commit"]);
    checkout_branch(temp.path(), "main", None)
        .await
        .expect("main should be checked out");
    merge_branch(temp.path(), "feature", None)
        .await
        .expect("feature should merge");
    delete_branch(temp.path(), "feature", None)
        .await
        .expect("merged feature branch should delete");
    std::fs::write(temp.path().join("tracked.txt"), "stashed\n")
        .expect("tracked file should be edited");

    stash(temp.path(), None).await.expect("change should stash");
    assert_eq!(
        std::fs::read_to_string(temp.path().join("tracked.txt"))
            .expect("tracked file should be readable"),
        "first\n"
    );
    pop_stash(temp.path(), None)
        .await
        .expect("stash should apply");
    assert_eq!(
        std::fs::read_to_string(temp.path().join("tracked.txt"))
            .expect("tracked file should be readable"),
        "stashed\n"
    );
    assert!(!git(temp.path(), &["branch", "--list", "feature"]).contains("feature"));
}

#[tokio::test]
async fn remote_branch_checkout_creates_a_tracking_branch() {
    let temp = repository();
    let remote = TempDir::new().expect("temporary remote should be created");
    git(remote.path(), &["init", "--bare"]);
    git(
        temp.path(),
        &[
            "remote",
            "add",
            "origin",
            remote.path().to_str().expect("remote path should be UTF-8"),
        ],
    );
    git(temp.path(), &["switch", "-c", "remote-only"]);
    std::fs::write(temp.path().join("remote.txt"), "remote\n")
        .expect("remote file should be written");
    git(temp.path(), &["add", "remote.txt"]);
    git(temp.path(), &["commit", "-m", "remote branch"]);
    git(temp.path(), &["push", "-u", "origin", "remote-only"]);
    git(temp.path(), &["switch", "main"]);
    git(temp.path(), &["branch", "-D", "remote-only"]);

    checkout_remote_branch(temp.path(), "origin/remote-only", None)
        .await
        .expect("remote branch should check out");
    let snapshot = load_repository_snapshot(temp.path())
        .await
        .expect("snapshot should load");
    assert_eq!(snapshot.branch, "remote-only");
    assert_eq!(snapshot.upstream.as_deref(), Some("origin/remote-only"));
}
