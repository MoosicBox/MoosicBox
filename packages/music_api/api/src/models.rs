use moosicbox_music_api::MusicApi;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiMusicApi {
    pub id: String,
    pub name: String,
    pub logged_in: bool,
    pub supports_scan: bool,
    pub scan_enabled: bool,
    pub supports_authentication: bool,
}

pub async fn convert_to_api_music_api(
    api: &dyn MusicApi,
) -> Result<ApiMusicApi, moosicbox_music_api::Error> {
    let auth = api.auth();
    Ok(ApiMusicApi {
        id: api.source().to_string(),
        name: api.source().to_string_display(),
        supports_authentication: auth.is_some(),
        logged_in: if let Some(auth) = auth {
            auth.is_logged_in().await?
        } else {
            false
        },
        supports_scan: api.supports_scan(),
        scan_enabled: if api.supports_scan() {
            api.scan_enabled().await?
        } else {
            false
        },
    })
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "type")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AuthValues {
    UsernamePassword { username: String, password: String },
    Poll,
}
