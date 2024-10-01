use clap::{Parser, Subcommand, ValueEnum};
use itertools::Itertools;
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
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    Features {
        #[arg(index = 1)]
        file: String,

        #[arg(long)]
        offset: Option<u16>,

        #[arg(long)]
        max: Option<u16>,

        #[arg(long)]
        chunked: Option<u16>,

        #[arg(short, long, value_enum, default_value_t=OutputType::Raw)]
        output: OutputType,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(None).expect("Failed to initialize logging");

    let args = Args::parse();

    match args.cmd {
        Commands::Features {
            file,
            output,
            offset,
            max,
            chunked,
        } => {
            log::debug!("Loading file '{}'", file);
            let source = std::fs::read_to_string(file)?;
            let value: Value = toml::from_str(&source)?;

            if let Some(features) = value.get("features") {
                if let Some(features) = features.as_table() {
                    let offset = offset.unwrap_or_default().into();
                    let feature_count = features.keys().len() - offset;
                    let features = features
                        .keys()
                        .skip(offset)
                        .take(
                            max.map(|x| std::cmp::min(feature_count, x as usize))
                                .unwrap_or(feature_count),
                        )
                        .cloned()
                        .collect::<Vec<_>>();

                    match output {
                        OutputType::Json => {
                            if let Some(chunked) = chunked {
                                let features = features
                                    .into_iter()
                                    .chunks(chunked as usize)
                                    .into_iter()
                                    .map(|x| x.collect::<Vec<_>>())
                                    .collect::<Vec<_>>();
                                println!("{}", serde_json::to_value(features).unwrap());
                            } else {
                                println!("{}", serde_json::to_value(features).unwrap());
                            }
                        }
                        OutputType::Raw => {
                            if chunked.is_some() {
                                panic!("chunked arg is not supported for raw output");
                            }
                            println!("{}", features.join("\n"));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
