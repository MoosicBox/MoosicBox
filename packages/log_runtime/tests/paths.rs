use moosicbox_log_runtime::{LogRuntimePathsConfig, resolve_paths};
use std::path::PathBuf;

const STATE: &str = "SWITCHY_LOG_TEST_STATE";
const LOG: &str = "SWITCHY_LOG_TEST_LOG";
const CHILD: &str = "SWITCHY_LOG_TEST_CASE";

#[test]
fn path_resolution_preserves_precedence() {
    if let Ok(case) = std::env::var(CHILD) {
        let run = || check_case(&case);
        #[cfg(feature = "simulator")]
        {
            use std::sync::Arc;
            use switchy_fs::{
                directories::DirectoryLocations,
                simulator::{Filesystem, with_filesystem},
            };
            let locations = if case == "missing" {
                DirectoryLocations::default()
            } else {
                DirectoryLocations {
                    home: Some("virtual-home".into()),
                    data_local: Some("virtual-local".into()),
                    ..DirectoryLocations::default()
                }
            };
            with_filesystem(
                &Arc::new(Filesystem::with_directory_locations(locations)),
                || {
                    assert_eq!(
                        switchy_fs::directories::home_dir(),
                        (case != "missing").then(|| PathBuf::from("virtual-home"))
                    );
                    assert_eq!(
                        switchy_fs::directories::data_local_dir(),
                        (case != "missing").then(|| PathBuf::from("virtual-local"))
                    );
                    run();
                },
            );
        }
        #[cfg(not(feature = "simulator"))]
        run();
        return;
    }
    // Isolate process environment; never mutate it while other tests run.
    for case in ["default", "state", "both", "empty", "xdg", "missing"] {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "path_resolution_preserves_precedence"])
            .env(CHILD, case)
            .env_remove(STATE)
            .env_remove(LOG)
            .env_remove("XDG_STATE_HOME");
        match case {
            "state" => {
                command.env(STATE, "explicit-state");
            }
            "both" => {
                command
                    .env(STATE, "explicit-state")
                    .env(LOG, "explicit-log");
            }
            "empty" => {
                command.env(STATE, "").env(LOG, "");
            }
            "xdg" => {
                command.env("XDG_STATE_HOME", "relative-xdg");
            }
            _ => {}
        }
        assert!(command.status().unwrap().success(), "case {case}");
    }
}

fn check_case(case: &str) {
    let config = LogRuntimePathsConfig {
        app_name: "test-app",
        state_dir_env: STATE,
        log_dir_env: LOG,
    };
    let paths = resolve_paths(&config);
    let (default_state, default_log) = platform_defaults();
    let expected_state = match case {
        "state" | "both" => PathBuf::from("explicit-state"),
        "empty" => PathBuf::new(),
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        "xdg" => PathBuf::from("relative-xdg/test-app"),
        _ => default_state,
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let default_log = {
        let _ = default_log;
        expected_state.join("logs")
    };
    let expected_log = match case {
        "both" => PathBuf::from("explicit-log"),
        "empty" => PathBuf::new(),
        _ => default_log,
    };
    assert_eq!(paths.state_dir, expected_state);
    assert_eq!(paths.log_dir, expected_log);
}

fn platform_defaults() -> (PathBuf, PathBuf) {
    #[cfg(target_os = "macos")]
    {
        let home = switchy_fs::directories::home_dir();
        (
            home.as_ref().map_or_else(
                || "./test-app/state".into(),
                |h| h.join("Library/Application Support/test-app/State"),
            ),
            home.map_or_else(
                || "./test-app/logs".into(),
                |h| h.join("Library/Logs/test-app"),
            ),
        )
    }
    #[cfg(target_os = "windows")]
    {
        let local = switchy_fs::directories::data_local_dir();
        (
            local
                .as_ref()
                .map_or_else(|| "./test-app/state".into(), |h| h.join("test-app/State")),
            local.map_or_else(|| "./test-app/logs".into(), |h| h.join("test-app/Logs")),
        )
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let state = switchy_fs::directories::home_dir().map_or_else(
            || PathBuf::from("./test-app/state"),
            |h| h.join(".local/state/test-app"),
        );
        let logs = state.join("logs");
        (state, logs)
    }
}
