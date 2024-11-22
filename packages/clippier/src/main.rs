#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

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

        #[arg(long)]
        features: Option<String>,

        #[arg(long)]
        skip_features: Option<String>,

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
            features: specific_features,
            skip_features,
            output,
        } => {
            let path = PathBuf::from_str(&file)?;
            let cargo_path = path.join("Cargo.toml");
            log::debug!("Loading file '{:?}'", cargo_path);
            let source = std::fs::read_to_string(&cargo_path)?;
            let value: Value = toml::from_str(&source)?;

            let specific_features =
                specific_features.map(|x| x.split(',').map(str::to_string).collect_vec());

            let skip_features =
                skip_features.map(|x| x.split(',').map(str::to_string).collect_vec());

            match output {
                OutputType::Json => {
                    if let Some(workspace_members) = value
                        .get("workspace")
                        .and_then(|x| x.get("members"))
                        .and_then(|x| x.as_array())
                        .and_then(|x| x.iter().map(|x| x.as_str()).collect::<Option<Vec<_>>>())
                    {
                        let mut packages = vec![];

                        assert!(
                            output != OutputType::Raw,
                            "workspace Cargo.toml is not supported for raw output"
                        );

                        for file in workspace_members {
                            let path = PathBuf::from_str(file)?;

                            packages.extend(process_configs(
                                &path,
                                offset,
                                max,
                                chunked,
                                spread,
                                specific_features.as_deref(),
                            )?);
                        }
                        println!("{}", serde_json::to_value(packages).unwrap());
                    } else {
                        let value: serde_json::Value = serde_json::to_value(process_configs(
                            &path,
                            offset,
                            max,
                            chunked,
                            spread,
                            specific_features.as_deref(),
                        )?)?;
                        println!("{value}");
                    }
                }
                OutputType::Raw => {
                    let features = fetch_features(
                        &value,
                        offset,
                        max,
                        specific_features.as_deref(),
                        skip_features.as_deref(),
                    );
                    assert!(
                        chunked.is_none(),
                        "chunked arg is not supported for raw output"
                    );
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
    specific_features: Option<&[String]>,
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

    let configs = conf.as_ref().map(|x| x.config.clone()).map_or_else(
        || {
            vec![ClippierConfiguration {
                os: "ubuntu".to_string(),
                dependencies: None,
                env: None,
                cargo: None,
                name: None,
                ci_steps: None,
                skip_features: None,
            }]
        },
        |config| config,
    );

    let mut packages = vec![];

    if let Some(name) = value
        .get("package")
        .and_then(|x| x.get("name"))
        .and_then(|x| x.as_str())
        .map(str::to_string)
    {
        for config in configs {
            let features = fetch_features(
                &value,
                offset,
                max,
                specific_features,
                config.skip_features.as_deref(),
            );
            let features = process_features(
                features,
                conf.as_ref()
                    .and_then(|x| x.parallelization.as_ref().map(|x| x.chunked))
                    .or(chunked),
                spread,
            );
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

#[allow(clippy::too_many_lines)]
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
                    ClippierEnv::Value(value) | ClippierEnv::FilteredValue { value, .. } => value,
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

    let mut ci_steps: Vec<_> = conf
        .and_then(|x| x.ci_steps.as_ref())
        .cloned()
        .unwrap_or_default()
        .into();
    let config_ci_steps: Vec<_> = config.ci_steps.clone().unwrap_or_default().into();
    ci_steps.extend(config_ci_steps);

    let ci_steps = ci_steps
        .into_iter()
        .filter(|x| {
            !x.features.as_ref().is_some_and(|f| {
                !f.iter()
                    .any(|required| features.iter().any(|x| x == required))
            })
        })
        .collect_vec();

    if !ci_steps.is_empty() {
        map.insert(
            "ciSteps".to_string(),
            serde_json::to_value(
                ci_steps
                    .iter()
                    .map(|x| x.command.as_str())
                    .collect_vec()
                    .join("\n"),
            )?,
        );
    }

    Ok(map)
}

enum FeaturesList {
    Chunked(Vec<Vec<String>>),
    NotChunked(Vec<String>),
}

impl TryFrom<FeaturesList> for serde_json::Value {
    type Error = serde_json::Error;

    fn try_from(value: FeaturesList) -> Result<Self, Self::Error> {
        Ok(match value {
            FeaturesList::Chunked(x) => serde_json::to_value(x)?,
            FeaturesList::NotChunked(x) => serde_json::to_value(x)?,
        })
    }
}

fn process_features(features: Vec<String>, chunked: Option<u16>, spread: bool) -> FeaturesList {
    if let Some(chunked) = chunked {
        let count = features.len();

        FeaturesList::Chunked(if count <= chunked as usize {
            vec![features]
        } else if spread && count > 1 {
            split(&features, chunked as usize)
                .map(<[String]>::to_vec)
                .collect::<Vec<_>>()
        } else {
            features
                .into_iter()
                .chunks(chunked as usize)
                .into_iter()
                .map(Iterator::collect)
                .collect::<Vec<_>>()
        })
    } else {
        FeaturesList::NotChunked(features)
    }
}

fn fetch_features(
    value: &Value,
    offset: Option<u16>,
    max: Option<u16>,
    specific_features: Option<&[String]>,
    skip_features: Option<&[String]>,
) -> Vec<String> {
    value.get("features").map_or_else(Vec::new, |features| {
        features.as_table().map_or_else(Vec::new, |features| {
            let offset = offset.unwrap_or_default().into();
            let feature_count = features.keys().len() - offset;
            features
                .keys()
                .filter(|x| !x.starts_with('_'))
                .filter(|x| !specific_features.as_ref().is_some_and(|s| !s.contains(x)))
                .filter(|x| !skip_features.as_ref().is_some_and(|s| s.contains(x)))
                .skip(offset)
                .take(max.map_or(feature_count, |x| std::cmp::min(feature_count, x as usize)))
                .cloned()
                .collect::<Vec<_>>()
        })
    })
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
pub struct CommandFilteredByFeatures {
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
#[serde(rename_all = "kebab-case")]
pub struct ClippierConfiguration {
    ci_steps: Option<VecOrItem<CommandFilteredByFeatures>>,
    cargo: Option<VecOrItem<String>>,
    env: Option<HashMap<String, ClippierEnv>>,
    dependencies: Option<Vec<CommandFilteredByFeatures>>,
    os: String,
    skip_features: Option<Vec<String>>,
    name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ParallelizationConfig {
    chunked: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ClippierConf {
    ci_steps: Option<VecOrItem<CommandFilteredByFeatures>>,
    cargo: Option<VecOrItem<String>>,
    config: Vec<ClippierConfiguration>,
    env: Option<HashMap<String, ClippierEnv>>,
    parallelization: Option<ParallelizationConfig>,
}
