//! Git operations for workspace change detection.
//!
//! This module provides async wrappers around git2 operations using `spawn_blocking`
//! since git2 is a synchronous library.

use std::path::Path;

#[cfg(feature = "git-diff")]
use git2::Repository;

#[cfg(feature = "git-diff")]
use switchy_async::task::spawn_blocking;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Extracts file diff lines from git between two commits for a specific file.
///
/// # Arguments
///
/// * `workspace_root` - Path to the git repository root
/// * `base_commit` - Base commit reference (e.g., `origin/main`, commit SHA)
/// * `head_commit` - Head commit reference (e.g., `HEAD`)
/// * `file_path` - Path to the file to diff (relative to repo root)
///
/// # Returns
///
/// A list of (operation, line content) tuples where operation is:
/// - `+` for additions
/// - `-` for deletions
/// - ` ` for context lines
///
/// # Errors
///
/// Returns an error if the repository cannot be opened or the commits cannot be found.
#[cfg(feature = "git-diff")]
pub async fn extract_file_diff_from_git(
    workspace_root: &Path,
    base_commit: &str,
    head_commit: &str,
    file_path: &str,
) -> Result<Vec<(char, String)>, BoxError> {
    let root = workspace_root.to_path_buf();
    let base = base_commit.to_string();
    let head = head_commit.to_string();
    let path = file_path.to_string();

    spawn_blocking(move || extract_file_diff_from_git_sync(&root, &base, &head, &path)).await?
}

/// Synchronous implementation of file diff extraction.
#[cfg(feature = "git-diff")]
fn extract_file_diff_from_git_sync(
    workspace_root: &Path,
    base_commit: &str,
    head_commit: &str,
    file_path: &str,
) -> Result<Vec<(char, String)>, BoxError> {
    let repo = Repository::open(workspace_root)?;

    let base_oid = repo.revparse_single(base_commit)?.id();
    let head_oid = repo.revparse_single(head_commit)?.id();

    let base_commit_obj = repo.find_commit(base_oid)?;
    let head_commit_obj = repo.find_commit(head_oid)?;

    let base_tree = base_commit_obj.tree()?;
    let head_tree = head_commit_obj.tree()?;

    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.pathspec(file_path);

    let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut diff_opts))?;

    let mut changes = Vec::new();

    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        let content = std::str::from_utf8(line.content()).unwrap_or("");
        changes.push((line.origin(), content.to_string()));
        true
    })?;

    log::debug!(
        "Found {} lines in {file_path} diff between {base_commit} and {head_commit}",
        changes.len()
    );

    Ok(changes)
}

/// Gets list of changed files between two git commits.
///
/// # Arguments
///
/// * `workspace_root` - Path to the git repository root
/// * `base_commit` - Base commit reference
/// * `head_commit` - Head commit reference
///
/// # Returns
///
/// A sorted, deduplicated list of changed file paths (relative to repo root).
///
/// # Errors
///
/// Returns an error if the repository cannot be opened or commits cannot be found.
#[cfg(feature = "git-diff")]
pub async fn get_changed_files_from_git(
    workspace_root: &Path,
    base_commit: &str,
    head_commit: &str,
) -> Result<Vec<String>, BoxError> {
    let root = workspace_root.to_path_buf();
    let base = base_commit.to_string();
    let head = head_commit.to_string();

    spawn_blocking(move || get_changed_files_from_git_sync(&root, &base, &head)).await?
}

/// Synchronous implementation of changed files extraction.
#[cfg(feature = "git-diff")]
fn get_changed_files_from_git_sync(
    workspace_root: &Path,
    base_commit: &str,
    head_commit: &str,
) -> Result<Vec<String>, BoxError> {
    let repo = Repository::open(workspace_root)?;

    let base_oid = repo.revparse_single(base_commit)?.id();
    let head_oid = repo.revparse_single(head_commit)?.id();

    let base_commit_obj = repo.find_commit(base_oid)?;
    let head_commit_obj = repo.find_commit(head_oid)?;

    let base_tree = base_commit_obj.tree()?;
    let head_tree = head_commit_obj.tree()?;

    let diff = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)?;

    let mut changed_files = Vec::new();

    diff.foreach(
        &mut |delta, _progress| {
            if let Some(path_str) = delta.new_file().path().and_then(|p| p.to_str()) {
                changed_files.push(path_str.to_string());
            } else if let Some(path_str) = delta.old_file().path().and_then(|p| p.to_str()) {
                changed_files.push(path_str.to_string());
            }
            true
        },
        None,
        None,
        None,
    )?;

    changed_files.sort();
    changed_files.dedup();

    log::debug!(
        "Found {} changed files from git between {base_commit} and {head_commit}",
        changed_files.len()
    );

    Ok(changed_files)
}

/// Checks if a file exists in a specific git commit.
///
/// # Arguments
///
/// * `workspace_root` - Path to the git repository root
/// * `commit_ref` - Commit reference (e.g., `HEAD`, `origin/main`)
/// * `file_path` - Path to check (relative to repo root)
///
/// # Returns
///
/// `true` if the file exists in the specified commit.
///
/// # Errors
///
/// Returns an error if:
/// * Opening the git repository fails
/// * Resolving the commit reference fails
/// * Spawning the blocking task fails
#[cfg(feature = "git-diff")]
pub async fn file_exists_in_commit(
    workspace_root: &Path,
    commit_ref: &str,
    file_path: &str,
) -> Result<bool, BoxError> {
    let root = workspace_root.to_path_buf();
    let commit = commit_ref.to_string();
    let path = file_path.to_string();

    spawn_blocking(move || file_exists_in_commit_sync(&root, &commit, &path)).await?
}

#[cfg(feature = "git-diff")]
fn file_exists_in_commit_sync(
    workspace_root: &Path,
    commit_ref: &str,
    file_path: &str,
) -> Result<bool, BoxError> {
    let repo = Repository::open(workspace_root)?;
    let commit_oid = repo.revparse_single(commit_ref)?.id();
    let commit = repo.find_commit(commit_oid)?;
    let tree = commit.tree()?;

    Ok(tree.get_path(Path::new(file_path)).is_ok())
}

/// Gets the content of a file at a specific git commit.
///
/// # Arguments
///
/// * `workspace_root` - Path to the git repository root
/// * `commit_ref` - Commit reference
/// * `file_path` - Path to the file (relative to repo root)
///
/// # Returns
///
/// The file content as a string, or `None` if the file doesn't exist.
///
/// # Errors
///
/// Returns an error if:
/// * Opening the git repository fails
/// * Resolving the commit reference fails
/// * Reading the blob content fails
/// * The file content is not valid UTF-8
/// * Spawning the blocking task fails
#[cfg(feature = "git-diff")]
pub async fn get_file_at_commit(
    workspace_root: &Path,
    commit_ref: &str,
    file_path: &str,
) -> Result<Option<String>, BoxError> {
    let root = workspace_root.to_path_buf();
    let commit = commit_ref.to_string();
    let path = file_path.to_string();

    spawn_blocking(move || get_file_at_commit_sync(&root, &commit, &path)).await?
}

#[cfg(feature = "git-diff")]
fn get_file_at_commit_sync(
    workspace_root: &Path,
    commit_ref: &str,
    file_path: &str,
) -> Result<Option<String>, BoxError> {
    let repo = Repository::open(workspace_root)?;
    let commit_oid = repo.revparse_single(commit_ref)?.id();
    let commit = repo.find_commit(commit_oid)?;
    let tree = commit.tree()?;

    let Ok(entry) = tree.get_path(Path::new(file_path)) else {
        return Ok(None);
    };

    let blob = repo.find_blob(entry.id())?;
    let content = std::str::from_utf8(blob.content())?.to_string();

    Ok(Some(content))
}

// Stub implementations when git-diff feature is disabled
#[cfg(not(feature = "git-diff"))]
pub async fn extract_file_diff_from_git(
    _workspace_root: &Path,
    _base_commit: &str,
    _head_commit: &str,
    _file_path: &str,
) -> Result<Vec<(char, String)>, BoxError> {
    Err("git-diff feature is not enabled".into())
}

#[cfg(not(feature = "git-diff"))]
pub async fn get_changed_files_from_git(
    _workspace_root: &Path,
    _base_commit: &str,
    _head_commit: &str,
) -> Result<Vec<String>, BoxError> {
    Err("git-diff feature is not enabled".into())
}

#[cfg(not(feature = "git-diff"))]
pub async fn file_exists_in_commit(
    _workspace_root: &Path,
    _commit_ref: &str,
    _file_path: &str,
) -> Result<bool, BoxError> {
    Err("git-diff feature is not enabled".into())
}

#[cfg(not(feature = "git-diff"))]
pub async fn get_file_at_commit(
    _workspace_root: &Path,
    _commit_ref: &str,
    _file_path: &str,
) -> Result<Option<String>, BoxError> {
    Err("git-diff feature is not enabled".into())
}
