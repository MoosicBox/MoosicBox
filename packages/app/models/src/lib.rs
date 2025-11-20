//! Data models for `MoosicBox` application configuration.
//!
//! This crate provides the core data structures used by `MoosicBox` applications
//! to manage connections, music API integrations, and library settings.
//!
//! # Main Types
//!
//! * [`Connection`] - Server connection configuration
//! * [`MusicApiSettings`] - Music API integration settings
//! * [`DownloadSettings`] - Download location management
//! * [`ScanSettings`] - Library scanning configuration

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::branches_sharing_code)]

use serde::{Deserialize, Serialize};

/// Re-exported authentication method type from the music API module.
///
/// Used by [`MusicApiSettings`] to specify the authentication mechanism
/// for external music API integrations.
#[cfg(feature = "music-api-api")]
pub use moosicbox_music_api_api::models::AuthMethod;

/// Represents a connection to a `MoosicBox` server.
///
/// Contains the server name and API URL for establishing connections.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Connection {
    /// Display name for the connection
    pub name: String,
    /// Base URL for the API endpoint
    pub api_url: String,
}

impl AsRef<Self> for Connection {
    /// Returns a reference to this connection.
    fn as_ref(&self) -> &Self {
        self
    }
}

/// Settings for a music API integration.
///
/// Stores configuration and capabilities for integrating with external music APIs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct MusicApiSettings {
    /// Unique identifier for the music API
    pub id: String,
    /// Display name of the music API
    pub name: String,
    /// Whether the user is currently logged in to the API
    pub logged_in: bool,
    /// Whether the API supports library scanning
    pub supports_scan: bool,
    /// Whether library scanning is enabled for this API
    pub scan_enabled: bool,
    /// Endpoint URL for triggering a library scan
    pub run_scan_endpoint: Option<String>,
    /// Authentication method used by the API
    #[cfg(feature = "music-api-api")]
    pub auth_method: Option<AuthMethod>,
}

impl AsRef<Self> for MusicApiSettings {
    /// Returns a reference to these music API settings.
    fn as_ref(&self) -> &Self {
        self
    }
}

#[cfg(feature = "music-api-api")]
/// Conversions between music API models.
///
/// Provides `From` trait implementations to convert from API-level
/// music API models to application-level settings.
pub mod music_api_api {
    use moosicbox_music_api_api::models::ApiMusicApi;

    use crate::MusicApiSettings;

    /// Converts from API-level music API model to application-level settings.
    ///
    /// This conversion automatically generates the `run_scan_endpoint` URL
    /// based on the API name if scanning is supported.
    impl From<ApiMusicApi> for MusicApiSettings {
        fn from(value: ApiMusicApi) -> Self {
            Self {
                logged_in: value.logged_in,
                supports_scan: value.supports_scan,
                scan_enabled: value.scan_enabled,
                run_scan_endpoint: value
                    .supports_scan
                    .then(|| format!("/music-api/scan?apiSource={}", value.name)),
                auth_method: value.auth_method,
                name: value.name,
                id: value.id,
            }
        }
    }
}

/// Settings for managing download locations.
///
/// Stores configured download paths and the default location for new downloads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadSettings {
    /// List of download locations as (ID, path) pairs
    pub download_locations: Vec<(u64, String)>,
    /// Default path for new downloads
    pub default_download_location: Option<String>,
}

impl AsRef<Self> for DownloadSettings {
    /// Returns a reference to these download settings.
    fn as_ref(&self) -> &Self {
        self
    }
}

/// Settings for library scanning.
///
/// Contains the filesystem paths to scan for media files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanSettings {
    /// List of filesystem paths to scan for media files
    pub scan_paths: Vec<String>,
}

impl AsRef<Self> for ScanSettings {
    /// Returns a reference to these scan settings.
    fn as_ref(&self) -> &Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_default() {
        let conn = Connection::default();
        assert_eq!(conn.name, "");
        assert_eq!(conn.api_url, "");
    }

    #[test]
    fn test_connection_as_ref() {
        let conn = Connection {
            name: "Test Server".to_string(),
            api_url: "https://api.example.com".to_string(),
        };
        let conn_ref: &Connection = conn.as_ref();
        assert_eq!(conn_ref.name, "Test Server");
        assert_eq!(conn_ref.api_url, "https://api.example.com");
    }

    #[test]
    fn test_connection_serialization_roundtrip() {
        let conn = Connection {
            name: "My Server".to_string(),
            api_url: "https://test.local:8080/api".to_string(),
        };

        let json = serde_json::to_string(&conn).expect("Failed to serialize");
        let deserialized: Connection = serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(conn, deserialized);
    }

    #[test]
    fn test_music_api_settings_as_ref() {
        let settings = MusicApiSettings {
            id: "tidal".to_string(),
            name: "Tidal".to_string(),
            logged_in: true,
            supports_scan: true,
            scan_enabled: true,
            run_scan_endpoint: Some("/music-api/scan?apiSource=Tidal".to_string()),
            #[cfg(feature = "music-api-api")]
            auth_method: Some(AuthMethod::UsernamePassword),
        };

        let settings_ref: &MusicApiSettings = settings.as_ref();
        assert_eq!(settings_ref.id, "tidal");
        assert_eq!(settings_ref.name, "Tidal");
    }

    #[test]
    fn test_music_api_settings_serialization_roundtrip() {
        let settings = MusicApiSettings {
            id: "spotify".to_string(),
            name: "Spotify".to_string(),
            logged_in: false,
            supports_scan: false,
            scan_enabled: false,
            run_scan_endpoint: None,
            #[cfg(feature = "music-api-api")]
            auth_method: None,
        };

        let json = serde_json::to_string(&settings).expect("Failed to serialize");
        let deserialized: MusicApiSettings =
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(settings, deserialized);
    }

    #[cfg(feature = "music-api-api")]
    #[test]
    fn test_api_music_api_to_settings_with_scan_support() {
        use moosicbox_music_api_api::models::ApiMusicApi;

        let api = ApiMusicApi {
            id: "qobuz".to_string(),
            name: "Qobuz".to_string(),
            logged_in: true,
            supports_scan: true,
            scan_enabled: true,
            auth_method: Some(AuthMethod::UsernamePassword),
        };

        let settings: MusicApiSettings = api.into();

        assert_eq!(settings.id, "qobuz");
        assert_eq!(settings.name, "Qobuz");
        assert!(settings.logged_in);
        assert!(settings.supports_scan);
        assert!(settings.scan_enabled);
        assert_eq!(
            settings.run_scan_endpoint,
            Some("/music-api/scan?apiSource=Qobuz".to_string())
        );
        assert_eq!(settings.auth_method, Some(AuthMethod::UsernamePassword));
    }

    #[cfg(feature = "music-api-api")]
    #[test]
    fn test_api_music_api_to_settings_without_scan_support() {
        use moosicbox_music_api_api::models::ApiMusicApi;

        let api = ApiMusicApi {
            id: "local".to_string(),
            name: "Local Library".to_string(),
            logged_in: false,
            supports_scan: false,
            scan_enabled: false,
            auth_method: None,
        };

        let settings: MusicApiSettings = api.into();

        assert_eq!(settings.id, "local");
        assert_eq!(settings.name, "Local Library");
        assert!(!settings.logged_in);
        assert!(!settings.supports_scan);
        assert!(!settings.scan_enabled);
        // When supports_scan is false, run_scan_endpoint should be None
        assert_eq!(settings.run_scan_endpoint, None);
        assert_eq!(settings.auth_method, None);
    }

    #[cfg(feature = "music-api-api")]
    #[test]
    fn test_api_music_api_to_settings_scan_disabled_but_supported() {
        use moosicbox_music_api_api::models::ApiMusicApi;

        let api = ApiMusicApi {
            id: "tidal".to_string(),
            name: "Tidal".to_string(),
            logged_in: true,
            supports_scan: true,
            scan_enabled: false,
            auth_method: Some(AuthMethod::Poll),
        };

        let settings: MusicApiSettings = api.into();

        assert_eq!(settings.id, "tidal");
        assert!(settings.supports_scan);
        assert!(!settings.scan_enabled);
        // Even when scan_enabled is false, if supports_scan is true,
        // the endpoint should still be generated
        assert_eq!(
            settings.run_scan_endpoint,
            Some("/music-api/scan?apiSource=Tidal".to_string())
        );
        assert_eq!(settings.auth_method, Some(AuthMethod::Poll));
    }

    #[cfg(feature = "music-api-api")]
    #[test]
    fn test_api_music_api_to_settings_special_characters_in_name() {
        use moosicbox_music_api_api::models::ApiMusicApi;

        let api = ApiMusicApi {
            id: "test-api".to_string(),
            name: "Test API & Service".to_string(),
            logged_in: true,
            supports_scan: true,
            scan_enabled: true,
            auth_method: Some(AuthMethod::UsernamePassword),
        };

        let settings: MusicApiSettings = api.into();

        // The endpoint URL should include the exact name (with special chars)
        assert_eq!(
            settings.run_scan_endpoint,
            Some("/music-api/scan?apiSource=Test API & Service".to_string())
        );
    }

    #[test]
    fn test_download_settings_as_ref() {
        let settings = DownloadSettings {
            download_locations: vec![(1, "/music/downloads".to_string())],
            default_download_location: Some("/music/default".to_string()),
        };

        let settings_ref: &DownloadSettings = settings.as_ref();
        assert_eq!(settings_ref.download_locations.len(), 1);
        assert_eq!(
            settings_ref.default_download_location,
            Some("/music/default".to_string())
        );
    }

    #[test]
    fn test_download_settings_serialization_roundtrip() {
        let settings = DownloadSettings {
            download_locations: vec![
                (1, "/path/one".to_string()),
                (2, "/path/two".to_string()),
                (3, "/path/three".to_string()),
            ],
            default_download_location: Some("/path/default".to_string()),
        };

        let json = serde_json::to_string(&settings).expect("Failed to serialize");
        let deserialized: DownloadSettings =
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(settings, deserialized);
    }

    #[test]
    fn test_download_settings_empty_locations() {
        let settings = DownloadSettings {
            download_locations: vec![],
            default_download_location: None,
        };

        let json = serde_json::to_string(&settings).expect("Failed to serialize");
        let deserialized: DownloadSettings =
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(settings, deserialized);
        assert!(deserialized.download_locations.is_empty());
        assert_eq!(deserialized.default_download_location, None);
    }

    #[test]
    fn test_scan_settings_as_ref() {
        let settings = ScanSettings {
            scan_paths: vec!["/music".to_string(), "/media".to_string()],
        };

        let settings_ref: &ScanSettings = settings.as_ref();
        assert_eq!(settings_ref.scan_paths.len(), 2);
    }

    #[test]
    fn test_scan_settings_serialization_roundtrip() {
        let settings = ScanSettings {
            scan_paths: vec![
                "/home/user/Music".to_string(),
                "/mnt/external/audio".to_string(),
                "/var/media/library".to_string(),
            ],
        };

        let json = serde_json::to_string(&settings).expect("Failed to serialize");
        let deserialized: ScanSettings =
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(settings, deserialized);
    }

    #[test]
    fn test_scan_settings_empty_paths() {
        let settings = ScanSettings { scan_paths: vec![] };

        let json = serde_json::to_string(&settings).expect("Failed to serialize");
        let deserialized: ScanSettings =
            serde_json::from_str(&json).expect("Failed to deserialize");

        assert_eq!(settings, deserialized);
        assert!(deserialized.scan_paths.is_empty());
    }
}
