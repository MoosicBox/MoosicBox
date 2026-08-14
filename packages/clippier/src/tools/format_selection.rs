//! Git-aware file selection for formatter invocations.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use git2::{Delta, DiffOptions, ErrorCode, Repository, StatusOptions};

use super::{BoxError, FormatScope, FormatSelection};

/// Resolves the files selected by a formatter scope.
///
/// # Errors
///
/// * If the working directory cannot be canonicalized
/// * If repository status cannot be read
/// * If a branch base or `HEAD` cannot be resolved
/// * If the branch merge base or tree diff cannot be computed
pub fn resolve_format_selection(
    working_dir: &Path,
    scope: FormatScope,
    git_base: Option<&str>,
) -> Result<FormatSelection, BoxError> {
    if scope == FormatScope::All {
        return Ok(FormatSelection::All);
    }

    let working_dir = working_dir.canonicalize()?;
    let repo = match Repository::discover(&working_dir) {
        Ok(repo) => repo,
        Err(error) if error.code() == ErrorCode::NotFound => {
            return Ok(FormatSelection::NoRepository);
        }
        Err(error) => return Err(error.into()),
    };
    let workdir = repo
        .workdir()
        .ok_or("Git-aware formatting is not supported in bare repositories")?
        .canonicalize()?;

    let mut repository_paths = local_changed_paths(&repo)?;
    if scope == FormatScope::Branch {
        let base =
            git_base.ok_or("branch format scope requires --git-base or runner.format.git-base")?;
        repository_paths.extend(branch_changed_paths(&repo, base)?);
    }

    let files = repository_paths
        .into_iter()
        .filter_map(|relative| {
            let absolute = workdir.join(relative);
            if !absolute.is_file() {
                return None;
            }
            absolute
                .strip_prefix(&working_dir)
                .ok()
                .map(Path::to_path_buf)
        })
        .collect();

    Ok(FormatSelection::Files(files))
}

fn local_changed_paths(repo: &Repository) -> Result<BTreeSet<PathBuf>, git2::Error> {
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);

    let statuses = repo.statuses(Some(&mut options))?;
    let mut paths = BTreeSet::new();
    for entry in statuses.iter() {
        let status = entry.status();
        let deleted = status.is_index_deleted() || status.is_wt_deleted();
        if deleted && !status.is_index_renamed() && !status.is_wt_renamed() {
            continue;
        }
        let path = if status.is_index_renamed() {
            entry
                .head_to_index()
                .and_then(|delta| delta.new_file().path().map(Path::to_path_buf))
        } else if status.is_wt_renamed() {
            entry
                .index_to_workdir()
                .and_then(|delta| delta.new_file().path().map(Path::to_path_buf))
        } else {
            entry.path().map(PathBuf::from)
        };
        if let Some(path) = path {
            paths.insert(path);
        }
    }
    Ok(paths)
}

fn branch_changed_paths(repo: &Repository, base: &str) -> Result<BTreeSet<PathBuf>, git2::Error> {
    let base_commit = repo.revparse_single(base)?.peel_to_commit()?;
    let head_commit = repo.head()?.peel_to_commit()?;
    let merge_base = repo.merge_base(base_commit.id(), head_commit.id())?;
    let merge_base_tree = repo.find_commit(merge_base)?.tree()?;
    let head_tree = head_commit.tree()?;

    let mut options = DiffOptions::new();
    options.include_untracked(false);
    let mut diff =
        repo.diff_tree_to_tree(Some(&merge_base_tree), Some(&head_tree), Some(&mut options))?;
    diff.find_similar(None)?;

    let mut paths = BTreeSet::new();
    for delta in diff.deltas() {
        if delta.status() == Delta::Deleted {
            continue;
        }
        if let Some(path) = delta.new_file().path() {
            paths.insert(path.to_path_buf());
        }
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{IndexAddOption, Signature};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{nanos}"));
        std::fs::create_dir_all(&path).expect("failed to create temp dir");
        path
    }

    fn commit_all(repo: &Repository, message: &str) {
        let mut index = repo.index().expect("failed to open index");
        index
            .add_all(["*"], IndexAddOption::DEFAULT, None)
            .expect("failed to add files");
        index.write().expect("failed to write index");
        let tree_id = index.write_tree().expect("failed to write tree");
        let tree = repo.find_tree(tree_id).expect("failed to find tree");
        let signature = Signature::now("Clippier Test", "clippier@example.com")
            .expect("failed to create signature");
        let parents = repo
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok())
            .into_iter()
            .collect::<Vec<_>>();
        let parent_refs = parents.iter().collect::<Vec<_>>();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )
        .expect("failed to commit");
    }

    #[test]
    fn changed_scope_selects_staged_unstaged_and_untracked_files() {
        let dir = temp_dir("clippier-format-selection");
        let repo = Repository::init(&dir).expect("failed to initialize repository");
        std::fs::write(dir.join("staged.rs"), "fn staged() {}\n").unwrap();
        std::fs::write(dir.join("unstaged.rs"), "fn unstaged() {}\n").unwrap();
        commit_all(&repo, "initial");

        std::fs::write(dir.join("staged.rs"), "fn staged(){ }\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("staged.rs")).unwrap();
        index.write().unwrap();
        std::fs::write(dir.join("unstaged.rs"), "fn unstaged(){ }\n").unwrap();
        std::fs::write(dir.join("untracked.rs"), "fn untracked(){ }\n").unwrap();

        let selection = resolve_format_selection(&dir, FormatScope::Changed, None).unwrap();
        assert_eq!(
            selection,
            FormatSelection::Files(BTreeSet::from([
                PathBuf::from("staged.rs"),
                PathBuf::from("unstaged.rs"),
                PathBuf::from("untracked.rs"),
            ]))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn changed_scope_uses_rename_destination_and_omits_deleted_source() {
        let dir = temp_dir("clippier-format-rename");
        let repo = Repository::init(&dir).unwrap();
        std::fs::write(dir.join("old.rs"), "fn renamed() {}\n").unwrap();
        commit_all(&repo, "initial");
        std::fs::rename(dir.join("old.rs"), dir.join("new.rs")).unwrap();
        let mut index = repo.index().unwrap();
        index.remove_path(Path::new("old.rs")).unwrap();
        index.add_path(Path::new("new.rs")).unwrap();
        index.write().unwrap();

        assert_eq!(
            resolve_format_selection(&dir, FormatScope::Changed, None).unwrap(),
            FormatSelection::Files(BTreeSet::from([PathBuf::from("new.rs")]))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn branch_scope_uses_merge_base_and_includes_local_changes() {
        let dir = temp_dir("clippier-format-branch");
        let repo = Repository::init(&dir).unwrap();
        std::fs::write(dir.join("base.rs"), "fn base() {}\n").unwrap();
        commit_all(&repo, "base");
        let base = repo.head().unwrap().target().unwrap();
        std::fs::write(dir.join("committed.rs"), "fn committed() {}\n").unwrap();
        commit_all(&repo, "branch change");
        std::fs::write(dir.join("local.rs"), "fn local(){ }\n").unwrap();

        let selection =
            resolve_format_selection(&dir, FormatScope::Branch, Some(&base.to_string())).unwrap();
        assert_eq!(
            selection,
            FormatSelection::Files(BTreeSet::from([
                PathBuf::from("committed.rs"),
                PathBuf::from("local.rs"),
            ]))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unborn_repository_selects_untracked_files() {
        let dir = temp_dir("clippier-format-unborn");
        Repository::init(&dir).unwrap();
        std::fs::write(dir.join("new.rs"), "fn new(){ }\n").unwrap();

        assert_eq!(
            resolve_format_selection(&dir, FormatScope::Changed, None).unwrap(),
            FormatSelection::Files(BTreeSet::from([PathBuf::from("new.rs")]))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn all_scope_does_not_require_a_repository() {
        let dir = temp_dir("clippier-format-all");
        assert_eq!(
            resolve_format_selection(&dir, FormatScope::All, None).unwrap(),
            FormatSelection::All
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn changed_scope_filters_to_working_directory_and_omits_deletions() {
        let dir = temp_dir("clippier-format-subtree");
        let repo = Repository::init(&dir).unwrap();
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("deleted.rs"), "fn deleted() {}\n").unwrap();
        std::fs::write(dir.join("nested/kept.rs"), "fn kept() {}\n").unwrap();
        commit_all(&repo, "initial");
        std::fs::remove_file(dir.join("deleted.rs")).unwrap();
        std::fs::write(dir.join("nested/kept.rs"), "fn kept(){ }\n").unwrap();
        std::fs::write(dir.join("outside.rs"), "fn outside(){ }\n").unwrap();

        let selection =
            resolve_format_selection(&dir.join("nested"), FormatScope::Changed, None).unwrap();
        assert_eq!(
            selection,
            FormatSelection::Files(BTreeSet::from([PathBuf::from("kept.rs")]))
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn branch_scope_reports_invalid_base_and_missing_head() {
        let dir = temp_dir("clippier-format-invalid-base");
        let repo = Repository::init(&dir).unwrap();
        std::fs::write(dir.join("file.rs"), "fn file() {}\n").unwrap();
        commit_all(&repo, "initial");

        let error = resolve_format_selection(&dir, FormatScope::Branch, Some("does-not-exist"))
            .unwrap_err();
        assert!(!error.to_string().is_empty());
        std::fs::remove_dir_all(dir).unwrap();

        let unborn = temp_dir("clippier-format-missing-head");
        Repository::init(&unborn).unwrap();
        let error =
            resolve_format_selection(&unborn, FormatScope::Branch, Some("HEAD")).unwrap_err();
        assert!(!error.to_string().is_empty());
        std::fs::remove_dir_all(unborn).unwrap();
    }

    #[test]
    fn non_repository_is_distinct_from_an_empty_repository_selection() {
        let dir = temp_dir("clippier-format-no-repository");
        assert_eq!(
            resolve_format_selection(&dir, FormatScope::Changed, None).unwrap(),
            FormatSelection::NoRepository
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
