use std::env;
use std::path::Path;
use std::process::Command;

fn main() {
    // Get the package directory (where this build script is running)
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let web_dir = Path::new(&manifest_dir).join("web");

    // Specify the TypeScript source directory relative to this package
    let ts_src_dir = web_dir.join("src");

    // Run pnpm script
    let status = Command::new("pnpm")
        .arg("run")
        .arg("bundle") // Replace with your actual script name
        .current_dir(&web_dir) // Ensure we run in the correct directory
        .status()
        .expect("Failed to execute pnpm script");

    if !status.success() {
        panic!("pnpm script failed");
    }

    // Watch TypeScript source directory for changes
    println!("cargo:rerun-if-changed={}", ts_src_dir.display());

    // Recursively watch all files in the directory
    watch_directory(&ts_src_dir);

    // Force downstream crates to rebuild when this build script runs
    println!("cargo:rerun-if-changed=build.rs");

    // Tell Cargo that the main crate needs to be rebuilt if the TS files change
    println!("cargo:rebuild-if-changed=true");
}

fn watch_directory(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                println!("cargo:rerun-if-changed={}", path.display());
            } else if path.is_dir() {
                watch_directory(&path);
            }
        }
    }
}
