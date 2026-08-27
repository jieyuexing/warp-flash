use repo_metadata::file_tree_update::{
    DirectoryNodeMetadata, FileTreeEntryUpdate, RepoMetadataUpdate, RepoNodeMetadata,
};
use repo_metadata::{StandingQueryContent, StandingQueryResultsDelta};
use warp_util::standardized_path::StandardizedPath;

use super::{proto_snapshot_to_update, proto_to_repo_metadata_update};
use crate::proto;

fn path(path: &str) -> StandardizedPath {
    StandardizedPath::try_new(path).unwrap()
}

fn standing_delta() -> StandingQueryResultsDelta {
    StandingQueryResultsDelta {
        upserted_project_skills: vec![StandingQueryContent::file(path(
            "/repo/.agents/skills/review/SKILL.md",
        ))],
        removed_project_skills: vec![StandingQueryContent::directory(path(
            "/repo/.claude/skills",
        ))],
        upserted_project_rules: vec![StandingQueryContent::file(path("/repo/WARP.md"))],
        removed_project_rules: vec![StandingQueryContent::file(path(
            "/repo/packages/api/AGENTS.md",
        ))],
    }
}

#[test]
fn incremental_update_round_trip_preserves_standing_results_delta() {
    let update = RepoMetadataUpdate {
        repo_path: path("/repo"),
        remove_entries: Vec::new(),
        update_entries: Vec::new(),
        standing_results_delta: standing_delta(),
    };

    let proto_update = proto::RepoMetadataUpdatePush::from(&update);
    let round_trip = proto_to_repo_metadata_update(&proto_update).unwrap();

    assert_eq!(
        round_trip.standing_results_delta,
        update.standing_results_delta
    );
}

#[test]
fn incremental_update_round_trip_preserves_directory_symlink_metadata() {
    let update = RepoMetadataUpdate {
        repo_path: path("/repo"),
        remove_entries: Vec::new(),
        update_entries: vec![FileTreeEntryUpdate {
            parent_path_to_replace: path("/repo"),
            subtree_metadata: vec![RepoNodeMetadata::Directory(DirectoryNodeMetadata {
                path: path("/repo/linked"),
                ignored: false,
                loaded: false,
                is_symlink: true,
            })],
        }],
        standing_results_delta: Default::default(),
    };

    let proto_update = proto::RepoMetadataUpdatePush::from(&update);
    let round_trip = proto_to_repo_metadata_update(&proto_update).unwrap();

    assert!(matches!(
        &round_trip.update_entries[0].subtree_metadata[0],
        RepoNodeMetadata::Directory(directory) if directory.is_symlink
    ));
}

#[test]
fn snapshot_conversion_seeds_standing_results() {
    let delta = standing_delta();
    let snapshot = proto::RepoMetadataSnapshot {
        repo_path: "/repo".to_string(),
        entries: Vec::new(),
        standing_results: Some((&delta).into()),
        sync_complete: true,
    };

    let update = proto_snapshot_to_update(&snapshot).unwrap();

    assert_eq!(update.standing_results_delta, delta);
}
