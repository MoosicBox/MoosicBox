#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::io::{Result, SeekFrom};
use switchy_async::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use switchy_fs::tempdir;
use switchy_fs::unsync::{OpenOptions, create_dir_all, remove_dir_all};

#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() -> Result<()> {
    println!("Demo: Async File Operations with switchy_fs\n");

    // Create a temporary directory for our examples
    let temp_dir = tempdir()?;
    let temp_path = temp_dir.path();
    println!("Using temporary directory: {}\n", temp_path.display());

    // Example 1: Create and write to a new file asynchronously
    println!("1. Creating and writing to a new file (async):");
    let file_path = temp_path.join("async_example.txt");
    let mut file = OpenOptions::new()
        .create(true) // Create the file if it doesn't exist
        .write(true) // Open for writing
        .open(&file_path)
        .await?;

    file.write_all(b"Hello, async switchy_fs!\n").await?;
    file.write_all(b"This is an async operation.\n").await?;
    drop(file); // Close the file
    println!("   Created and wrote to: {}", file_path.display());

    // Example 2: Read file contents asynchronously
    println!("\n2. Reading file contents (async):");
    let mut file = OpenOptions::new().read(true).open(&file_path).await?;

    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;
    println!("   File contents:\n   {}", contents.replace('\n', "\n   "));
    drop(file);

    // Example 3: Append to existing file asynchronously
    println!("3. Appending to existing file (async):");
    let mut file = OpenOptions::new().append(true).open(&file_path).await?;

    file.write_all(b"This line was appended asynchronously.\n")
        .await?;
    drop(file);

    // Read and display updated contents
    let mut file = OpenOptions::new().read(true).open(&file_path).await?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;
    println!(
        "   Updated contents:\n   {}",
        contents.replace('\n', "\n   ")
    );
    drop(file);

    // Example 4: Async seek operations
    println!("4. Using async seek to read specific parts:");
    let mut file = OpenOptions::new().read(true).open(&file_path).await?;

    // Seek to position 7 (start of "async")
    file.seek(SeekFrom::Start(7)).await?;
    let mut buffer = vec![0u8; 5];
    file.read_exact(&mut buffer).await?;
    println!(
        "   Read 5 bytes from position 7: {}",
        String::from_utf8_lossy(&buffer)
    );

    // Seek from end
    file.seek(SeekFrom::End(-15)).await?;
    let mut buffer = vec![0u8; 14];
    file.read_exact(&mut buffer).await?;
    println!(
        "   Read 14 bytes from 15 bytes before end: {}",
        String::from_utf8_lossy(&buffer)
    );
    drop(file);

    // Example 5: Concurrent file operations
    println!("\n5. Performing concurrent file operations:");

    // Create multiple files concurrently
    let files = ["file1.txt", "file2.txt", "file3.txt"];
    let mut tasks = Vec::new();

    for (i, filename) in files.iter().enumerate() {
        let file_path = temp_path.join(filename);
        let filename = (*filename).to_string();
        let task = tokio::spawn(async move {
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .open(&file_path)
                .await?;

            file.write_all(format!("Content for file {}\n", i + 1).as_bytes())
                .await?;
            Result::Ok(filename)
        });
        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        let filename = task.await.expect("Task panicked")?;
        println!("   Created: {filename}");
    }

    // Example 6: Read and write simultaneously (async)
    println!("\n6. Async read-write operations:");
    let rw_path = temp_path.join("readwrite.txt");
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&rw_path)
        .await?;

    // Write some initial content
    file.write_all(b"Initial content\n").await?;

    // Seek back to start and read
    file.seek(SeekFrom::Start(0)).await?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;
    println!("   Current contents: {}", contents.trim());

    // Append more content
    file.write_all(b"Added via async read-write\n").await?;

    // Read complete file
    file.seek(SeekFrom::Start(0)).await?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;
    println!(
        "   Updated contents:\n   {}",
        contents.replace('\n', "\n   ")
    );
    drop(file);

    // Example 7: Async directory operations
    println!("7. Async directory operations:");
    let nested_path = temp_path.join("async").join("nested").join("dirs");
    create_dir_all(&nested_path).await?;
    println!("   Created directory structure: {}", nested_path.display());

    let nested_file = nested_path.join("async_data.txt");
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(&nested_file)
        .await?;
    file.write_all(b"Data in async nested directory\n").await?;
    drop(file);
    println!("   Created file: {}", nested_file.display());

    // Clean up nested directories
    let async_path = temp_path.join("async");
    remove_dir_all(&async_path).await?;
    println!("   Cleaned up nested directories");

    println!("\nDemo completed successfully!");
    println!("Temporary directory will be automatically cleaned up.");

    Ok(())
}
