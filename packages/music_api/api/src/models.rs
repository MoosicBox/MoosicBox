use moosicbox_music_api::MusicApi;
use moosicbox_music_models::ApiSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct ApiMusicApi {
    pub name: String,
    pub logged_in: bool,
    pub run_scan_endpoint: Option<String>,
    pub auth_endpoint: Option<String>,
}

impl From<(&ApiSource, &dyn MusicApi)> for ApiMusicApi {
    fn from((source, _api): (&ApiSource, &dyn MusicApi)) -> Self {
        Self {
            name: source.to_string(),
            logged_in: false,
            run_scan_endpoint: None,
            auth_endpoint: None,
        }
    }
}
