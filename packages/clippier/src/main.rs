use clap::{Parser, ValueEnum};
use toml::Value;

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[clap(rename_all = "kebab_case")]
pub enum OutputType {
    Json,
    Raw,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(index = 1)]
    file: String,

    #[arg(short, long, value_enum, default_value_t=OutputType::Raw)]
    output: OutputType,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None).expect("Failed to initialize logging");

    let args = Args::parse();

    log::debug!("Loading file '{}'", args.file);
    let source = std::fs::read_to_string(args.file)?;
    let value: Value = toml::from_str(&source)?;

    if let Some(features) = value.get("features") {
        if let Some(features) = features.as_table() {
            match args.output {
                OutputType::Json => {
                    println!(
                        "{}",
                        serde_json::to_value(features.keys().cloned().collect::<Vec<_>>()).unwrap()
                    );
                }
                OutputType::Raw => {
                    println!(
                        "{}",
                        features.keys().cloned().collect::<Vec<_>>().join("\n")
                    );
                }
            }
        }
    }

    Ok(())
}
