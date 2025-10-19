use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use thiserror::Error;

use crate::AppType;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse config file: {0}")]
    ParseError(#[from] json5::Error),
    #[error("Config directory not found")]
    ConfigDirNotFound,
}

/// Global configuration that applies to all profiles
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GlobalConfig {
    /// Server settings (host, port, etc.)
    pub server: Option<ServerConfig>,

    /// Backup configuration
    pub backup: Option<BackupConfig>,

    /// Logging configuration
    pub logging: Option<LoggingConfig>,

    /// Feature flags
    pub features: Option<FeatureFlags>,

    /// Default profile selection
    pub default_profile: Option<String>,
}

/// Profile-specific configuration
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProfileConfig {
    /// Music library paths
    pub library_paths: Option<Vec<String>>,

    /// Streaming service credentials
    pub services: Option<ServiceCredentials>,

    /// Playback preferences
    pub playback: Option<PlaybackConfig>,

    /// Audio quality settings
    pub audio_quality: Option<AudioQualityConfig>,

    /// Player-specific settings
    pub player: Option<PlayerConfig>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    pub host: Option<String>,
    pub port: Option<u16>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupConfig {
    pub enabled: Option<bool>,
    pub schedule: Option<String>,
    pub retention_days: Option<u32>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingConfig {
    pub level: Option<String>,
    pub file: Option<String>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FeatureFlags {
    pub experimental: Option<bool>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceCredentials {
    pub tidal: Option<TidalCredentials>,
    pub qobuz: Option<QobuzCredentials>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalCredentials {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzCredentials {
    pub app_id: String,
    pub user_auth_token: String,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackConfig {
    pub gapless: Option<bool>,
    pub crossfade_duration: Option<f32>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioQualityConfig {
    pub preferred_format: Option<String>,
    pub bit_depth: Option<u8>,
    pub sample_rate: Option<u32>,
}

#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerConfig {
    pub volume: Option<f32>,
    pub buffer_size: Option<u32>,
}

/// Get the path to a config file, preferring .json5 but also checking .json
fn get_config_file_path(dir: &Path, filename: &str) -> Option<PathBuf> {
    let json5_path = dir.join(format!("{filename}.json5"));
    if json5_path.exists() {
        return Some(json5_path);
    }

    let json_path = dir.join(format!("{filename}.json"));
    if json_path.exists() {
        return Some(json_path);
    }

    None
}

/// Load a config file from disk, parsing it with json5
fn load_config_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ConfigError> {
    let content = fs::read_to_string(path)?;
    let config = json5::from_str(&content)?;
    Ok(config)
}

/// Load global configuration from the config directory
///
/// # Errors
///
/// * If the config directory cannot be found
/// * If the config file cannot be read
/// * If the config file is malformed
pub fn load_global_config(app_type: AppType) -> Result<GlobalConfig, ConfigError> {
    let config_dir =
        crate::get_app_config_dir_path(app_type).ok_or(ConfigError::ConfigDirNotFound)?;

    get_config_file_path(&config_dir, "config").map_or_else(
        || Ok(GlobalConfig::default()),
        |path| load_config_file(&path),
    )
}

/// Load profile-specific configuration
///
/// # Errors
///
/// * If the profile directory cannot be found
/// * If the config file cannot be read
/// * If the config file is malformed
pub fn load_profile_config(app_type: AppType, profile: &str) -> Result<ProfileConfig, ConfigError> {
    let profile_dir =
        crate::get_profile_dir_path(app_type, profile).ok_or(ConfigError::ConfigDirNotFound)?;

    get_config_file_path(&profile_dir, "config").map_or_else(
        || Ok(ProfileConfig::default()),
        |path| load_config_file(&path),
    )
}

/// Merged configuration combining global and profile-specific settings
#[derive(Debug, Clone)]
pub struct MergedConfig {
    pub global: GlobalConfig,
    pub profile: ProfileConfig,
}

/// Load merged configuration for a specific profile
///
/// This loads both global and profile-specific configurations.
/// Profile-specific settings take precedence over global settings when merging.
///
/// # Errors
///
/// * If the config directories cannot be found
/// * If the config files cannot be read
/// * If the config files are malformed
pub fn load_merged_config(app_type: AppType, profile: &str) -> Result<MergedConfig, ConfigError> {
    let global = load_global_config(app_type)?;
    let profile = load_profile_config(app_type, profile)?;

    Ok(MergedConfig { global, profile })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_global_config_json5() {
        let json5_content = r#"{
            // Global server configuration
            server: {
                host: "0.0.0.0",
                port: 8080,
            },
            backup: {
                enabled: true,
                schedule: "0 0 * * *", // Daily at midnight
                retentionDays: 30,
            },
            logging: {
                level: "info",
            },
            features: {
                experimental: false,
            },
            defaultProfile: "default",
        }"#;

        let config: GlobalConfig = json5::from_str(json5_content).unwrap();
        assert_eq!(config.default_profile, Some("default".to_string()));
        assert_eq!(config.server.as_ref().unwrap().port, Some(8080));
        assert_eq!(config.backup.as_ref().unwrap().enabled, Some(true));
    }

    #[test]
    fn test_parse_profile_config_json5() {
        let json5_content = r#"{
            // Profile-specific configuration
            libraryPaths: [
                "/music/library1",
                "/music/library2",
            ],
            services: {
                tidal: {
                    accessToken: "token123",
                    refreshToken: "refresh456",
                },
            },
            playback: {
                gapless: true,
                crossfadeDuration: 2.5,
            },
            audioQuality: {
                preferredFormat: "FLAC",
                bitDepth: 24,
                sampleRate: 96000,
            },
        }"#;

        let config: ProfileConfig = json5::from_str(json5_content).unwrap();
        assert_eq!(config.library_paths.as_ref().unwrap().len(), 2);
        assert_eq!(config.playback.as_ref().unwrap().gapless, Some(true));
        assert_eq!(config.audio_quality.as_ref().unwrap().bit_depth, Some(24));
    }
}
