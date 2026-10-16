//! Layered, allowlisted preferences shared by publishing and configuration reporting.
//!
//! Precedence is explicit overrides, environment, workspace, user, then built-in defaults.
//! Only the `publish` namespace is interpreted; other existing configuration is ignored.

use std::{
    error::Error,
    ffi::OsString,
    fmt, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

/// Policy for publishing packages with source changes outside their release inputs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum, Serialize, Deserialize)]
pub enum PurityPolicy {
    /// Report impurity without blocking publication.
    #[default]
    #[serde(rename = "warn")]
    Warn,
    /// Refuse to publish impure packages.
    #[serde(rename = "require-pure")]
    RequirePure,
}

impl fmt::Display for PurityPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Warn => "warn",
            Self::RequirePure => "require-pure",
        })
    }
}

impl FromStr for PurityPolicy {
    type Err = String;

    /// Parse a canonical policy name.
    ///
    /// # Errors
    ///
    /// * Returns an error for values other than `warn` and `require-pure`.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "warn" => Ok(Self::Warn),
            "require-pure" => Ok(Self::RequirePure),
            _ => Err(format!(
                "invalid publish purity {value:?}; expected warn or require-pure"
            )),
        }
    }
}

/// Highest-priority preferences supplied by a caller, usually explicit CLI options.
#[derive(Debug, Clone, Copy, Default)]
pub struct PreferencesOverrides {
    /// Leave unset to inherit the next configured layer.
    pub purity: Option<PurityPolicy>,
    /// Override whether real publishes validate the entire queue before uploading.
    pub preflight: Option<bool>,
}

/// Resolved publishing preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublishPreferences {
    /// Effective publishing purity policy.
    pub purity: PurityPolicy,
    /// Validate the entire queue before real uploads (dry runs always validate).
    pub preflight: bool,
}

/// Origin of an effective setting; file origins retain their diagnostic path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "source", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreferenceSource {
    /// Built-in compatibility default.
    BuiltIn,
    /// Per-user allowlisted preferences.
    UserConfig {
        /// File from which the setting was read.
        path: PathBuf,
    },
    /// Workspace-local preferences.
    WorkspaceConfig {
        /// File from which the setting was read.
        path: PathBuf,
    },
    /// Runtime environment override.
    Environment {
        /// Name of the environment variable.
        variable: String,
    },
    /// Explicit API or CLI override.
    Override,
}

impl fmt::Display for PreferenceSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BuiltIn => f.write_str("built-in"),
            Self::UserConfig { path } => write!(f, "user config {}", path.display()),
            Self::WorkspaceConfig { path } => write!(f, "workspace config {}", path.display()),
            Self::Environment { variable } => write!(f, "environment {variable}"),
            Self::Override => f.write_str("explicit override"),
        }
    }
}

/// Per-setting provenance for publishing preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PublishProvenance {
    /// Origin of `publish.purity`.
    pub purity: PreferenceSource,
    /// Origin of `publish.preflight`.
    pub preflight: PreferenceSource,
}

/// Per-namespace provenance for effective preferences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreferencesProvenance {
    /// Publishing setting origins.
    pub publish: PublishProvenance,
}

/// Effective preferences and the origin of every resolved setting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectivePreferences {
    /// Resolved publishing preferences.
    pub publish: PublishPreferences,
    /// Per-setting provenance, separate from the setting values.
    pub provenance: PreferencesProvenance,
}

impl EffectivePreferences {
    /// Render a human-readable configuration report including provenance.
    #[must_use]
    pub fn to_raw_string(&self) -> String {
        format!(
            "publish.purity = {} ({})\npublish.preflight = {} ({})\n",
            self.publish.purity,
            self.provenance.publish.purity,
            self.publish.preflight,
            self.provenance.publish.preflight
        )
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublishLayer {
    purity: Option<PurityPolicy>,
    preflight: Option<bool>,
}

#[derive(Default)]
struct EnvironmentInputs {
    xdg_config_home: Option<OsString>,
    home: Option<OsString>,
    purity: Option<OsString>,
    preflight: Option<OsString>,
}

impl EnvironmentInputs {
    fn capture() -> Self {
        Self {
            xdg_config_home: std::env::var_os("XDG_CONFIG_HOME"),
            home: std::env::var_os("HOME"),
            purity: std::env::var_os("CLIPPIER_PUBLISH_PURITY"),
            preflight: std::env::var_os("CLIPPIER_PUBLISH_PREFLIGHT"),
        }
    }

    fn user_config_path(&self) -> Option<PathBuf> {
        self.xdg_config_home
            .as_ref()
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                self.home
                    .as_ref()
                    .map(|home| PathBuf::from(home).join(".config"))
            })
            .map(|root| root.join("clippier/clippier.toml"))
    }
}

type BoxError = Box<dyn Error + Send + Sync>;

fn contextual_error(path: &Path, error: impl fmt::Display) -> BoxError {
    std::io::Error::other(format!("{}: {error}", path.display())).into()
}

fn read_optional(path: &Path) -> Result<Option<String>, BoxError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(contextual_error(path, error)),
    }
}

fn read_layer(path: &Path) -> Result<PublishLayer, BoxError> {
    let Some(text) = read_optional(path)? else {
        return Ok(PublishLayer::default());
    };
    let document: toml::Value =
        toml::from_str(&text).map_err(|error| contextual_error(path, error))?;
    document.get("publish").map_or_else(
        || Ok(PublishLayer::default()),
        |publish| {
            publish
                .clone()
                .try_into()
                .map_err(|error| contextual_error(path, error))
        },
    )
}

// Discovery deliberately starts at the caller's path, never at the process cwd.
fn workspace_config_path(path: &Path) -> Result<PathBuf, BoxError> {
    let start = crate::cargo_workspace::normalize_workspace_root(path);
    let start =
        fs::canonicalize(start.join(".")).map_err(|error| contextual_error(&start, error))?;
    for directory in start.ancestors() {
        let manifest = directory.join("Cargo.toml");
        let Some(text) = read_optional(&manifest)? else {
            continue;
        };
        let document: toml::Value =
            toml::from_str(&text).map_err(|error| contextual_error(&manifest, error))?;
        if document.get("workspace").is_some() {
            return Ok(directory.join("clippier.toml"));
        }
        // Cargo owns membership semantics, including exclusions and explicit workspace links.
        // locate-project only discovers manifests; it does not resolve dependencies or use the network.
        let output = std::process::Command::new("cargo")
            .args(["locate-project", "--workspace", "--message-format", "plain"])
            .arg("--manifest-path")
            .arg(&manifest)
            .current_dir(directory)
            .output()
            .map_err(|error| contextual_error(&manifest, error))?;
        if !output.status.success() {
            return Err(contextual_error(
                &manifest,
                format!(
                    "cargo locate-project failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            ));
        }
        let located = std::str::from_utf8(&output.stdout)
            .map_err(|error| contextual_error(&manifest, error))?;
        let located = Path::new(located.trim_end_matches(['\r', '\n']));
        let root = located
            .parent()
            .filter(|_| {
                located.is_absolute()
                    && located.file_name().is_some_and(|name| name == "Cargo.toml")
            })
            .ok_or_else(|| {
                contextual_error(
                    &manifest,
                    "cargo locate-project returned an invalid manifest path",
                )
            })?;
        return Ok(root.join("clippier.toml"));
    }
    Ok(start.join("clippier.toml"))
}

/// Resolve shared preferences for a workspace, package, nested directory, or manifest path.
///
/// User files are read from absolute `XDG_CONFIG_HOME`, falling back to `HOME/.config`.
/// Only typed, allowlisted publishing preferences are used from configuration files;
/// unrelated namespaces cannot install commands or modify the process environment.
/// All present layers are validated, including settings shadowed by higher-priority layers.
///
/// # Errors
///
/// * The supplied workspace directory cannot be resolved (includes its path).
/// * A present configuration or discovery manifest is unreadable or malformed (includes its path).
/// * Cargo workspace discovery fails for the nearest package manifest (includes its path).
/// * The `publish` namespace contains unknown keys, wrong types, or an invalid purity policy.
/// * `CLIPPIER_PUBLISH_PURITY` is non-Unicode or is not a canonical purity policy.
pub fn resolve_preferences(
    workspace_root: &Path,
    overrides: PreferencesOverrides,
) -> Result<EffectivePreferences, Box<dyn Error + Send + Sync>> {
    resolve_with_inputs(workspace_root, overrides, &EnvironmentInputs::capture())
}

fn resolve_with_inputs(
    workspace_root: &Path,
    overrides: PreferencesOverrides,
    environment: &EnvironmentInputs,
) -> Result<EffectivePreferences, BoxError> {
    let (mut purity, mut source) = if let Some(path) = environment.user_config_path()
        && let Some(value) = read_layer(&path)?.purity
    {
        (value, PreferenceSource::UserConfig { path })
    } else {
        (PurityPolicy::default(), PreferenceSource::BuiltIn)
    };
    let path = workspace_config_path(workspace_root)?;
    if let Some(value) = read_layer(&path)?.purity {
        purity = value;
        source = PreferenceSource::WorkspaceConfig { path };
    }
    if let Some(value) = &environment.purity {
        let value = value.to_str().ok_or_else(|| {
            std::io::Error::other("CLIPPIER_PUBLISH_PURITY must contain Unicode text")
        })?;
        purity = value
            .parse()
            .map_err(|error| std::io::Error::other(format!("CLIPPIER_PUBLISH_PURITY: {error}")))?;
        source = PreferenceSource::Environment {
            variable: "CLIPPIER_PUBLISH_PURITY".to_owned(),
        };
    }
    if let Some(value) = overrides.purity {
        purity = value;
        source = PreferenceSource::Override;
    }
    let (mut preflight, mut preflight_source) = if let Some(path) = environment.user_config_path()
        && let Some(value) = read_layer(&path)?.preflight
    {
        (value, PreferenceSource::UserConfig { path })
    } else {
        (false, PreferenceSource::BuiltIn)
    };
    let path = workspace_config_path(workspace_root)?;
    if let Some(value) = read_layer(&path)?.preflight {
        preflight = value;
        preflight_source = PreferenceSource::WorkspaceConfig { path };
    }
    if let Some(value) = &environment.preflight {
        preflight = value
            .to_str()
            .and_then(|value| value.parse::<bool>().ok())
            .ok_or_else(|| {
                std::io::Error::other("CLIPPIER_PUBLISH_PREFLIGHT must be true or false")
            })?;
        preflight_source = PreferenceSource::Environment {
            variable: "CLIPPIER_PUBLISH_PREFLIGHT".to_owned(),
        };
    }
    if let Some(value) = overrides.preflight {
        preflight = value;
        preflight_source = PreferenceSource::Override;
    }
    Ok(EffectivePreferences {
        publish: PublishPreferences { purity, preflight },
        provenance: PreferencesProvenance {
            publish: PublishProvenance {
                purity: source,
                preflight: preflight_source,
            },
        },
    })
}

#[cfg(test)]
mod tests;
