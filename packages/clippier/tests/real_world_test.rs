#[cfg(feature = "git-diff")]
use clippier::git_diff::{
    extract_changed_dependencies_from_git, find_transitively_affected_external_deps,
    parse_cargo_lock_changes,
};
use clippier::test_utils::test_resources::load_cargo_lock_for_git_diff;
use std::path::Path;

/// Seeds a single file from the real filesystem into the simulated filesystem.
///
/// This reads the file content from the real filesystem (using `std::fs`) and
/// writes it to the simulated filesystem (using `switchy_fs`). The parent
/// directories are created if needed.
#[cfg(feature = "git-diff")]
fn seed_file_from_real_fs(path: &Path) {
    if !switchy_fs::is_simulator_enabled() {
        return;
    }

    // Read from real filesystem
    let content = std::fs::read(path).expect("Failed to read file from real filesystem");

    // Create parent directory in simulated filesystem if needed
    if let Some(parent) = path.parent() {
        switchy_fs::sync::create_dir_all(parent).expect("Failed to create parent directory");
    }

    // Write to simulated filesystem
    switchy_fs::sync::write(path, content).expect("Failed to write file to simulated filesystem");
}

#[cfg(feature = "git-diff")]
#[test_log::test(switchy_async::test)]
async fn test_real_world_tokio_update() {
    // Simulate a real git diff where tokio was updated from 1.35.0 to 1.36.0
    let cargo_lock_changes = vec![
        (' ', "[[package]]".to_string()),
        (' ', "name = \"tokio\"".to_string()),
        ('-', "version = \"1.35.0\"".to_string()),
        ('+', "version = \"1.36.0\"".to_string()),
        (
            ' ',
            "source = \"registry+https://github.com/rust-lang/crates.io-index\"".to_string(),
        ),
    ];

    let changed_deps = parse_cargo_lock_changes(&cargo_lock_changes);
    insta::assert_debug_snapshot!(changed_deps, @r###"
    [
        "tokio",
    ]
    "###);

    // Load the tokio update Cargo.lock from test resources
    let cargo_lock = load_cargo_lock_for_git_diff("real-world", "tokio-update");

    // Find all packages affected by the tokio update
    let all_affected = find_transitively_affected_external_deps(&cargo_lock, &changed_deps);

    // Should include tokio, hyper, reqwest, and moosicbox_server
    // but NOT moosicbox_models (only depends on serde)
    insta::assert_debug_snapshot!(all_affected, @r###"
    [
        "hyper",
        "moosicbox_server",
        "reqwest",
        "tokio",
    ]
    "###);
}

#[cfg(feature = "git-diff")]
#[test_log::test(switchy_async::test)]
async fn test_real_world_cargo_lock_changes() {
    // This test uses the actual git commits mentioned in the issue
    let workspace_root = Path::new(".").canonicalize().unwrap(); // Absolute path to workspace root
    let mut workspace_root = workspace_root.clone();

    while !workspace_root.join(".git").exists() {
        workspace_root = workspace_root.parent().unwrap().to_path_buf();
    }

    // Seed the simulated filesystem with the real Cargo.lock and Cargo.toml files
    // so that switchy_fs can find them when the simulator is enabled.
    // We do this manually because seed_from_real_fs_same_path expects directories.
    seed_file_from_real_fs(&workspace_root.join("Cargo.lock"));
    seed_file_from_real_fs(&workspace_root.join("Cargo.toml"));

    let base_commit = "c721488ba3aa21df6d7c8f9874c3189ae3d6191d";
    let head_commit = "3c5d315c42b0b579c27d41cce9c2c6280a6e0e34";

    match extract_changed_dependencies_from_git(
        &workspace_root,
        base_commit,
        head_commit,
        &["Cargo.lock".to_string()],
    ) {
        Ok(changed_deps) => {
            println!("Changed external dependencies: {changed_deps:?}");

            // Based on the git diff analysis, only these packages should be detected as new:
            // console, encode_unicode, insta, similar, similar-asserts
            let expected_new_packages = [
                "console".to_string(),
                "encode_unicode".to_string(),
                "insta".to_string(),
                "similar".to_string(),
                "similar-asserts".to_string(),
            ];

            // All expected packages should be in the result
            for expected in &expected_new_packages {
                assert!(
                    changed_deps.contains(expected),
                    "Expected new package {expected} should be in changed dependencies"
                );
            }

            // Only the expected packages should be detected as directly changed
            assert_eq!(
                changed_deps.len(),
                expected_new_packages.len(),
                "Should only detect the 5 new packages as directly changed, got: {changed_deps:?}"
            );
        }
        Err(e) => {
            panic!("Failed to extract changed dependencies: {e}");
        }
    }
}

#[cfg(feature = "git-diff")]
#[test_log::test(switchy_async::test)]
async fn test_debug_cargo_lock_parsing() {
    use clippier::git_diff::extract_changed_dependencies_from_git;
    use std::path::Path;

    // Test the complete flow that filters by Cargo.lock
    let mut workspace_root = Path::new(".").canonicalize().unwrap(); // Absolute path to workspace root

    while !workspace_root.join(".git").exists() {
        workspace_root = workspace_root.parent().unwrap().to_path_buf();
    }

    // Seed the simulated filesystem with the real Cargo.lock and Cargo.toml files
    // so that switchy_fs can find them when the simulator is enabled.
    // We do this manually because seed_from_real_fs_same_path expects directories.
    seed_file_from_real_fs(&workspace_root.join("Cargo.lock"));
    seed_file_from_real_fs(&workspace_root.join("Cargo.toml"));

    let base_commit = "c721488ba3aa21df6d7c8f9874c3189ae3d6191d";
    let head_commit = "3c5d315c42b0b579c27d41cce9c2c6280a6e0e34";

    match extract_changed_dependencies_from_git(
        &workspace_root,
        base_commit,
        head_commit,
        &["Cargo.lock".to_string()],
    ) {
        Ok(affected_deps) => {
            println!(
                "Total affected external dependencies: {}",
                affected_deps.len()
            );
            println!("First 20 affected dependencies:");
            for (i, dep) in affected_deps.iter().take(20).enumerate() {
                println!("  {}: {}", i + 1, dep);
            }

            // Check if the expected packages are there
            let expected = [
                "console",
                "encode_unicode",
                "insta",
                "similar",
                "similar-asserts",
            ];
            for exp in &expected {
                if affected_deps.contains(&exp.to_string()) {
                    println!("✓ Expected package found in affected deps: {exp}");
                } else {
                    println!("✗ Expected package NOT found in affected deps: {exp}");
                }
            }
        }
        Err(e) => {
            panic!("Failed to extract changed dependencies: {e}");
        }
    }
}

#[cfg(feature = "git-diff")]
#[test_log::test(switchy_async::test)]
async fn test_debug_raw_diff_lines() {
    use git2::Repository;
    use std::path::Path;

    // Test parsing the actual git diff to see what changes are detected
    let workspace_root = Path::new(".").canonicalize().unwrap(); // Absolute path to workspace root
    let mut workspace_root = workspace_root.clone();

    while !workspace_root.join(".git").exists() {
        workspace_root = workspace_root.parent().unwrap().to_path_buf();
    }

    let base_commit = "c721488ba3aa21df6d7c8f9874c3189ae3d6191d";
    let head_commit = "3c5d315c42b0b579c27d41cce9c2c6280a6e0e34";

    if let Ok(repo) = Repository::open(workspace_root)
        && let (Ok(base_oid), Ok(head_oid)) = (
            repo.revparse_single(base_commit).map(|o| o.id()),
            repo.revparse_single(head_commit).map(|o| o.id()),
        )
        && let (Ok(base_commit), Ok(head_commit)) =
            (repo.find_commit(base_oid), repo.find_commit(head_oid))
        && let (Ok(base_tree), Ok(head_tree)) = (base_commit.tree(), head_commit.tree())
        && let Ok(diff) = repo.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)
    {
        let mut cargo_lock_changes = Vec::new();

        // Extract the changes
        let _ = diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            let content = std::str::from_utf8(line.content()).unwrap_or("");
            cargo_lock_changes.push((line.origin(), content.to_string()));
            true
        });

        // Look for lines that contain 'serde' to understand why it's being detected as changed
        println!("Lines containing 'serde':");
        for (i, (op, line)) in cargo_lock_changes.iter().enumerate() {
            if line.to_lowercase().contains("serde") {
                println!("  {}: {} '{}'", i, op, line.trim());
            }
        }

        // Look for patterns around package declarations
        println!("\nPattern analysis around package declarations:");
        for (i, (_op, line)) in cargo_lock_changes.iter().enumerate() {
            if line.trim().starts_with("name = \"") {
                // Show context around name declarations
                let start = i.saturating_sub(2);
                let end = std::cmp::min(i + 5, cargo_lock_changes.len());
                println!("Context around line {i} (name declaration):");
                for (j, (op, line)) in cargo_lock_changes[start..end].iter().enumerate() {
                    let marker = if j == i { ">>>" } else { "   " };
                    println!("  {} {}: {} '{}'", marker, j, op, line.trim());
                }
                println!();
            }
        }

        return;
    }

    panic!("Could not access git repository or commits");
}
