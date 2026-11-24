#![cfg(feature = "init")]

use moosicbox_log_runtime::LogRuntimePaths;
use moosicbox_log_runtime::init::{FileMode, FileSinkConfig, InitConfig, init};
use std::fs;

// Separate test executable: initialization installs a process-global subscriber.
#[test]
fn exact_sink_appends_formatted_filtered_records() {
    let dir = tempfile::tempdir().unwrap();
    let paths = LogRuntimePaths {
        state_dir: dir.path().into(),
        log_dir: dir.path().into(),
    };
    let path = dir.path().join("trace.log");
    fs::write(&path, b"old\n").unwrap();
    let mut cfg = InitConfig::new(&paths);
    cfg.sinks.stderr = false;
    cfg.env_filter = Some("off,contract=info".into());
    cfg.with_target = true;
    cfg.sinks.file = Some(FileSinkConfig {
        mode: FileMode::Exact("trace.log"),
    });
    let handle = init(cfg).unwrap();
    tracing::info!(target: "contract", "first");
    tracing::debug!(target: "contract", "not written");
    tracing::info!(target: "contract", n = 2, "second");
    drop(handle);
    let contents = fs::read_to_string(path).unwrap();
    let mut lines = contents.lines();
    assert_eq!(lines.next(), Some("old"));
    for expected in [" INFO contract: first", " INFO contract: second n=2"] {
        let (timestamp, rest) = lines.next().unwrap().split_once(' ').unwrap();
        assert!(timestamp.ends_with('Z'));
        assert_eq!(rest, expected);
    }
    assert!(lines.next().is_none());
    assert!(contents.ends_with('\n'));
}
