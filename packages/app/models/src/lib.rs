#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::branches_sharing_code)]

use serde::{Deserialize, Serialize};

#[cfg(feature = "music-api-api")]
pub use moosicbox_music_api_api::models::AuthMethod;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Connection {
    pub name: String,
    pub api_url: String,
}

impl AsRef<Self> for Connection {
    fn as_ref(&self) -> &Self {
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct MusicApiSettings {
    pub id: String,
    pub name: String,
    pub logged_in: bool,
    pub supports_scan: bool,
    pub scan_enabled: bool,
    pub run_scan_endpoint: Option<String>,
    #[cfg(feature = "music-api-api")]
    pub auth_method: Option<AuthMethod>,
}

impl AsRef<Self> for MusicApiSettings {
    fn as_ref(&self) -> &Self {
        self
    }
}

#[cfg(feature = "music-api-api")]
pub mod music_api_api {
    use moosicbox_music_api_api::models::ApiMusicApi;

    use crate::MusicApiSettings;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DownloadSettings {
    pub download_locations: Vec<(u64, String)>,
    pub default_download_location: Option<String>,
}

impl AsRef<Self> for DownloadSettings {
    fn as_ref(&self) -> &Self {
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanSettings {
    pub scan_paths: Vec<String>,
}

impl AsRef<Self> for ScanSettings {
    fn as_ref(&self) -> &Self {
        self
    }
}
