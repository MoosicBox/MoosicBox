use std::env;
use std::path::Path;
use std::process::Command;

static NPM_COMMANDS: [&str; 3] = ["pnpm", "bun", "npm"];

fn main() {
    // Get the package directory (where this build script is running)
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let web_dir = Path::new(&manifest_dir).join("web");

    // Specify the TypeScript source directory relative to this package
    let ts_src_dir = web_dir.join("src");

    run_command(&NPM_COMMANDS, &["install"], &web_dir);
    run_command(&NPM_COMMANDS, &["run", "bundle"], &web_dir);

    // Watch TypeScript source directory for changes
    println!("cargo:rerun-if-changed={}", ts_src_dir.display());

    // Force downstream crates to rebuild when this build script runs
    println!("cargo:rerun-if-changed=build.rs");
}

fn run_command(binaries: &[&str], arguments: &[&str], dir: &Path) {
    for binary in binaries {
        let mut command = Command::new(binary);
        let mut command = command.current_dir(dir);

        for arg in arguments {
            command = command.arg(arg);
        }

        match command.spawn() {
            Ok(mut child) => {
                let status = child
                    .wait()
                    .unwrap_or_else(|e| panic!("Failed to execute {binary} script: {e:?}"));

                if !status.success() {
                    panic!("{binary} script failed: status_code={:?}", status.code());
                }

                return;
            }
            Err(e) => {
                if let std::io::ErrorKind::NotFound = e.kind() {
                    continue;
                }
                panic!("Failed to execute {binary} script: {e:?}");
            }
        }
    }
}
