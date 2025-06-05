use moosicbox_music_api::{MusicApi, auth::Auth};
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
    pub auth_method: Option<AuthMethod>,
}

pub async fn convert_to_api_music_api(
    api: &dyn MusicApi,
) -> Result<ApiMusicApi, moosicbox_music_api::Error> {
    let auth = api.auth();
    Ok(ApiMusicApi {
        id: api.source().to_string(),
        name: api.source().to_string_display(),
        auth_method: auth.and_then(|x| auth_method(x)),
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

fn auth_method(value: &Auth) -> Option<AuthMethod> {
    match value {
        #[cfg(feature = "auth-username-password")]
        Auth::UsernamePassword(..) => Some(AuthMethod::UsernamePassword),
        #[cfg(feature = "auth-poll")]
        Auth::Poll(..) => Some(AuthMethod::Poll),
        Auth::None => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AuthMethod {
    UsernamePassword,
    Poll,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "type")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AuthValues {
    UsernamePassword { username: String, password: String },
    Poll,
}
