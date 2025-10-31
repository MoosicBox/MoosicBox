#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::io::{Read, Seek, SeekFrom, Write};
use switchy_fs::sync::OpenOptions;
use switchy_fs::sync::{create_dir_all, read_dir_sorted, read_to_string, remove_dir_all, write};

fn main() -> std::io::Result<()> {
    println!("switchy_fs Basic File I/O Example\n");
    println!("This example demonstrates core file operations using switchy_fs.");
    println!("The same code works with both real and simulated filesystems!\n");

    // Reset simulator filesystem if in simulator mode
    #[cfg(feature = "simulator")]
    {
        switchy_fs::simulator::reset_fs();
        println!("Note: Running in SIMULATOR mode - all operations happen in-memory\n");
    }

    #[cfg(all(feature = "std", not(feature = "simulator")))]
    {
        println!("Note: Running in STANDARD mode - using real filesystem\n");
    }

    // Example 1: Create directories
    println!("=== Example 1: Creating Directories ===");
    let base_path = if cfg!(feature = "simulator") {
        "/tmp/switchy_example"
    } else {
        "target/switchy_example"
    };

    create_dir_all(base_path)?;
    println!("Created directory: {base_path}");

    create_dir_all(format!("{base_path}/subdir/nested"))?;
    println!("Created nested directories: {base_path}/subdir/nested\n");

    // Example 2: Write files using helper function
    println!("=== Example 2: Writing Files (Simple) ===");
    write(format!("{base_path}/hello.txt"), b"Hello, World!")?;
    println!("Wrote: {base_path}/hello.txt");

    write(
        format!("{base_path}/subdir/data.txt"),
        b"This is nested data",
    )?;
    println!("Wrote: {base_path}/subdir/data.txt\n");

    // Example 3: Read files using helper function
    println!("=== Example 3: Reading Files (Simple) ===");
    let content = read_to_string(format!("{base_path}/hello.txt"))?;
    println!("Read from hello.txt: \"{content}\"");

    let nested_content = read_to_string(format!("{base_path}/subdir/data.txt"))?;
    println!("Read from data.txt: \"{nested_content}\"\n");

    // Example 4: Using OpenOptions for more control
    println!("=== Example 4: Using OpenOptions ===");
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(format!("{base_path}/config.txt"))?;

    writeln!(file, "# Configuration File")?;
    writeln!(file, "setting1=value1")?;
    writeln!(file, "setting2=value2")?;
    println!("Created and wrote to: {base_path}/config.txt");

    // Need to drop the file handle before reading
    drop(file);

    let config_content = read_to_string(format!("{base_path}/config.txt"))?;
    println!("Config contents:\n{config_content}");

    // Example 5: Appending to files
    println!("=== Example 5: Appending to Files ===");
    let mut append_file = OpenOptions::new()
        .append(true)
        .open(format!("{base_path}/config.txt"))?;

    writeln!(append_file, "setting3=appended")?;
    println!("Appended line to config.txt");
    drop(append_file);

    let updated_content = read_to_string(format!("{base_path}/config.txt"))?;
    println!("Updated config:\n{updated_content}");

    // Example 6: Reading and seeking
    println!("=== Example 6: Reading with Seeking ===");
    let mut read_file = OpenOptions::new()
        .read(true)
        .open(format!("{base_path}/hello.txt"))?;

    // Read first 5 bytes
    let mut buffer = vec![0u8; 5];
    read_file.read_exact(&mut buffer)?;
    println!("First 5 bytes: {}", String::from_utf8_lossy(&buffer));

    // Seek back to start
    read_file.seek(SeekFrom::Start(0))?;
    let mut full_content = String::new();
    read_file.read_to_string(&mut full_content)?;
    println!("Full content after seeking: \"{full_content}\"");
    drop(read_file);
    println!();

    // Example 7: Reading directories
    println!("=== Example 7: Reading Directories ===");
    let entries = read_dir_sorted(base_path)?;
    println!("Files in {base_path}:");
    for entry in entries {
        println!("  - {}", entry.file_name().to_string_lossy());
    }
    println!();

    // Example 8: Cleanup
    println!("=== Example 8: Removing Directories ===");
    remove_dir_all(base_path)?;
    println!("Removed directory: {base_path}");

    println!("\n✅ All examples completed successfully!");

    Ok(())
}
