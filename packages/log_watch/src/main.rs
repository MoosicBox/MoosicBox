#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(name = "moosicbox-log-watch")]
#[command(about = "Interactive MoosicBox log watcher")]
struct Cli {
    #[arg(
        long,
        env = "MOOSICBOX_LOG_WATCH_TITLE",
        default_value = "MoosicBox Log Watch"
    )]
    title: String,

    #[arg(long, env = "MOOSICBOX_LOG_DIR")]
    log_dir: Option<PathBuf>,

    #[arg(
        long,
        env = "MOOSICBOX_LOG_FILE_PREFIX",
        default_value = "moosicbox_server.log"
    )]
    log_file_prefix: String,

    #[arg(long, env = "MOOSICBOX_LOG_WATCH_LINES")]
    lines: Option<usize>,

    #[arg(long, env = "MOOSICBOX_LOG_WATCH_SINCE")]
    since: Option<String>,

    #[arg(long, env = "MOOSICBOX_LOG_WATCH_PROFILE")]
    profile: Option<String>,

    #[arg(long = "include")]
    include: Vec<String>,

    #[arg(long = "include-i")]
    include_i: Vec<String>,

    #[arg(long = "exclude")]
    exclude: Vec<String>,

    #[arg(long = "exclude-i")]
    exclude_i: Vec<String>,

    #[arg(long, env = "MOOSICBOX_LOG_WATCH_STATE_FILE")]
    state_file: Option<PathBuf>,
}

fn default_paths() -> moosicbox_log_runtime::LogRuntimePaths {
    moosicbox_log_runtime::resolve_paths(&moosicbox_log_runtime::LogRuntimePathsConfig {
        app_name: "moosicbox",
        state_dir_env: "MOOSICBOX_STATE_DIR",
        log_dir_env: "MOOSICBOX_LOG_DIR",
    })
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let paths = default_paths();
    let log_dir = cli.log_dir.unwrap_or(paths.log_dir);
    let state_file = cli
        .state_file
        .or_else(|| Some(paths.state_dir.join("log_watch_state.json")));

    moosicbox_log_watch::run_watch(moosicbox_log_watch::WatchRunConfig {
        title: cli.title,
        log_dir,
        log_file_prefix: cli.log_file_prefix,
        lines: cli.lines,
        since: cli.since,
        profile: cli.profile,
        include: cli.include,
        include_i: cli.include_i,
        exclude: cli.exclude,
        exclude_i: cli.exclude_i,
        state_file,
    })
}
