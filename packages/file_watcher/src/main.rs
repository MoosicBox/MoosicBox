#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use clap::Parser;
use moosicbox_file_watcher::{EventFilter, WatchError, watch_directory};
use std::path::PathBuf;

/// Cross-platform file watcher for monitoring filesystem changes
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Directory or file to watch
    #[arg(value_name = "PATH")]
    path: PathBuf,

    /// Quiet mode (suppress event output)
    #[arg(short, long)]
    quiet: bool,

    /// Monitor continuously (vs one-shot)
    #[arg(short, long)]
    monitor: bool,

    /// Comma-separated events to watch (modify,close_write,create,remove,access)
    #[arg(short, long, value_name = "EVENTS")]
    events: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(Some("moosicbox-file-watcher.log"), None)
        .expect("Failed to initialize logging");

    let args = Args::parse();

    // Parse event filter
    let filter = if let Some(events) = &args.events {
        EventFilter::parse(events)?
    } else {
        // Default to all events
        EventFilter::default()
            .with_modify()
            .with_close_write()
            .with_create()
            .with_remove()
            .with_access()
    };

    // Setup signal handling for graceful shutdown
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();

    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                log::info!("Received interrupt signal");
                r.store(false, std::sync::atomic::Ordering::SeqCst);
            }
            Err(err) => {
                log::error!("Error setting up signal handler: {err}");
            }
        }
    });

    // Watch the path
    if args.monitor {
        watch_directory_continuous(&args.path, filter, args.quiet, running)?;
    } else {
        watch_directory_once(&args.path, filter, args.quiet)?;
    }

    Ok(())
}

fn watch_directory_continuous(
    path: &PathBuf,
    filter: EventFilter,
    quiet: bool,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), WatchError> {
    watch_directory(path, filter, |event| {
        if !quiet {
            println!("{event:?}");
        }

        // Check if we should continue running
        if !running.load(std::sync::atomic::Ordering::SeqCst) {
            std::process::exit(0);
        }
    })
}

fn watch_directory_once(
    path: &PathBuf,
    filter: EventFilter,
    quiet: bool,
) -> Result<(), WatchError> {
    let mut event_received = false;

    watch_directory(path, filter, |event| {
        if !quiet {
            println!("{event:?}");
        }
        event_received = true;

        if event_received {
            std::process::exit(0);
        }
    })
}
