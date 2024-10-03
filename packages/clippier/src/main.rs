use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
};

use clap::{Parser, Subcommand, ValueEnum};
use itertools::Itertools;
use serde::Deserialize;
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
            let path = PathBuf::from_str(&file)?;
            let cargo_path = path.join("Cargo.toml");
            log::debug!("Loading file '{:?}'", cargo_path);
            let source = std::fs::read_to_string(&cargo_path)?;
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
                            let path = PathBuf::from_str(file)?;

                            packages.extend(process_configs(&path, offset, max, chunked, spread)?);
                        }
                        println!("{}", serde_json::to_value(packages).unwrap());
                    } else {
                        let value: serde_json::Value = serde_json::to_value(process_configs(
                            &path, offset, max, chunked, spread,
                        )?)?;
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

fn process_configs(
    path: &Path,
    offset: Option<u16>,
    max: Option<u16>,
    chunked: Option<u16>,
    spread: bool,
) -> Result<Vec<serde_json::Map<String, serde_json::Value>>, Box<dyn std::error::Error>> {
    log::debug!("Loading file '{:?}'", path);
    let cargo_path = path.join("Cargo.toml");
    let source = std::fs::read_to_string(cargo_path)?;
    let value: Value = toml::from_str(&source)?;

    let conf_path = path.join("clippier.toml");
    let conf = if conf_path.is_file() {
        let source = std::fs::read_to_string(conf_path)?;
        let value: ClippierConf = toml::from_str(&source)?;
        Some(value)
    } else {
        None
    };

    log::debug!("{path:?} conf={conf:?}");

    let configs = if let Some(config) = conf.as_ref().map(|x| x.config.clone()) {
        config
    } else {
        vec![ClippierConfiguration {
            os: "ubuntu".to_string(),
            dependencies: None,
            env: None,
            cargo: None,
            name: None,
        }]
    };

    let mut packages = vec![];

    if let Some(name) = value
        .get("package")
        .and_then(|x| x.get("name"))
        .and_then(|x| x.as_str())
        .map(|x| x.to_string())
    {
        let features = process_features(fetch_features(&value, offset, max), chunked, spread);

        for config in configs {
            match &features {
                FeaturesList::Chunked(x) => {
                    for features in x {
                        packages.push(create_map(
                            conf.as_ref(),
                            &config,
                            path.to_str().unwrap(),
                            &name,
                            features,
                        )?);
                    }
                }
                FeaturesList::NotChunked(x) => {
                    packages.push(create_map(
                        conf.as_ref(),
                        &config,
                        path.to_str().unwrap(),
                        &name,
                        x,
                    )?);
                }
            }
        }
    }

    Ok(packages)
}

fn create_map(
    conf: Option<&ClippierConf>,
    config: &ClippierConfiguration,
    file: &str,
    name: &str,
    features: &[String],
) -> Result<serde_json::Map<String, serde_json::Value>, Box<dyn std::error::Error>> {
    let mut map = serde_json::Map::new();
    map.insert("os".to_string(), serde_json::to_value(&config.os)?);
    map.insert("path".to_string(), serde_json::to_value(file)?);
    map.insert(
        "name".to_string(),
        serde_json::to_value(config.name.as_deref().unwrap_or(name))?,
    );
    map.insert("features".to_string(), features.into());

    if let Some(dependencies) = &config.dependencies {
        let matches = dependencies
            .iter()
            .filter(|x| {
                !x.features.as_ref().is_some_and(|f| {
                    !f.iter()
                        .any(|required| features.iter().any(|x| x == required))
                })
            })
            .collect::<Vec<_>>();

        if !matches.is_empty() {
            map.insert(
                "dependencies".to_string(),
                serde_json::to_value(
                    matches
                        .iter()
                        .map(|x| x.command.as_str())
                        .collect::<Vec<_>>()
                        .join("\n"),
                )?,
            );
        }
    }

    let mut env = conf
        .and_then(|x| x.env.as_ref())
        .cloned()
        .unwrap_or_default();
    env.extend(config.env.clone().unwrap_or_default());

    let matches = env
        .iter()
        .filter(|(_k, v)| match v {
            ClippierEnv::Value(..) => true,
            ClippierEnv::FilteredValue { features: f, .. } => !f.as_ref().is_some_and(|f| {
                !f.iter()
                    .any(|required| features.iter().any(|x| x == required))
            }),
        })
        .map(|(k, v)| {
            (
                k,
                match v {
                    ClippierEnv::Value(value) => value,
                    ClippierEnv::FilteredValue { value, .. } => value,
                },
            )
        })
        .collect::<Vec<_>>();

    if !matches.is_empty() {
        map.insert(
            "env".to_string(),
            serde_json::to_value(
                matches
                    .iter()
                    .map(|(k, v)| serde_json::to_value(v).map(|v| format!("{k}={v}")))
                    .collect::<Result<Vec<_>, _>>()?
                    .join("\n"),
            )?,
        );
    }

    let mut cargo: Vec<_> = conf
        .and_then(|x| x.cargo.as_ref())
        .cloned()
        .unwrap_or_default()
        .into();
    let config_cargo: Vec<_> = config.cargo.clone().unwrap_or_default().into();
    cargo.extend(config_cargo);

    if !cargo.is_empty() {
        map.insert("cargo".to_string(), serde_json::to_value(cargo.join(" "))?);
    }

    Ok(map)
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

#[derive(Debug, Clone, Deserialize)]
pub struct ClippierDependency {
    command: String,
    features: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ClippierEnv {
    Value(String),
    FilteredValue {
        value: String,
        features: Option<Vec<String>>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum VecOrItem<T> {
    Value(T),
    Values(Vec<T>),
}

impl<T> From<VecOrItem<T>> for Vec<T> {
    fn from(value: VecOrItem<T>) -> Self {
        match value {
            VecOrItem::Value(x) => vec![x],
            VecOrItem::Values(x) => x,
        }
    }
}

impl<T> Default for VecOrItem<T> {
    fn default() -> Self {
        Self::Values(vec![])
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClippierConfiguration {
    cargo: Option<VecOrItem<String>>,
    env: Option<HashMap<String, ClippierEnv>>,
    dependencies: Option<Vec<ClippierDependency>>,
    os: String,
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClippierConf {
    cargo: Option<VecOrItem<String>>,
    config: Vec<ClippierConfiguration>,
    env: Option<HashMap<String, ClippierEnv>>,
}
