//! End-to-end coverage of explicit clock driving and nested task completion.
#![cfg(feature = "simulator")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[test]
fn seeded_example_finishes_all_nested_tasks() {
    for seed in ["0", "42", "12345"] {
        let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_async_simulated"))
            .env("SIMULATOR_SEED", seed)
            .env("SIMULATOR_STEP_MULTIPLIER", "1")
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if std::time::Instant::now() >= deadline {
                child.kill().unwrap();
                let output = child.wait_with_output().unwrap();
                panic!("example stalled for seed {seed}: {output:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "seed {seed}: {output:?}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(stdout.contains("Blocking Function Polled To Completion"));
        for actor in 0..5 {
            for event in ["Start", "Ended", "Inner"] {
                assert_eq!(
                    stdout
                        .matches(&format!("Spawned Fn #{actor:02}: {event} "))
                        .count(),
                    1,
                    "seed {seed}: {stdout}"
                );
            }
        }
        assert!(stdout.trim_end().ends_with("End of Asynchronous Execution"));
    }
}
