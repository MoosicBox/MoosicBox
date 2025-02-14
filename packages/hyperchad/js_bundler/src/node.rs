use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr as _,
    sync::LazyLock,
};

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
        .map(ToString::to_string)
        .collect::<Vec<_>>()
});

pub fn run_npm_command(arguments: &[&str], dir: &Path) {
    run_command(ENABLED_NPM_COMMANDS.clone().into_iter(), arguments, dir);
}

pub(crate) fn run_command(binaries: impl Iterator<Item = String>, arguments: &[&str], dir: &Path) {
    for ref binary in binaries
        .map(|x| PathBuf::from_str(&x).unwrap())
        .map(|x| {
            if x.file_name().is_some_and(|x| x == "pnpm") {
                if let Ok(var) = env::var("PNPM_HOME") {
                    return PathBuf::from_str(&var).unwrap().join(x);
                }
            }

            x
        })
        .map(fixup_binary_filename)
        .map(|x| x.to_str().unwrap().to_string())
    {
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

fn fixup_binary_filename(binary: PathBuf) -> PathBuf {
    if cfg!(windows) {
        let parent = binary.parent();

        if let Some(parent) = parent {
            let cmd = parent.join(format!(
                "{}.CMD",
                binary.file_name().unwrap().to_str().unwrap()
            ));

            if cmd.is_file() {
                return cmd;
            }
        }
    }

    binary
}
