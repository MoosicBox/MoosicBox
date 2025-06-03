#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::branches_sharing_code)]

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MusicApiSettings {
    pub id: String,
    pub name: String,
    pub logged_in: bool,
    pub authentication_enabled: bool,
    pub run_scan_endpoint: Option<String>,
    pub auth_endpoint: Option<String>,
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
                authentication_enabled: value.authentication_enabled,
                logged_in: value.logged_in,
                run_scan_endpoint: value
                    .scanning_enabled
                    .then(|| format!("/music-api/scan?apiSource={}", value.name)),
                auth_endpoint: value
                    .authentication_enabled
                    .then(|| format!("/music-api/auth?apiSource={}", value.name)),
                name: value.name,
                id: value.id,
            }
        }
    }
}
