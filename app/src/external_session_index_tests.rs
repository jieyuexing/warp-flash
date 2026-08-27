use std::fs;
use std::time::{Duration, SystemTime};

use tempfile::TempDir;

use super::*;

fn write_codex_session(root: &Path, bucket: &str, id: &str, cwd: &str) -> PathBuf {
    let path = root
        .join("sessions")
        .join(bucket)
        .join(format!("rollout-{id}.jsonl"));
    fs::create_dir_all(path.parent().expect("session parent")).expect("create codex bucket");
    fs::write(
        &path,
        format!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"cwd\":\"{cwd}\"}}}}\n{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"Title for {id}\"}}]}}}}\n"
        ),
    )
    .expect("write codex session");
    path
}

fn write_grok_session(root: &Path, encoded_cwd: &str, id: &str, archived: bool) -> PathBuf {
    let path = root
        .join("sessions")
        .join(encoded_cwd)
        .join(id)
        .join("summary.json");
    fs::create_dir_all(path.parent().expect("summary parent")).expect("create grok session");
    fs::write(
        &path,
        format!(
            "{{\"session_summary\":\"Grok {id}\",\"updated_at\":\"2026-07-19T12:00:00Z\",\"archived\":{archived}}}"
        ),
    )
    .expect("write grok summary");
    path
}

fn tab(group: ExternalSessionGroup, id: &str, cwd: &str, age_seconds: u64) -> ExternalSessionTab {
    ExternalSessionTab {
        group,
        session_id: id.to_owned(),
        title: id.to_owned(),
        cwd: Some(PathBuf::from(cwd)),
        updated_at: SystemTime::UNIX_EPOCH + Duration::from_secs(age_seconds),
        archived: false,
    }
}

#[test]
fn codex_default_index_never_reads_archived_root() {
    let temp = TempDir::new().expect("temp dir");
    write_codex_session(temp.path(), "2026/07/19", "active-id", "/workspace");
    let archived_path = temp
        .path()
        .join("archived_sessions")
        .join("rollout-archived-id.jsonl");
    fs::create_dir_all(archived_path.parent().expect("archive parent"))
        .expect("create archive root");
    fs::write(
        archived_path,
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"archived-id\",\"cwd\":\"/workspace\"}}\n",
    )
    .expect("write archived session");

    let sessions = index_codex_sessions(temp.path());

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, "active-id");
    assert!(!sessions[0].archived);
}

#[test]
fn projection_keeps_distinct_codex_and_grok_groups_and_hides_grok_archives() {
    let codex = TempDir::new().expect("codex temp dir");
    let grok = TempDir::new().expect("grok temp dir");
    write_codex_session(codex.path(), "2026/07/19", "codex-id", "/workspace");
    write_grok_session(grok.path(), "%2Fworkspace", "grok-id", false);
    write_grok_session(grok.path(), "%2Fworkspace", "archived-grok-id", true);

    let projection = index_external_sessions(codex.path(), grok.path());

    assert_eq!(projection.codex.len(), 1);
    assert_eq!(projection.codex[0].group, ExternalSessionGroup::Codex);
    assert_eq!(projection.grok.len(), 1);
    assert_eq!(projection.grok[0].group, ExternalSessionGroup::Grok);
    assert_eq!(projection.grok[0].session_id, "grok-id");
}

#[test]
fn projection_applies_limit_per_group_after_preferring_workspace_matches() {
    let projection = ExternalSessionProjection {
        codex: vec![
            tab(ExternalSessionGroup::Codex, "new-other", "/other", 30),
            tab(ExternalSessionGroup::Codex, "match-1", "/workspace", 20),
            tab(ExternalSessionGroup::Codex, "match-2", "/workspace/sub", 10),
        ],
        grok: vec![
            tab(ExternalSessionGroup::Grok, "grok-3", "/workspace", 30),
            tab(ExternalSessionGroup::Grok, "grok-2", "/workspace", 20),
            tab(ExternalSessionGroup::Grok, "grok-1", "/workspace", 10),
        ],
    };

    let projected = projection.for_workspace(Some(Path::new("/workspace")), 2);

    assert_eq!(
        projected
            .codex
            .iter()
            .map(|session| session.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["match-1", "match-2"]
    );
    assert_eq!(projected.codex.len(), 2);
    assert_eq!(projected.grok.len(), 2);
}

#[test]
fn activation_prepares_resume_without_running_the_command() {
    match activation_action(ExternalSessionGroup::Codex, "codex-id") {
        WorkspaceAction::InsertInInput {
            content,
            replace_buffer,
            ensure_agent_mode,
        } => {
            assert_eq!(content, "codex resume codex-id");
            assert!(replace_buffer);
            assert!(!ensure_agent_mode);
        }
        _ => panic!("Codex activation must only prepare the resume command"),
    }

    match activation_action(ExternalSessionGroup::Grok, "grok-id") {
        WorkspaceAction::InsertInInput {
            content,
            replace_buffer,
            ensure_agent_mode,
        } => {
            assert_eq!(content, "grok --resume grok-id");
            assert!(replace_buffer);
            assert!(!ensure_agent_mode);
        }
        _ => panic!("Grok activation must only prepare the resume command"),
    }
}
