//! File-based configuration loading for `MoosicBox`.
//!
//! This module provides functionality for loading configuration from JSON5 files,
//! supporting both global (application-wide) and profile-specific settings.
//!
//! Configuration files are loaded from the directory structure created by the root
//! module's path functions. Files can be in either `.json5` or `.json` format,
//! with `.json5` preferred.
//!
//! # Configuration Hierarchy
//!
//! * **Global Config** (`~/.local/moosicbox/{app}/config.json5`) - Application-wide settings
//! * **Profile Config** (`~/.local/moosicbox/{app}/profiles/{name}/config.json5`) - Per-profile settings
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(feature = "file")]
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! use moosicbox_config::{AppType, file::{load_global_config, load_merged_config}};
//!
//! // Load global configuration
//! let global = load_global_config(AppType::Server)?;
//! println!("Default profile: {:?}", global.default_profile);
//!
//! // Load merged configuration (global + profile-specific)
//! let merged = load_merged_config(AppType::Server, "production")?;
//! println!("Library paths: {:?}", merged.profile.library_paths);
//! # Ok(())
//! # }
//! ```

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_with::skip_serializing_none;
use thiserror::Error;

use crate::AppType;

/// Errors that can occur when loading configuration files.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Failed to read the configuration file from disk
    #[error("Failed to read config file: {0}")]
    ReadError(#[from] std::io::Error),
    /// Failed to parse the configuration file as valid JSON5
    #[error("Failed to parse config file: {0}")]
    ParseError(#[from] json5::Error),
    /// Configuration directory could not be found or determined
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

/// Server network configuration.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    /// Server host address
    pub host: Option<String>,
    /// Server port number
    pub port: Option<u16>,
}

/// Backup and data retention configuration.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupConfig {
    /// Whether backups are enabled
    pub enabled: Option<bool>,
    /// Cron-style schedule string for automated backups
    pub schedule: Option<String>,
    /// Number of days to retain backup data
    pub retention_days: Option<u32>,
}

/// Logging configuration.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoggingConfig {
    /// Log level (e.g., "debug", "info", "warn", "error")
    pub level: Option<String>,
    /// Path to log file
    pub file: Option<String>,
}

/// Feature flag configuration for enabling or disabling functionality.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FeatureFlags {
    /// Enable experimental features (unstable or in-development functionality)
    pub experimental: Option<bool>,
}

/// Credentials for external music streaming services.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceCredentials {
    /// Tidal service credentials
    pub tidal: Option<TidalCredentials>,
    /// Qobuz service credentials
    pub qobuz: Option<QobuzCredentials>,
}

/// Authentication credentials for Tidal streaming service.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalCredentials {
    /// OAuth access token for API authentication
    pub access_token: String,
    /// OAuth refresh token for renewing access tokens
    pub refresh_token: Option<String>,
}

/// Authentication credentials for Qobuz streaming service.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QobuzCredentials {
    /// Qobuz application ID
    pub app_id: String,
    /// User authentication token
    pub user_auth_token: String,
}

/// Audio playback behavior configuration.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackConfig {
    /// Enable gapless playback (seamless transitions between tracks)
    pub gapless: Option<bool>,
    /// Duration of crossfade between tracks in seconds
    pub crossfade_duration: Option<f32>,
}

/// Audio quality and format preferences.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioQualityConfig {
    /// Preferred audio format (e.g., "FLAC", "MP3", "AAC")
    pub preferred_format: Option<String>,
    /// Bit depth in bits (e.g., 16, 24)
    pub bit_depth: Option<u8>,
    /// Sample rate in Hz (e.g., 44100, 48000, 96000)
    pub sample_rate: Option<u32>,
}

/// Audio player configuration.
#[skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerConfig {
    /// Default volume level (0.0 to 1.0)
    pub volume: Option<f32>,
    /// Audio buffer size in bytes
    pub buffer_size: Option<u32>,
}

/// Get the path to a config file, preferring `.json5` but also checking `.json`.
///
/// This function checks for a config file with the given filename in the provided directory,
/// preferring `.json5` format over `.json` format if both exist.
///
/// Returns `None` if neither file exists.
#[must_use]
fn get_config_file_path(dir: &Path, filename: &str) -> Option<PathBuf> {
    let json5_path = dir.join(format!("{filename}.json5"));
    if switchy_fs::exists(&json5_path) {
        return Some(json5_path);
    }

    let json_path = dir.join(format!("{filename}.json"));
    if switchy_fs::exists(&json_path) {
        return Some(json_path);
    }

    None
}

/// Load a config file from disk, parsing it with `json5`.
///
/// This function reads the file at the given path and parses it as JSON5 format,
/// deserializing it into the specified type.
///
/// # Errors
///
/// * If the file cannot be read
/// * If the file content is not valid JSON5
/// * If the JSON5 content cannot be deserialized into type `T`
fn load_config_file<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ConfigError> {
    let content = switchy_fs::sync::read_to_string(path)?;
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
    /// Global configuration that applies to all profiles
    pub global: GlobalConfig,
    /// Profile-specific configuration settings
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
    use serial_test::serial;
    use switchy_fs::sync;

    #[test_log::test]
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

    #[test_log::test]
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

    // Tests for get_config_file_path()
    #[test_log::test]
    fn test_get_config_file_path_prefers_json5() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        // Create both .json5 and .json files
        let json5_path = temp_path.join("config.json5");
        let json_path = temp_path.join("config.json");
        sync::write(&json5_path, "{}").unwrap();
        sync::write(&json_path, "{}").unwrap();

        let result = get_config_file_path(temp_path, "config");
        assert_eq!(result, Some(json5_path));
    }

    #[test_log::test]
    fn test_get_config_file_path_falls_back_to_json() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        // Create only .json file
        let json_path = temp_path.join("config.json");
        sync::write(&json_path, "{}").unwrap();

        let result = get_config_file_path(temp_path, "config");
        assert_eq!(result, Some(json_path));
    }

    #[test_log::test]
    fn test_get_config_file_path_returns_none_when_missing() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let result = get_config_file_path(temp_path, "config");
        assert_eq!(result, None);
    }

    // Tests for load_config_file()
    #[test_log::test]
    fn test_load_config_file_success() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let config_path = temp_path.join("config.json5");
        sync::write(
            &config_path,
            r#"{
            // Test global config
            defaultProfile: "test",
        }"#,
        )
        .unwrap();

        let result: Result<GlobalConfig, ConfigError> = load_config_file(&config_path);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().default_profile, Some("test".to_string()));
    }

    #[test_log::test]
    fn test_load_config_file_read_error() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let config_path = temp_path.join("nonexistent.json5");

        let result: Result<GlobalConfig, ConfigError> = load_config_file(&config_path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ReadError(_)));
    }

    #[test_log::test]
    fn test_load_config_file_parse_error() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();

        let config_path = temp_path.join("malformed.json5");
        sync::write(&config_path, "{ invalid json5: ").unwrap();

        let result: Result<GlobalConfig, ConfigError> = load_config_file(&config_path);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }

    // Tests for load_global_config()
    // Tests that modify ROOT_DIR must run serially to avoid interference
    #[test_log::test]
    #[serial]
    fn test_load_global_config_with_json5_file() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();
        let app_dir = temp_path.join("server");
        sync::create_dir_all(&app_dir).unwrap();

        let config_path = app_dir.join("config.json5");
        sync::write(
            &config_path,
            r#"{
            server: {
                host: "127.0.0.1",
                port: 9090,
            },
            defaultProfile: "production",
        }"#,
        )
        .unwrap();

        // Set root directory to temp directory
        crate::set_root_dir(temp_path.to_path_buf());
        let result = load_global_config(AppType::Server);
        // Reset root directory
        crate::set_root_dir(home::home_dir().unwrap().join(".local").join("moosicbox"));

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.default_profile, Some("production".to_string()));
        assert_eq!(config.server.as_ref().unwrap().port, Some(9090));
    }

    #[test_log::test]
    #[serial]
    fn test_load_global_config_with_json_file() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();
        let app_dir = temp_path.join("server");
        sync::create_dir_all(&app_dir).unwrap();

        let config_path = app_dir.join("config.json");
        sync::write(
            &config_path,
            r#"{
            "defaultProfile": "staging"
        }"#,
        )
        .unwrap();

        crate::set_root_dir(temp_path.to_path_buf());
        let result = load_global_config(AppType::Server);
        crate::set_root_dir(home::home_dir().unwrap().join(".local").join("moosicbox"));

        assert!(result.is_ok());
        assert_eq!(result.unwrap().default_profile, Some("staging".to_string()));
    }

    #[test_log::test]
    #[serial]
    fn test_load_global_config_returns_default_when_file_missing() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();
        let app_dir = temp_path.join("server");
        sync::create_dir_all(&app_dir).unwrap();

        crate::set_root_dir(temp_path.to_path_buf());
        let result = load_global_config(AppType::Server);
        crate::set_root_dir(home::home_dir().unwrap().join(".local").join("moosicbox"));

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.default_profile, None);
        assert_eq!(config.server, None);
    }

    #[test_log::test]
    #[serial]
    fn test_load_global_config_parse_error() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();
        let app_dir = temp_path.join("server");
        sync::create_dir_all(&app_dir).unwrap();

        let config_path = app_dir.join("config.json5");
        sync::write(&config_path, "{ malformed: ").unwrap();

        crate::set_root_dir(temp_path.to_path_buf());
        let result = load_global_config(AppType::Server);
        crate::set_root_dir(home::home_dir().unwrap().join(".local").join("moosicbox"));

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }

    // Tests for load_profile_config()
    #[test_log::test]
    #[serial]
    fn test_load_profile_config_with_json5_file() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();
        let profiles_dir = temp_path
            .join("server")
            .join("profiles")
            .join("test_profile");
        sync::create_dir_all(&profiles_dir).unwrap();

        let config_path = profiles_dir.join("config.json5");
        sync::write(
            &config_path,
            r#"{
            libraryPaths: ["/music"],
            playback: {
                gapless: true,
            },
        }"#,
        )
        .unwrap();

        crate::set_root_dir(temp_path.to_path_buf());
        let result = load_profile_config(AppType::Server, "test_profile");
        crate::set_root_dir(home::home_dir().unwrap().join(".local").join("moosicbox"));

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.library_paths.as_ref().unwrap().len(), 1);
        assert_eq!(config.playback.as_ref().unwrap().gapless, Some(true));
    }

    #[test_log::test]
    #[serial]
    fn test_load_profile_config_returns_default_when_file_missing() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();
        let profiles_dir = temp_path
            .join("server")
            .join("profiles")
            .join("empty_profile");
        sync::create_dir_all(&profiles_dir).unwrap();

        crate::set_root_dir(temp_path.to_path_buf());
        let result = load_profile_config(AppType::Server, "empty_profile");
        crate::set_root_dir(home::home_dir().unwrap().join(".local").join("moosicbox"));

        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.library_paths, None);
        assert_eq!(config.playback, None);
    }

    #[test_log::test]
    #[serial]
    fn test_load_profile_config_parse_error() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();
        let profiles_dir = temp_path
            .join("server")
            .join("profiles")
            .join("bad_profile");
        sync::create_dir_all(&profiles_dir).unwrap();

        let config_path = profiles_dir.join("config.json5");
        sync::write(&config_path, "{ invalid: ").unwrap();

        crate::set_root_dir(temp_path.to_path_buf());
        let result = load_profile_config(AppType::Server, "bad_profile");
        crate::set_root_dir(home::home_dir().unwrap().join(".local").join("moosicbox"));

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }

    // Tests for load_merged_config()
    #[test_log::test]
    #[serial]
    fn test_load_merged_config_combines_global_and_profile() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();
        let app_dir = temp_path.join("server");
        let profiles_dir = app_dir.join("profiles").join("merged_profile");
        sync::create_dir_all(&profiles_dir).unwrap();

        // Write global config
        let global_config_path = app_dir.join("config.json5");
        sync::write(
            &global_config_path,
            r#"{
            server: {
                host: "0.0.0.0",
                port: 8080,
            },
            defaultProfile: "merged_profile",
        }"#,
        )
        .unwrap();

        // Write profile config
        let profile_config_path = profiles_dir.join("config.json5");
        sync::write(
            &profile_config_path,
            r#"{
            libraryPaths: ["/music/flac", "/music/mp3"],
            playback: {
                gapless: true,
                crossfadeDuration: 3.0,
            },
        }"#,
        )
        .unwrap();

        crate::set_root_dir(temp_path.to_path_buf());
        let result = load_merged_config(AppType::Server, "merged_profile");
        crate::set_root_dir(home::home_dir().unwrap().join(".local").join("moosicbox"));

        assert!(result.is_ok());
        let merged = result.unwrap();

        // Verify global config
        assert_eq!(
            merged.global.default_profile,
            Some("merged_profile".to_string())
        );
        assert_eq!(merged.global.server.as_ref().unwrap().port, Some(8080));

        // Verify profile config
        assert_eq!(merged.profile.library_paths.as_ref().unwrap().len(), 2);
        assert_eq!(
            merged.profile.playback.as_ref().unwrap().gapless,
            Some(true)
        );
        assert_eq!(
            merged.profile.playback.as_ref().unwrap().crossfade_duration,
            Some(3.0)
        );
    }

    #[test_log::test]
    #[serial]
    fn test_load_merged_config_with_missing_files() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();
        let app_dir = temp_path.join("server");
        let profiles_dir = app_dir.join("profiles").join("sparse_profile");
        sync::create_dir_all(&profiles_dir).unwrap();

        crate::set_root_dir(temp_path.to_path_buf());
        let result = load_merged_config(AppType::Server, "sparse_profile");
        crate::set_root_dir(home::home_dir().unwrap().join(".local").join("moosicbox"));

        assert!(result.is_ok());
        let merged = result.unwrap();

        // Both should be defaults
        assert_eq!(merged.global.default_profile, None);
        assert_eq!(merged.profile.library_paths, None);
    }

    #[test_log::test]
    #[serial]
    fn test_load_merged_config_global_parse_error_propagates() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();
        let app_dir = temp_path.join("server");
        let profiles_dir = app_dir.join("profiles").join("test_profile");
        sync::create_dir_all(&profiles_dir).unwrap();

        // Write malformed global config
        let global_config_path = app_dir.join("config.json5");
        sync::write(&global_config_path, "{ bad: ").unwrap();

        crate::set_root_dir(temp_path.to_path_buf());
        let result = load_merged_config(AppType::Server, "test_profile");
        crate::set_root_dir(home::home_dir().unwrap().join(".local").join("moosicbox"));

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }

    #[test_log::test]
    #[serial]
    fn test_load_merged_config_profile_parse_error_propagates() {
        let temp_dir = switchy_fs::tempdir().unwrap();
        let temp_path = temp_dir.path();
        let app_dir = temp_path.join("server");
        let profiles_dir = app_dir.join("profiles").join("test_profile");
        sync::create_dir_all(&profiles_dir).unwrap();

        // Write valid global config
        let global_config_path = app_dir.join("config.json5");
        sync::write(&global_config_path, "{ defaultProfile: \"test\" }").unwrap();

        // Write malformed profile config
        let profile_config_path = profiles_dir.join("config.json5");
        sync::write(&profile_config_path, "{ bad: ").unwrap();

        crate::set_root_dir(temp_path.to_path_buf());
        let result = load_merged_config(AppType::Server, "test_profile");
        crate::set_root_dir(home::home_dir().unwrap().join(".local").join("moosicbox"));

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }
}
