#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![forbid(unsafe_code)]

//! Command-line interface for finding available network ports
//!
//! This binary provides a simple CLI to find available ports on the system.
//! It supports finding any available port (with the `rand` feature) or searching
//! within a specific port range (exclusive or inclusive).
//!
//! # Usage
//!
//! Find any available port (requires `rand` feature):
//! ```bash
//! openport
//! ```
//!
//! Find an available port in a range (exclusive):
//! ```bash
//! openport 15000 16000
//! ```
//!
//! Find an available port in a range (inclusive):
//! ```bash
//! openport 15000 16000 --inclusive
//! ```

use clap::Parser;
use std::process;

/// Find an available port on the system
#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Start of the port range
    #[arg(value_name = "START")]
    start: Option<u16>,

    /// End of the port range
    #[arg(value_name = "END", requires = "start")]
    end: Option<u16>,

    /// Use inclusive range (includes the end port)
    #[arg(long)]
    inclusive: bool,
}

fn main() {
    let cli = Cli::parse();

    match (cli.start, cli.end) {
        (None, None) => {
            // No arguments - find any available port using rand feature
            #[cfg(feature = "rand")]
            {
                if let Some(port) = openport::pick_random_unused_port() {
                    println!("{port}");
                } else {
                    eprintln!("Error: No available ports found");
                    process::exit(1);
                }
            }
            #[cfg(not(feature = "rand"))]
            {
                eprintln!("Error: The 'rand' feature is required to find any available port");
                eprintln!("Please rebuild with: cargo build --features rand");
                process::exit(1);
            }
        }
        (Some(start), Some(end)) => {
            if start >= end {
                eprintln!("Error: Start port must be less than end port");
                process::exit(1);
            }

            let port = if cli.inclusive {
                openport::pick_unused_port(start..=end)
            } else {
                openport::pick_unused_port(start..end)
            };

            if let Some(port) = port {
                println!("{port}");
            } else {
                eprintln!("Error: No available ports found in range {start}..{end}");
                process::exit(1);
            }
        }
        _ => unreachable!("clap ensures START and END are provided together"),
    }
}
