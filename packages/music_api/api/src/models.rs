use moosicbox_music_api::MusicApi;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ApiMusicApi {
    pub name: String,
    pub logged_in: bool,
    pub scanning_enabled: bool,
    pub authentication_enabled: bool,
}

pub async fn convert_to_api_music_api(api: &dyn MusicApi) -> ApiMusicApi {
    ApiMusicApi {
        name: api.source().to_string(),
        logged_in: api.is_logged_in().await.unwrap_or_default(),
        scanning_enabled: api.scan_enabled(),
        authentication_enabled: api.authentication_enabled(),
    }
}
