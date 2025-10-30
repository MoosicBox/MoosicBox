#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::io::Write;
use switchy_fs::exists;
use switchy_fs::sync::{
    OpenOptions, create_dir_all, read_dir_sorted, remove_dir_all, walk_dir_sorted,
};
use switchy_fs::tempdir;

#[allow(clippy::too_many_lines)]
fn main() -> std::io::Result<()> {
    println!("Demo: Directory Operations with switchy_fs\n");

    // Create a temporary directory for our examples
    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path();
    println!("Using temporary directory: {}\n", temp_path.display());

    // Example 1: Create nested directory structures
    println!("1. Creating nested directory structures:");
    let nested = temp_path
        .join("project")
        .join("src")
        .join("modules")
        .join("utils");
    create_dir_all(&nested)?;
    println!("   Created: {}", nested.display());

    let docs = temp_path.join("project").join("docs");
    create_dir_all(&docs)?;
    println!("   Created: {}", docs.display());

    let tests = temp_path.join("project").join("tests");
    create_dir_all(&tests)?;
    println!("   Created: {}", tests.display());

    // Example 2: Check if paths exist
    println!("\n2. Checking if paths exist:");
    let project_path = temp_path.join("project");
    println!("   'project' exists: {}", exists(&project_path));
    println!(
        "   'project/src' exists: {}",
        exists(project_path.join("src"))
    );
    println!(
        "   'project/nonexistent' exists: {}",
        exists(project_path.join("nonexistent"))
    );

    // Example 3: Create some files in the directories
    println!("\n3. Creating files in directories:");
    let files = vec![
        ("project/src/main.rs", "fn main() {}"),
        ("project/src/lib.rs", "pub mod utils;"),
        ("project/src/modules/utils/mod.rs", "pub fn helper() {}"),
        ("project/docs/README.md", "# Project Documentation"),
        ("project/tests/test1.rs", "#[test] fn test1() {}"),
        ("project/tests/test2.rs", "#[test] fn test2() {}"),
    ];

    for (path, content) in &files {
        let file_path = temp_path.join(path);
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&file_path)?;
        file.write_all(content.as_bytes())?;
        println!("   Created: {path}");
    }

    // Example 4: Read directory contents (sorted)
    println!("\n4. Reading directory contents (sorted by filename):");
    let src_path = temp_path.join("project").join("src");
    let entries = read_dir_sorted(&src_path)?;

    println!("   Contents of 'project/src':");
    for entry in entries {
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy();
        let entry_type = if path.is_dir() { "dir" } else { "file" };
        println!("     - {name} ({entry_type})");
    }

    // Example 5: Read another directory
    println!("\n5. Reading 'project/tests' directory:");
    let tests_path = temp_path.join("project").join("tests");
    let entries = read_dir_sorted(&tests_path)?;

    for entry in entries {
        let name = entry.file_name();
        println!("     - {}", name.to_string_lossy());
    }

    // Example 6: Recursively walk directory tree (sorted)
    println!("\n6. Walking entire directory tree (sorted by path):");
    let project_path = temp_path.join("project");
    let entries = walk_dir_sorted(&project_path)?;

    println!("   All entries under 'project':");
    for entry in entries {
        let path = entry.path();
        // Get relative path from project root for cleaner output
        let relative = path.strip_prefix(&project_path).unwrap();
        let entry_type = if path.is_dir() { "DIR " } else { "FILE" };
        println!("     [{entry_type}] {}", relative.display());
    }

    // Example 7: Create and remove directories
    println!("\n7. Demonstrating directory removal:");
    let temp_structure = temp_path.join("temp_structure");
    create_dir_all(temp_structure.join("level1").join("level2").join("level3"))?;
    println!("   Created temp_structure with nested levels");

    // Create some files in it
    for i in 1..=3 {
        let file_path = temp_structure.join(format!("file{i}.txt"));
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&file_path)?;
        file.write_all(format!("Content {i}").as_bytes())?;
    }
    println!("   Created 3 files in temp_structure");

    // Verify it exists
    println!(
        "   'temp_structure' exists before removal: {}",
        exists(&temp_structure)
    );

    // Remove the entire structure
    remove_dir_all(&temp_structure)?;
    println!("   Removed temp_structure and all contents");

    // Verify it's gone
    println!(
        "   'temp_structure' exists after removal: {}",
        exists(&temp_structure)
    );

    // Example 8: Demonstrating sorted order
    println!("\n8. Demonstrating deterministic sorted order:");
    let sorted_test = temp_path.join("sorted_test");
    create_dir_all(&sorted_test)?;

    // Create files in random order
    let files = ["zebra.txt", "apple.txt", "mango.txt", "banana.txt"];
    for filename in &files {
        let file_path = sorted_test.join(filename);
        OpenOptions::new()
            .create(true)
            .write(true)
            .open(&file_path)?;
    }
    println!("   Created files: {files:?}");

    println!("   Reading back (sorted alphabetically):");
    let entries = read_dir_sorted(&sorted_test)?;
    for entry in entries {
        let name = entry.file_name();
        println!("     - {}", name.to_string_lossy());
    }

    println!("\nDemo completed successfully!");
    println!("Temporary directory will be automatically cleaned up.");

    Ok(())
}
