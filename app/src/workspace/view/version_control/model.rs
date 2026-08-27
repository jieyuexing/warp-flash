use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use warp_util::git::{run_git_command, run_git_command_strict_with_env};

const FIELD_SEPARATOR: char = '\u{1f}';
const RECORD_SEPARATOR: char = '\u{1e}';
pub const MAX_CHANGES: usize = 1_000;
pub const MAX_COMMITS: usize = 200;
pub const MAX_BRANCHES: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeGroup {
    Conflicts,
    Staged,
    Unstaged,
    Untracked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffTarget {
    Index,
    Worktree,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitChange {
    pub path: PathBuf,
    pub original_path: Option<PathBuf>,
    pub index_status: char,
    pub worktree_status: char,
}

impl GitChange {
    pub fn is_conflicted(&self) -> bool {
        matches!(
            (self.index_status, self.worktree_status),
            ('D', 'D')
                | ('A', 'U')
                | ('U', 'D')
                | ('U', 'A')
                | ('D', 'U')
                | ('A', 'A')
                | ('U', 'U')
        )
    }

    pub fn is_untracked(&self) -> bool {
        self.index_status == '?' && self.worktree_status == '?'
    }

    pub fn is_staged(&self) -> bool {
        !self.is_conflicted() && !self.is_untracked() && self.index_status != ' '
    }

    pub fn is_unstaged(&self) -> bool {
        !self.is_conflicted() && !self.is_untracked() && self.worktree_status != ' '
    }

    pub fn groups(&self) -> impl Iterator<Item = ChangeGroup> {
        let mut groups = Vec::with_capacity(2);
        if self.is_conflicted() {
            groups.push(ChangeGroup::Conflicts);
        } else if self.is_untracked() {
            groups.push(ChangeGroup::Untracked);
        } else {
            if self.is_staged() {
                groups.push(ChangeGroup::Staged);
            }
            if self.is_unstaged() {
                groups.push(ChangeGroup::Unstaged);
            }
        }
        groups.into_iter()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitCommit {
    pub hash: String,
    pub short_hash: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: String,
    pub decorations: Vec<String>,
    pub subject: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BranchKind {
    Local,
    Remote,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitBranch {
    pub kind: BranchKind,
    pub name: String,
    pub short_hash: String,
    pub upstream: Option<String>,
    pub tracking: Option<String>,
    pub subject: String,
    pub committed_at: String,
    pub is_current: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepositorySnapshot {
    pub root: PathBuf,
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub changes: Vec<GitChange>,
    pub changes_truncated: bool,
    pub commits: Vec<GitCommit>,
    pub branches: Vec<GitBranch>,
}

struct ParsedStatus {
    branch: String,
    upstream: Option<String>,
    ahead: usize,
    behind: usize,
    changes: Vec<GitChange>,
}

impl RepositorySnapshot {
    pub fn changes_in_group(&self, group: ChangeGroup) -> impl Iterator<Item = &GitChange> {
        self.changes
            .iter()
            .filter(move |change| change.groups().any(|candidate| candidate == group))
    }
}

pub async fn discover_repository_roots(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut seen = HashSet::new();
    for path in paths {
        let candidate = if path.is_file() {
            path.parent().unwrap_or(path)
        } else {
            path.as_path()
        };
        let Ok(output) = run_git_command(candidate, &["rev-parse", "--show-toplevel"]).await else {
            continue;
        };
        let root = PathBuf::from(output.trim());
        if seen.insert(root.clone()) {
            roots.push(root);
        }
    }
    roots
}

pub async fn load_repository_snapshot(root: &Path) -> Result<RepositorySnapshot> {
    let status = run_git_command(
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--branch",
            "--untracked-files=all",
        ],
    )
    .await
    .context("failed to load Git status")?;
    let ParsedStatus {
        branch,
        upstream,
        ahead,
        behind,
        mut changes,
    } = parse_status(&status)?;
    let changes_truncated = changes.len() > MAX_CHANGES;
    changes.truncate(MAX_CHANGES);

    let max_commits = format!("-{MAX_COMMITS}");
    let log_format = format!(
        "--format=%H{FIELD_SEPARATOR}%h{FIELD_SEPARATOR}%P{FIELD_SEPARATOR}%an{FIELD_SEPARATOR}%ae{FIELD_SEPARATOR}%aI{FIELD_SEPARATOR}%D{FIELD_SEPARATOR}%s{RECORD_SEPARATOR}"
    );
    let commits = run_git_command(
        root,
        &["log", &max_commits, "--date=iso-strict", &log_format],
    )
    .await
    .map(|output| parse_log(&output))
    .unwrap_or_default();

    let branch_format = format!(
        "--format=%(refname){FIELD_SEPARATOR}%(refname:short){FIELD_SEPARATOR}%(objectname:short){FIELD_SEPARATOR}%(upstream:short){FIELD_SEPARATOR}%(upstream:trackshort){FIELD_SEPARATOR}%(subject){FIELD_SEPARATOR}%(committerdate:iso-strict){FIELD_SEPARATOR}%(HEAD){RECORD_SEPARATOR}"
    );
    let max_branches = format!("--count={MAX_BRANCHES}");
    let branches = run_git_command(
        root,
        &[
            "for-each-ref",
            &max_branches,
            "--sort=-committerdate",
            &branch_format,
            "refs/heads",
            "refs/remotes",
        ],
    )
    .await
    .map(|output| parse_branches(&output))
    .unwrap_or_default();

    Ok(RepositorySnapshot {
        root: root.to_path_buf(),
        branch,
        upstream,
        ahead,
        behind,
        changes,
        changes_truncated,
        commits,
        branches,
    })
}

pub async fn load_diff(root: &Path, change: &GitChange, target: DiffTarget) -> Result<String> {
    let path = change.path.to_string_lossy();
    if change.is_untracked() {
        return run_git_command(root, &["diff", "--no-index", "--", "/dev/null", &path]).await;
    }
    match target {
        DiffTarget::Index => {
            run_git_command(root, &["diff", "--cached", "--find-renames", "--", &path]).await
        }
        DiffTarget::Worktree => {
            run_git_command(root, &["diff", "--find-renames", "--", &path]).await
        }
    }
}

pub async fn load_commit_diff(root: &Path, hash: &str) -> Result<String> {
    run_git_command(
        root,
        &["show", "--format=fuller", "--stat", "--patch", hash],
    )
    .await
}

pub async fn stage_paths(root: &Path, paths: &[PathBuf], path_env: Option<&str>) -> Result<String> {
    run_git_with_paths(root, &["add", "--"], paths, path_env).await
}

pub async fn unstage_paths(
    root: &Path,
    paths: &[PathBuf],
    path_env: Option<&str>,
) -> Result<String> {
    if run_git_command_strict_with_env(root, &["rev-parse", "--verify", "HEAD"], path_env)
        .await
        .is_ok()
    {
        run_git_with_paths(root, &["reset", "--quiet", "--"], paths, path_env).await
    } else {
        run_git_with_paths(root, &["rm", "--cached", "--quiet", "--"], paths, path_env).await
    }
}

pub async fn fetch(root: &Path, path_env: Option<&str>) -> Result<String> {
    run_git_command_strict_with_env(root, &["fetch", "--prune"], path_env).await
}

pub async fn pull(root: &Path, path_env: Option<&str>) -> Result<String> {
    run_git_command_strict_with_env(root, &["pull", "--ff-only"], path_env).await
}

pub async fn checkout_branch(root: &Path, branch: &str, path_env: Option<&str>) -> Result<String> {
    run_git_command_strict_with_env(root, &["switch", branch], path_env).await
}

pub async fn checkout_remote_branch(
    root: &Path,
    branch: &str,
    path_env: Option<&str>,
) -> Result<String> {
    run_git_command_strict_with_env(root, &["switch", "--track", branch], path_env).await
}

pub async fn create_branch(root: &Path, branch: &str, path_env: Option<&str>) -> Result<String> {
    run_git_command_strict_with_env(root, &["switch", "-c", branch], path_env).await
}

pub async fn merge_branch(root: &Path, branch: &str, path_env: Option<&str>) -> Result<String> {
    run_git_command_strict_with_env(root, &["merge", "--no-edit", branch], path_env).await
}

pub async fn delete_branch(root: &Path, branch: &str, path_env: Option<&str>) -> Result<String> {
    run_git_command_strict_with_env(root, &["branch", "-d", branch], path_env).await
}

pub async fn discard_paths(
    root: &Path,
    paths: &[PathBuf],
    path_env: Option<&str>,
) -> Result<String> {
    run_git_with_paths(root, &["restore", "--worktree", "--"], paths, path_env).await
}

pub async fn stash(root: &Path, path_env: Option<&str>) -> Result<String> {
    run_git_command_strict_with_env(
        root,
        &["stash", "push", "-u", "-m", "Warp Version Control"],
        path_env,
    )
    .await
}

pub async fn pop_stash(root: &Path, path_env: Option<&str>) -> Result<String> {
    run_git_command_strict_with_env(root, &["stash", "pop"], path_env).await
}

async fn run_git_with_paths(
    root: &Path,
    prefix: &[&str],
    paths: &[PathBuf],
    path_env: Option<&str>,
) -> Result<String> {
    if paths.is_empty() {
        return Err(anyhow!("no paths selected"));
    }
    let owned_paths = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let mut args = prefix.to_vec();
    args.extend(owned_paths.iter().map(String::as_str));
    run_git_command_strict_with_env(root, &args, path_env).await
}

fn parse_status(output: &str) -> Result<ParsedStatus> {
    let mut records = output.split('\0').filter(|record| !record.is_empty());
    let branch_record = records.next().unwrap_or_default();
    let (branch, upstream, ahead, behind) = parse_branch_header(branch_record);
    let mut changes = Vec::new();

    while let Some(record) = records.next() {
        let mut chars = record.chars();
        let index_status = chars
            .next()
            .ok_or_else(|| anyhow!("missing index status"))?;
        let worktree_status = chars
            .next()
            .ok_or_else(|| anyhow!("missing worktree status"))?;
        if chars.next() != Some(' ') {
            return Err(anyhow!("malformed Git status record"));
        }
        let path = PathBuf::from(chars.as_str());
        let original_path = (matches!(index_status, 'R' | 'C')
            || matches!(worktree_status, 'R' | 'C'))
        .then(|| records.next().map(PathBuf::from))
        .flatten();
        changes.push(GitChange {
            path,
            original_path,
            index_status,
            worktree_status,
        });
    }

    Ok(ParsedStatus {
        branch,
        upstream,
        ahead,
        behind,
        changes,
    })
}

fn parse_branch_header(record: &str) -> (String, Option<String>, usize, usize) {
    let header = record.strip_prefix("## ").unwrap_or(record);
    if let Some(branch) = header.strip_prefix("No commits yet on ") {
        return (branch.to_string(), None, 0, 0);
    }
    if let Some(detached) = header.strip_prefix("HEAD (no branch)") {
        return (format!("HEAD{detached}"), None, 0, 0);
    }
    let (branch, tracking) = header.split_once("...").unwrap_or((header, ""));
    let (upstream, counts) = tracking
        .split_once(' ')
        .map_or((tracking, ""), |(upstream, counts)| (upstream, counts));
    let upstream = (!upstream.is_empty()).then(|| upstream.to_string());
    let counts = counts.trim().trim_start_matches('[').trim_end_matches(']');
    let mut ahead = 0;
    let mut behind = 0;
    for count in counts.split(", ") {
        if let Some(value) = count.strip_prefix("ahead ") {
            ahead = value.parse().unwrap_or_default();
        } else if let Some(value) = count.strip_prefix("behind ") {
            behind = value.parse().unwrap_or_default();
        }
    }
    (branch.to_string(), upstream, ahead, behind)
}

fn parse_log(output: &str) -> Vec<GitCommit> {
    output
        .split(RECORD_SEPARATOR)
        .filter_map(|record| {
            let fields = record
                .trim_matches('\n')
                .split(FIELD_SEPARATOR)
                .collect::<Vec<_>>();
            if fields.len() != 8 || fields[0].is_empty() {
                return None;
            }
            Some(GitCommit {
                hash: fields[0].to_string(),
                short_hash: fields[1].to_string(),
                parents: fields[2].split_whitespace().map(str::to_string).collect(),
                author_name: fields[3].to_string(),
                author_email: fields[4].to_string(),
                authored_at: fields[5].to_string(),
                decorations: fields[6]
                    .split(',')
                    .map(str::trim)
                    .filter(|decoration| !decoration.is_empty())
                    .map(str::to_string)
                    .collect(),
                subject: fields[7].to_string(),
            })
        })
        .collect()
}

fn parse_branches(output: &str) -> Vec<GitBranch> {
    output
        .split(RECORD_SEPARATOR)
        .filter_map(|record| {
            let fields = record
                .trim_matches('\n')
                .split(FIELD_SEPARATOR)
                .collect::<Vec<_>>();
            if fields.len() != 8 || fields[1].is_empty() || fields[1].ends_with("/HEAD") {
                return None;
            }
            let kind = if fields[0].starts_with("refs/heads/") {
                BranchKind::Local
            } else if fields[0].starts_with("refs/remotes/") {
                BranchKind::Remote
            } else {
                return None;
            };
            Some(GitBranch {
                kind,
                name: fields[1].to_string(),
                short_hash: fields[2].to_string(),
                upstream: (!fields[3].is_empty()).then(|| fields[3].to_string()),
                tracking: (!fields[4].is_empty()).then(|| fields[4].to_string()),
                subject: fields[5].to_string(),
                committed_at: fields[6].to_string(),
                is_current: fields[7].trim() == "*",
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
