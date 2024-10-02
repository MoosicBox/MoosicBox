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

        #[arg(short, long)]
        spread: bool,

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
            offset,
            max,
            chunked,
            spread,
            output,
        } => {
            log::debug!("Loading file '{}'", file);
            let source = std::fs::read_to_string(file)?;
            let value: Value = toml::from_str(&source)?;

            match output {
                OutputType::Json => {
                    if let Some(workspace_members) = value
                        .get("workspace")
                        .and_then(|x| x.get("members"))
                        .and_then(|x| x.as_array())
                        .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
                    {
                        let mut packages = vec![];

                        if output == OutputType::Raw {
                            panic!("workspace Cargo.toml is not supported for raw output");
                        }

                        for file in workspace_members {
                            log::debug!("Loading file '{}'", file);
                            let source = std::fs::read_to_string(format!("{}/Cargo.toml", file))?;
                            let value: Value = toml::from_str(&source)?;

                            if let Some(name) = value
                                .get("package")
                                .and_then(|x| x.get("name"))
                                .and_then(|x| x.as_str())
                                .map(|x| x.to_string())
                            {
                                let features = process_features(
                                    fetch_features(&value, offset, max),
                                    chunked,
                                    spread,
                                );

                                match features {
                                    FeaturesList::Chunked(x) => {
                                        for features in x {
                                            let mut map = serde_json::Map::new();
                                            map.insert(
                                                "path".to_string(),
                                                serde_json::to_value(file).unwrap(),
                                            );
                                            map.insert(
                                                "name".to_string(),
                                                serde_json::to_value(&name).unwrap(),
                                            );
                                            map.insert(
                                                "features".to_string(),
                                                serde_json::to_value(features).unwrap(),
                                            );

                                            packages.push(map);
                                        }
                                    }
                                    FeaturesList::NotChunked(x) => {
                                        let mut map = serde_json::Map::new();
                                        map.insert(
                                            "path".to_string(),
                                            serde_json::to_value(file).unwrap(),
                                        );
                                        map.insert(
                                            "name".to_string(),
                                            serde_json::to_value(name).unwrap(),
                                        );
                                        map.insert(
                                            "features".to_string(),
                                            serde_json::to_value(x).unwrap(),
                                        );

                                        packages.push(map);
                                    }
                                }
                            }
                        }
                        println!("{}", serde_json::to_value(packages).unwrap());
                    } else {
                        let features =
                            process_features(fetch_features(&value, offset, max), chunked, spread);
                        let value: serde_json::Value = features.into();
                        println!("{value}");
                    }
                }
                OutputType::Raw => {
                    let features = fetch_features(&value, offset, max);
                    if chunked.is_some() {
                        panic!("chunked arg is not supported for raw output");
                    }
                    println!("{}", features.join("\n"));
                }
            }
        }
    }

    Ok(())
}

enum FeaturesList {
    Chunked(Vec<Vec<String>>),
    NotChunked(Vec<String>),
}

impl From<FeaturesList> for serde_json::Value {
    fn from(value: FeaturesList) -> Self {
        match value {
            FeaturesList::Chunked(x) => serde_json::to_value(x).unwrap(),
            FeaturesList::NotChunked(x) => serde_json::to_value(x).unwrap(),
        }
    }
}

fn process_features(features: Vec<String>, chunked: Option<u16>, spread: bool) -> FeaturesList {
    if let Some(chunked) = chunked {
        let count = features.len();

        FeaturesList::Chunked(if count <= chunked as usize {
            vec![features]
        } else if spread && count > 1 {
            split(&features, chunked as usize)
                .map(|x| x.to_vec())
                .collect::<Vec<_>>()
        } else {
            features
                .into_iter()
                .chunks(chunked as usize)
                .into_iter()
                .map(|x| x.collect::<Vec<_>>())
                .collect::<Vec<_>>()
        })
    } else {
        FeaturesList::NotChunked(features)
    }
}

fn fetch_features(value: &Value, offset: Option<u16>, max: Option<u16>) -> Vec<String> {
    if let Some(features) = value.get("features") {
        if let Some(features) = features.as_table() {
            let offset = offset.unwrap_or_default().into();
            let feature_count = features.keys().len() - offset;
            features
                .keys()
                .skip(offset)
                .take(
                    max.map(|x| std::cmp::min(feature_count, x as usize))
                        .unwrap_or(feature_count),
                )
                .cloned()
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    } else {
        vec![]
    }
}

pub fn split<T>(slice: &[T], n: usize) -> impl Iterator<Item = &[T]> {
    let len = slice.len() / n;
    let rem = slice.len() % n;
    let len = if rem != 0 { len + 1 } else { len };
    let len = slice.len() / len;
    let rem = slice.len() % len;
    Split { slice, len, rem }
}

struct Split<'a, T> {
    slice: &'a [T],
    len: usize,
    rem: usize,
}

impl<'a, T> Iterator for Split<'a, T> {
    type Item = &'a [T];

    fn next(&mut self) -> Option<Self::Item> {
        if self.slice.is_empty() {
            return None;
        }
        let mut len = self.len;
        if self.rem > 0 {
            len += 1;
            self.rem -= 1;
        }
        let (chunk, rest) = self.slice.split_at(len);
        self.slice = rest;
        Some(chunk)
    }
}
