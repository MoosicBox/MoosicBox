use moosicbox_music_api::MusicApi;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiMusicApi {
    pub id: String,
    pub name: String,
    pub logged_in: bool,
    pub scanning_enabled: bool,
    pub authentication_enabled: bool,
}

pub async fn convert_to_api_music_api(
    api: &dyn MusicApi,
) -> Result<ApiMusicApi, moosicbox_music_api::Error> {
    Ok(ApiMusicApi {
        id: api.source().to_string(),
        name: api.source().to_string_display(),
        logged_in: if api.authentication_enabled() {
            api.is_logged_in().await?
        } else {
            false
        },
        scanning_enabled: api.scan_enabled(),
        authentication_enabled: api.authentication_enabled(),
    })
}
