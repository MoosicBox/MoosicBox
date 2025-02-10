use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::LazyLock;

static NPM_COMMANDS: [&str; 3] = ["pnpm", "bun", "npm"];

static ENABLED_NPM_COMMANDS: LazyLock<Vec<String>> = LazyLock::new(|| {
    NPM_COMMANDS
        .iter()
        .filter(|x| match **x {
            #[cfg(feature = "pnpm")]
            "pnpm" => true,
            #[cfg(feature = "bun")]
            "bun" => true,
            #[cfg(feature = "npm")]
            "npm" => true,
            _ => false,
        })
        .map(|x| {
            if *x == "pnpm" {
                if let Ok(var) = env::var("PNPM_HOME") {
                    PathBuf::from_str(&var)
                        .unwrap()
                        .join(if cfg!(windows) {
                            format!("{x}.CMD")
                        } else {
                            x.to_string()
                        })
                        .to_str()
                        .unwrap()
                        .to_string()
                } else {
                    x.to_string()
                }
            } else {
                x.to_string()
            }
        })
        .collect::<Vec<_>>()
});

static MANIFEST_DIR_STR: &str = env!("CARGO_MANIFEST_DIR");
static MANIFEST_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| PathBuf::from_str(MANIFEST_DIR_STR).unwrap());

pub fn bundle(target: &Path, out: &Path) {
    if cfg!(feature = "swc") {
        #[cfg(feature = "swc")]
        return bundle_swc(target, out);
    } else if cfg!(feature = "esbuild") {
        #[cfg(feature = "esbuild")]
        return bundle_esbuild(target, out);
    }

    panic!("No bundlers enabled");
}

#[cfg(feature = "swc")]
pub fn bundle_swc(target: &Path, out: &Path) {
    unimplemented!()
}

#[cfg(feature = "esbuild")]
pub fn bundle_esbuild(target: &Path, out: &Path) {
    run_npm_command(&["install"], &MANIFEST_DIR);
    run_command(
        std::iter::once(
            &MANIFEST_DIR
                .join("node_modules")
                .join(".bin")
                .join("esbuild")
                .to_str()
                .unwrap()
                .to_string(),
        ),
        &[
            target.to_str().unwrap(),
            "--minify",
            "--bundle",
            &format!("--outfile={}", out.display()),
        ],
        &MANIFEST_DIR,
    );
}

pub fn run_npm_command(arguments: &[&str], dir: &Path) {
    run_command(ENABLED_NPM_COMMANDS.iter(), arguments, dir);
}

fn run_command<'a>(binaries: impl Iterator<Item = &'a String>, arguments: &[&str], dir: &Path) {
    for ref binary in binaries {
        let mut command = Command::new(binary);
        let mut command = command.current_dir(dir);

        for arg in arguments {
            command = command.arg(arg);
        }

        println!("Running {binary} {}", arguments.join(" "));

        match command.spawn() {
            Ok(mut child) => {
                let status = child
                    .wait()
                    .unwrap_or_else(|e| panic!("Failed to execute {binary} script: {e:?}"));

                if !status.success() {
                    if status.code() == Some(127) {
                        println!("Binary {binary} not found (status code 127)");
                        continue;
                    }

                    panic!("{binary} script failed: status_code={:?}", status.code());
                }

                return;
            }
            Err(e) => {
                if let std::io::ErrorKind::NotFound = e.kind() {
                    println!("Binary {binary} not found");
                    continue;
                }
                panic!("Failed to execute {binary} script: {e:?}");
            }
        }
    }

    panic!("Failed to execute script for any of the binaries");
}
