#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::io::{Read, Seek, SeekFrom, Write};
use switchy_fs::sync::{OpenOptions, create_dir_all, remove_dir_all};
use switchy_fs::tempdir;

fn main() -> std::io::Result<()> {
    println!("Demo: Basic File Operations with switchy_fs\n");

    // Create a temporary directory for our examples
    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path();
    println!("Using temporary directory: {}\n", temp_path.display());

    // Example 1: Create and write to a new file
    println!("1. Creating and writing to a new file:");
    let file_path = temp_path.join("example.txt");
    let mut file = OpenOptions::new()
        .create(true) // Create the file if it doesn't exist
        .write(true) // Open for writing
        .open(&file_path)?;

    file.write_all(b"Hello, switchy_fs!\n")?;
    file.write_all(b"This is line 2.\n")?;
    drop(file); // Close the file
    println!("   Created and wrote to: {}", file_path.display());

    // Example 2: Read file contents
    println!("\n2. Reading file contents:");
    let mut file = OpenOptions::new()
        .read(true) // Open for reading
        .open(&file_path)?;

    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    println!("   File contents:\n   {}", contents.replace('\n', "\n   "));
    drop(file);

    // Example 3: Append to existing file
    println!("3. Appending to existing file:");
    let mut file = OpenOptions::new()
        .append(true) // Open in append mode
        .open(&file_path)?;

    file.write_all(b"This line was appended.\n")?;
    drop(file);

    // Read and display updated contents
    let mut file = OpenOptions::new().read(true).open(&file_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    println!(
        "   Updated contents:\n   {}",
        contents.replace('\n', "\n   ")
    );
    drop(file);

    // Example 4: Seek operations
    println!("4. Using seek to read specific parts of the file:");
    let mut file = OpenOptions::new().read(true).open(&file_path)?;

    // Seek to position 7 (start of "switchy_fs")
    file.seek(SeekFrom::Start(7))?;
    let mut buffer = vec![0u8; 10];
    file.read_exact(&mut buffer)?;
    println!(
        "   Read 10 bytes from position 7: {}",
        String::from_utf8_lossy(&buffer)
    );

    // Seek from end
    file.seek(SeekFrom::End(-10))?;
    let mut buffer = vec![0u8; 9];
    file.read_exact(&mut buffer)?;
    println!(
        "   Read 9 bytes from 10 bytes before end: {}",
        String::from_utf8_lossy(&buffer)
    );
    drop(file);

    // Example 5: Truncate and overwrite
    println!("\n5. Truncating and overwriting file:");
    let mut file = OpenOptions::new()
        .write(true) // Open for writing
        .truncate(true) // Truncate file to 0 length
        .open(&file_path)?;

    file.write_all(b"File has been completely overwritten.\n")?;
    drop(file);

    let mut file = OpenOptions::new().read(true).open(&file_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    println!("   New contents:\n   {}", contents.replace('\n', "\n   "));
    drop(file);

    // Example 6: Read and write simultaneously
    println!("6. Opening file for both reading and writing:");
    let mut file = OpenOptions::new().read(true).write(true).open(&file_path)?;

    // Read current contents
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    println!("   Current contents: {}", contents.trim());

    // Write additional data
    file.write_all(b"Added via read-write mode.\n")?;

    // Seek back to start and read again
    file.seek(SeekFrom::Start(0))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    println!(
        "   Updated contents:\n   {}",
        contents.replace('\n', "\n   ")
    );
    drop(file);

    // Example 7: Working with nested directories
    println!("7. Creating files in nested directories:");
    let nested_path = temp_path.join("data").join("files").join("output");
    create_dir_all(&nested_path)?;
    println!("   Created directory structure: {}", nested_path.display());

    let nested_file = nested_path.join("data.txt");
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(&nested_file)?;
    file.write_all(b"Data in nested directory\n")?;
    drop(file);
    println!("   Created file: {}", nested_file.display());

    // Clean up nested directories
    let data_path = temp_path.join("data");
    remove_dir_all(&data_path)?;
    println!("   Cleaned up nested directories");

    println!("\nDemo completed successfully!");
    println!("Temporary directory will be automatically cleaned up.");

    Ok(())
}
