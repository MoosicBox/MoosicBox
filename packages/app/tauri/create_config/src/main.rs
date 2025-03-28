#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    bundled: bool,

    #[arg(short, long)]
    output: String,
}

fn main() {
    moosicbox_logging::init(None, None).expect("Failed to initialize FreeLog");

    let args = Args::parse();

    moosicbox_app_create_config::generate(args.bundled, args.output);
}
