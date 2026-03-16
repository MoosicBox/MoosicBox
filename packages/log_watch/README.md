# moosicbox_log_watch

Generic log watching, filtering, and optional TUI.

## Description

`moosicbox_log_watch` provides reusable primitives for:

- resolving the active rolled log file by prefix (`active_log_file_path`)
- filtering lines with include/exclude regex rules (`LogFilterRule`, `line_visible_in_watch`)
- parsing `--since` duration values and RFC3339 timestamp cutoff checks
- optional profile/state persistence via JSON (`persistence-json` feature)
- optional interactive ratatui-based log watch UI (`tui` feature)

## Features

- `default` (empty)
- `persistence-json`: enables profile state persistence APIs (`profiles_list`, `profile_show`, `profile_delete`, `profile_rename`)
- `tui`: enables interactive watch mode entry point (`run_watch`)

## Installation

```toml
[dependencies]
moosicbox_log_watch = "0.1.0"
```

Enable optional features as needed:

```toml
[dependencies]
moosicbox_log_watch = { version = "0.1.0", features = ["persistence-json", "tui"] }
```

## Usage

Core filtering APIs:

```rust
use moosicbox_log_watch::{
    LogFilterCaseMode, LogFilterKind, LogFilterRule, line_visible_in_watch,
};

let filters = vec![
    LogFilterRule::new(
        LogFilterKind::Include,
        "ERROR|WARN".to_string(),
        LogFilterCaseMode::Sensitive,
    ),
    LogFilterRule::new(
        LogFilterKind::Exclude,
        "healthcheck".to_string(),
        LogFilterCaseMode::Insensitive,
    ),
];

assert!(line_visible_in_watch(
    "2026-01-01T00:00:00Z ERROR request failed",
    &filters,
    None,
));
```

Time-based helpers:

```rust
use moosicbox_log_watch::{line_matches_since, parse_since_cutoff};

let cutoff = parse_since_cutoff("10m")?;
assert!(line_matches_since("2026-01-01T00:00:00Z some log line", Some(cutoff)) == false);
# Ok::<(), anyhow::Error>(())
```

TUI entry point (`tui` feature):

```rust
#[cfg(feature = "tui")]
{
    use std::path::PathBuf;
    use moosicbox_log_watch::{WatchRunConfig, run_watch};

    run_watch(WatchRunConfig {
        title: "Server Logs".to_string(),
        log_dir: PathBuf::from("./logs"),
        log_file_prefix: "server.log".to_string(),
        lines: Some(200),
        since: Some("30m".to_string()),
        profile: Some("default".to_string()),
        include: Vec::new(),
        include_i: Vec::new(),
        exclude: Vec::new(),
        exclude_i: Vec::new(),
        state_file: Some(PathBuf::from("./watch-state.json")),
    })?;
}
# Ok::<(), anyhow::Error>(())
```

## License

Licensed under MPL-2.0.
