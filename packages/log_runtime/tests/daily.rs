#![cfg(feature = "init")]

use moosicbox_log_runtime::LogRuntimePaths;
use moosicbox_log_runtime::init::{FileMode, FileSinkConfig, InitConfig, init};
use std::fs;
use time::OffsetDateTime;

#[test]
fn nested_daily_prefix_opens_immediately_and_appends() {
    let dir = tempfile::tempdir().unwrap();
    let paths = LogRuntimePaths {
        state_dir: dir.path().into(),
        log_dir: dir.path().into(),
    };
    let parent = dir.path().join("nested");
    fs::create_dir(&parent).unwrap();
    let today = OffsetDateTime::now_utc().date();
    // Seed both possible dates in case the test crosses UTC midnight.
    for day in [today, today.next_day().unwrap()] {
        fs::write(parent.join(format!("app.{day}")), "old\n").unwrap();
    }
    let mut cfg = InitConfig::new(&paths);
    cfg.sinks.stderr = false;
    cfg.env_filter = Some("info".into());
    cfg.sinks.file = Some(FileSinkConfig {
        mode: FileMode::RollingDaily("nested/app"),
    });
    let handle = init(cfg).unwrap();
    tracing::info!("daily marker");
    drop(handle);
    let files: Vec<_> = fs::read_dir(parent)
        .unwrap()
        .map(|p| fs::read_to_string(p.unwrap().path()).unwrap())
        .collect();
    assert_eq!(files.len(), 2);
    assert!(
        files
            .iter()
            .any(|s| s.starts_with("old\n") && s.ends_with(" INFO daily marker\n"))
    );
}
