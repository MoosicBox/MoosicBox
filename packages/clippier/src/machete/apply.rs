//! Checked publication with evidence revalidation and guarded rollback.
use super::{BoxError, MachetePlan, MacheteReport, discover::optional};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

fn check(plan: &MachetePlan, completed: &[(PathBuf, String, String)]) -> Result<(), BoxError> {
    for (path, before) in &plan.snapshots {
        let expected = completed
            .iter()
            .find(|(published, _, _)| published == path)
            .map_or(before.as_deref(), |(_, _, after)| Some(after.as_bytes()));
        if optional(path)?.as_deref() != expected {
            return Err(format!("stale machete plan: {} changed", path.display()).into());
        }
        if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(format!("symlink appeared: {}", path.display()).into());
        }
    }
    for (directory, expected) in &plan.directories {
        let mut current = fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        current.sort();
        if &current != expected {
            return Err(format!("stale machete inventory: {} changed", directory.display()).into());
        }
        if fs::canonicalize(directory)? != *directory {
            return Err(format!("directory became a symlink: {}", directory.display()).into());
        }
    }
    Ok(())
}

/// Revalidate an unmodified plan without publishing any files.
///
/// # Errors
/// * Incomplete or altered plans, stale evidence, symlinks, or inventory I/O failures.
pub(super) fn validate_plan(plan: &MachetePlan) -> Result<(), BoxError> {
    if !plan.complete || !plan.report.complete {
        return Err("cannot apply incomplete machete analysis".into());
    }
    let changes = plan
        .changes
        .iter()
        .map(|change| {
            (
                change.path.clone(),
                change.before.clone(),
                change.after.clone(),
            )
        })
        .collect::<Vec<_>>();
    if changes != plan.sealed_changes {
        return Err("machete plan changes were modified; re-plan".into());
    }
    check(plan, &[])
}

// Create the replacement beside its destination and rename only after rechecking.
// This is not a cross-process lock: callers must serialize concurrent apply calls.
fn publish(path: &Path, expected: &str, replacement: &str) -> Result<(), BoxError> {
    let directory = path.parent().ok_or("manifest has no parent")?;
    let temporary = directory.join(format!(".clippier-machete-{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| -> Result<(), BoxError> {
        file.write_all(replacement.as_bytes())?;
        file.set_permissions(fs::metadata(path)?.permissions())?;
        file.sync_all()?;
        if fs::read(path)? != expected.as_bytes()
            || fs::symlink_metadata(path)?.file_type().is_symlink()
        {
            return Err(format!("concurrent edit: {}", path.display()).into());
        }
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err()
        && let Err(cleanup) = fs::remove_file(&temporary)
        && cleanup.kind() != std::io::ErrorKind::NotFound
    {
        return Err(
            format!("publication failed: {result:?}; temporary cleanup failed: {cleanup}").into(),
        );
    }
    result
}

/// Explicitly publish a complete, unmodified plan after rechecking all evidence.
///
/// Preserves Cargo.lock byte-for-byte and never runs Cargo. Publication is atomic
/// per file, not across the workspace. Failure rolls back only replacements whose
/// bytes still belong to this operation; concurrent edits are never overwritten
/// during rollback. Callers must serialize apply against other workspace writers;
/// filesystem checks cannot eliminate the final check/rename race.
///
/// # Errors
/// * Incomplete/tampered plans, stale manifests/configuration/source/lockfiles.
/// * Changed directory inventories, symlinks, I/O or publication failure.
/// * Rollback failure, explicitly included alongside the original error.
pub fn apply_machete_plan(plan: &MachetePlan) -> Result<MacheteReport, BoxError> {
    validate_plan(plan)?;
    let changes = plan
        .changes
        .iter()
        .map(|c| (c.path.clone(), c.before.clone(), c.after.clone()))
        .collect::<Vec<_>>();
    let mut completed = Vec::new();
    let result = (|| -> Result<(), BoxError> {
        for (path, before, after) in &changes {
            check(plan, &completed)?;
            let _: toml_edit::DocumentMut = after.parse()?;
            publish(path, before, after)?;
            completed.push((path.clone(), before.clone(), after.clone()));
        }
        check(plan, &completed)?;
        Ok(())
    })();
    if let Err(error) = result {
        let mut diagnostics = Vec::new();
        for (path, before, after) in completed.iter().rev() {
            if let Err(rollback) = publish(path, after, before) {
                diagnostics.push(format!("{}: {rollback}", path.display()));
            }
        }
        return Err(format!(
            "machete apply failed: {error}; rollback diagnostics: {}",
            diagnostics.join("; ")
        )
        .into());
    }
    let mut report = plan.report.clone();
    report.applied = true;
    Ok(report)
}
