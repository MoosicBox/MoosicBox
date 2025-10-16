#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![forbid(unsafe_code)]

use std::env;
use std::process;

fn print_usage() {
    eprintln!("Usage: openport [START END] [--inclusive]");
    eprintln!();
    eprintln!("Find an available port on the system.");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  openport              # Find any available port");
    eprintln!("  openport 15000 16000  # Find port in range 15000..16000");
    eprintln!("  openport 8000 9000 --inclusive  # Find port in range 8000..=9000");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    match args.len() {
        1 => {
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
        3 | 4 => {
            // Parse range arguments
            let start: u16 = args[1].parse().unwrap_or_else(|_| {
                eprintln!("Error: Invalid start port '{}'", args[1]);
                print_usage();
                process::exit(1);
            });

            let end: u16 = args[2].parse().unwrap_or_else(|_| {
                eprintln!("Error: Invalid end port '{}'", args[2]);
                print_usage();
                process::exit(1);
            });

            if start >= end {
                eprintln!("Error: Start port must be less than end port");
                process::exit(1);
            }

            // Check for --inclusive flag
            let inclusive = args.len() == 4 && args[3] == "--inclusive";

            let port = if inclusive {
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
        _ => {
            eprintln!("Error: Invalid arguments");
            print_usage();
            process::exit(1);
        }
    }
}
