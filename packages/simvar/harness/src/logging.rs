//! Logging utilities for simulation output.
//!
//! This module provides logging functionality that adapts to whether the TUI
//! (terminal user interface) is enabled or not.

use crate::USE_TUI;

/// Logs a message either to the log system or stdout.
///
/// When the TUI is enabled, messages are sent to the log system.
/// When the TUI is disabled, messages are printed directly to stdout.
pub fn log_message(msg: impl Into<String>) {
    let msg = msg.into();

    if USE_TUI {
        log::info!("{msg}");
    } else {
        println!("{msg}");
    }
}

#[cfg(feature = "pretty_env_logger")]
/// Initializes the pretty environment logger with custom formatting.
///
/// This configures the logger to include thread IDs, targets, levels, and
/// optional host/client names in log output. When the TUI is enabled, logs
/// are written to `.log/simulation.log` instead of stdout.
///
/// # Errors
///
/// * Returns an error if the log directory cannot be created
/// * Returns an error if the log file cannot be created
#[allow(clippy::unnecessary_wraps)]
pub fn init_pretty_env_logger() -> std::io::Result<()> {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use crate::{client::current_client, host::current_host};

    const NO_LOG: bool = std::option_env!("NO_LOG").is_some();

    if NO_LOG {
        return Ok(());
    }

    let mut builder = pretty_env_logger::formatted_builder();

    #[cfg(feature = "tui")]
    if USE_TUI {
        use std::{fs::File, path::PathBuf, str::FromStr as _};

        use pretty_env_logger::env_logger::Target;

        let log_dir = PathBuf::from_str(".log").unwrap();
        std::fs::create_dir_all(&log_dir)?;
        let simulation_log_file = log_dir.join("simulation.log");
        let file = File::create(simulation_log_file)?;

        builder.target(Target::Pipe(Box::new(file)));
    }

    builder
        .parse_default_env()
        .format(|buf, record| {
            static MAX_THREAD_PREFIX_LEN: AtomicUsize = AtomicUsize::new(0);
            static MAX_TARGET_PREFIX_LEN: AtomicUsize = AtomicUsize::new(0);
            static MAX_LEVEL_PREFIX_LEN: AtomicUsize = AtomicUsize::new(0);

            use std::io::Write as _;

            use pretty_env_logger::env_logger::fmt::Color;

            let target = record.target();

            let mut style = buf.style();
            let level = record.level();
            let level_style = style.set_color(match level {
                log::Level::Error => Color::Red,
                log::Level::Warn => Color::Yellow,
                log::Level::Info => Color::Green,
                log::Level::Debug => Color::Blue,
                log::Level::Trace => Color::Magenta,
            });

            let thread_id = switchy::unsync::thread_id();
            let ts = buf.timestamp_millis();
            let level_prefix_len = "[]".len() + level.to_string().len();
            let thread_prefix_len = "[Thread ]".len() + thread_id.to_string().len();
            let target_prefix_len = "[]".len() + target.len();

            let mut max_level_prefix_len = MAX_LEVEL_PREFIX_LEN.load(Ordering::SeqCst);
            if level_prefix_len > max_level_prefix_len {
                max_level_prefix_len = level_prefix_len;
                MAX_LEVEL_PREFIX_LEN.store(level_prefix_len, Ordering::SeqCst);
            }
            let level_padding = max_level_prefix_len - level_prefix_len;

            let mut max_thread_prefix_len = MAX_THREAD_PREFIX_LEN.load(Ordering::SeqCst);
            if thread_prefix_len > max_thread_prefix_len {
                max_thread_prefix_len = thread_prefix_len;
                MAX_THREAD_PREFIX_LEN.store(thread_prefix_len, Ordering::SeqCst);
            }
            let thread_padding = max_thread_prefix_len - thread_prefix_len;

            let mut max_target_prefix_len = MAX_TARGET_PREFIX_LEN.load(Ordering::SeqCst);
            if target_prefix_len > max_target_prefix_len {
                max_target_prefix_len = target_prefix_len;
                MAX_TARGET_PREFIX_LEN.store(target_prefix_len, Ordering::SeqCst);
            }
            let target_padding = max_target_prefix_len - target_prefix_len;

            write!(
                buf,
                "\
                [{ts}] \
                [Thread {thread_id}] {empty:<thread_padding$}\
                [{target}] {empty:<target_padding$}\
                [{level}] {empty:<level_padding$}\
                ",
                empty = "",
                level = level_style.value(level),
            )?;

            if let Some(host) = current_host() {
                let mut style = buf.style();
                let host_style = style.set_color(Color::Cyan);
                write!(buf, "[{host}] ", host = host_style.value(host))?;
            }

            if let Some(host) = current_client() {
                let mut style = buf.style();
                let host_style = style.set_color(Color::Cyan);
                write!(buf, "[{host}] ", host = host_style.value(host))?;
            }

            writeln!(buf, "{args}", args = record.args())
        })
        .init();

    Ok(())
}
