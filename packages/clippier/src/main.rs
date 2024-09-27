use clap::Parser;
use toml::Value;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(index = 1)]
    file: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None).expect("Failed to initialize logging");

    let args = Args::parse();

    log::debug!("Loading file '{}'", args.file);
    let source = std::fs::read_to_string(args.file)?;
    let value: Value = toml::from_str(&source)?;

    if let Some(features) = value.get("features") {
        if let Some(features) = features.as_table() {
            println!(
                "{}",
                features.keys().cloned().collect::<Vec<_>>().join("\n")
            )
        }
    }

    Ok(())
}
