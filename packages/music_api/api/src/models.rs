//! Data models for music API HTTP requests and responses.
//!
//! This module provides the API-layer representations of music service providers,
//! authentication methods, and conversion utilities from the core music API types
//! to their HTTP API equivalents.

use moosicbox_music_api::{MusicApi, auth::Auth};
use serde::{Deserialize, Serialize};

/// API representation of a music service provider.
///
/// Contains the current state and capabilities of a music API provider,
/// including authentication status, scanning capabilities, and configuration.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiMusicApi {
    /// Unique identifier for the music API source
    pub id: String,
    /// Display name of the music API
    pub name: String,
    /// Whether the user is currently authenticated with this API
    pub logged_in: bool,
    /// Whether this API supports library scanning
    pub supports_scan: bool,
    /// Whether library scanning is currently enabled for this API
    pub scan_enabled: bool,
    /// The authentication method supported by this API, if any
    pub auth_method: Option<AuthMethod>,
}

/// Converts a `MusicApi` trait object into its API representation.
///
/// This function queries the music API's current state to build an `ApiMusicApi`
/// struct containing authentication status, scanning capabilities, and configuration.
///
/// # Errors
///
/// * Returns an error if checking the login status fails
/// * Returns an error if checking the scan enabled status fails
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

/// Determines the API authentication method from an `Auth` enum variant.
///
/// Maps the internal authentication representation to the API's public `AuthMethod` enum.
/// Returns `None` if the authentication type is not supported by the enabled features.
const fn auth_method(value: &Auth) -> Option<AuthMethod> {
    match value {
        #[cfg(feature = "auth-username-password")]
        Auth::UsernamePassword(..) => Some(AuthMethod::UsernamePassword),
        #[cfg(feature = "auth-poll")]
        Auth::Poll(..) => Some(AuthMethod::Poll),
        #[cfg(not(feature = "_auth"))]
        _ => None,
        #[cfg(feature = "_auth")]
        Auth::None => None,
    }
}

/// Authentication method supported by a music API provider.
///
/// Different music APIs may support different authentication mechanisms.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AuthMethod {
    /// Traditional username and password authentication
    UsernamePassword,
    /// OAuth-style device polling authentication
    Poll,
}

/// Authentication credentials for authenticating with a music API.
///
/// Contains the actual credential values needed for the specified authentication method.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "type")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AuthValues {
    /// Username and password credentials
    UsernamePassword {
        /// Username for authentication
        username: String,
        /// Password for authentication
        password: String,
    },
    /// No credentials needed - OAuth polling will be initiated
    Poll,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use moosicbox_music_api::auth::{ApiAuth, ApiAuthBuilder};
    use moosicbox_music_api::{Error, MusicApi, TrackOrId};
    use moosicbox_music_api_models::{
        AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder, ArtistOrderDirection,
        TrackAudioQuality, TrackOrder, TrackOrderDirection, TrackSource,
    };
    use moosicbox_music_models::{
        Album, AlbumType, ApiSource, Artist, PlaybackQuality, Track, id::Id,
    };
    use moosicbox_paging::{PagingResponse, PagingResult};
    use switchy_async::sync::RwLock;

    use super::*;
    struct MockMusicApi {
        source: ApiSource,
        auth: Option<ApiAuth>,
        supports_scan: bool,
        scan_enabled: Arc<RwLock<bool>>,
    }

    impl MockMusicApi {
        fn new(source: ApiSource) -> Self {
            Self {
                source,
                auth: None,
                supports_scan: false,
                scan_enabled: Arc::new(RwLock::new(false)),
            }
        }

        fn with_auth(mut self, auth: ApiAuth) -> Self {
            self.auth = Some(auth);
            self
        }

        fn with_supports_scan(mut self, supports_scan: bool) -> Self {
            self.supports_scan = supports_scan;
            self
        }

        fn with_scan_enabled(mut self, scan_enabled: bool) -> Self {
            self.scan_enabled = Arc::new(RwLock::new(scan_enabled));
            self
        }
    }

    #[async_trait]
    impl MusicApi for MockMusicApi {
        fn source(&self) -> &ApiSource {
            &self.source
        }

        fn auth(&self) -> Option<&ApiAuth> {
            self.auth.as_ref()
        }

        fn supports_scan(&self) -> bool {
            self.supports_scan
        }

        async fn scan_enabled(&self) -> Result<bool, Error> {
            Ok(*self.scan_enabled.read().await)
        }

        async fn artists(
            &self,
            _offset: Option<u32>,
            _limit: Option<u32>,
            _order: Option<ArtistOrder>,
            _order_direction: Option<ArtistOrderDirection>,
        ) -> PagingResult<Artist, Error> {
            Ok(PagingResponse::empty())
        }

        async fn artist(&self, _artist_id: &Id) -> Result<Option<Artist>, Error> {
            Ok(None)
        }

        async fn add_artist(&self, _artist_id: &Id) -> Result<(), Error> {
            Ok(())
        }

        async fn remove_artist(&self, _artist_id: &Id) -> Result<(), Error> {
            Ok(())
        }

        async fn albums(&self, _request: &AlbumsRequest) -> PagingResult<Album, Error> {
            Ok(PagingResponse::empty())
        }

        async fn album(&self, _album_id: &Id) -> Result<Option<Album>, Error> {
            Ok(None)
        }

        async fn album_versions(
            &self,
            _album_id: &Id,
            _offset: Option<u32>,
            _limit: Option<u32>,
        ) -> PagingResult<moosicbox_menu_models::AlbumVersion, Error> {
            Ok(PagingResponse::empty())
        }

        #[allow(clippy::too_many_arguments)]
        async fn artist_albums(
            &self,
            _artist_id: &Id,
            _album_type: Option<AlbumType>,
            _offset: Option<u32>,
            _limit: Option<u32>,
            _order: Option<AlbumOrder>,
            _order_direction: Option<AlbumOrderDirection>,
        ) -> PagingResult<Album, Error> {
            Ok(PagingResponse::empty())
        }

        async fn add_album(&self, _album_id: &Id) -> Result<(), Error> {
            Ok(())
        }

        async fn remove_album(&self, _album_id: &Id) -> Result<(), Error> {
            Ok(())
        }

        async fn tracks(
            &self,
            _track_ids: Option<&[Id]>,
            _offset: Option<u32>,
            _limit: Option<u32>,
            _order: Option<TrackOrder>,
            _order_direction: Option<TrackOrderDirection>,
        ) -> PagingResult<Track, Error> {
            Ok(PagingResponse::empty())
        }

        async fn track(&self, _track_id: &Id) -> Result<Option<Track>, Error> {
            Ok(None)
        }

        async fn album_tracks(
            &self,
            _album_id: &Id,
            _offset: Option<u32>,
            _limit: Option<u32>,
            _order: Option<TrackOrder>,
            _order_direction: Option<TrackOrderDirection>,
        ) -> PagingResult<Track, Error> {
            Ok(PagingResponse::empty())
        }

        async fn add_track(&self, _track_id: &Id) -> Result<(), Error> {
            Ok(())
        }

        async fn remove_track(&self, _track_id: &Id) -> Result<(), Error> {
            Ok(())
        }

        async fn track_source(
            &self,
            _track: TrackOrId,
            _quality: TrackAudioQuality,
        ) -> Result<Option<TrackSource>, Error> {
            Ok(None)
        }

        async fn track_size(
            &self,
            _track: TrackOrId,
            _source: &TrackSource,
            _quality: PlaybackQuality,
        ) -> Result<Option<u64>, Error> {
            Ok(None)
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn convert_to_api_music_api_without_auth_sets_logged_in_false() {
        let source = ApiSource::register("test_no_auth", "Test No Auth");
        let api = MockMusicApi::new(source.clone());

        let result = convert_to_api_music_api(&api).await.unwrap();

        assert_eq!(result.id, source.to_string());
        assert_eq!(result.name, source.to_string_display());
        assert!(!result.logged_in);
        assert!(result.auth_method.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn convert_to_api_music_api_with_auth_returns_logged_in_status() {
        let source = ApiSource::register("test_with_auth_logged_in", "Test Auth");
        let auth = ApiAuthBuilder::new()
            .without_auth()
            .with_logged_in(true)
            .build();
        let api = MockMusicApi::new(source.clone()).with_auth(auth);

        let result = convert_to_api_music_api(&api).await.unwrap();

        assert!(result.logged_in);
    }

    #[test_log::test(switchy_async::test)]
    async fn convert_to_api_music_api_with_auth_not_logged_in() {
        let source = ApiSource::register("test_with_auth_not_logged_in", "Test Auth");
        let auth = ApiAuthBuilder::new()
            .without_auth()
            .with_logged_in(false)
            .build();
        let api = MockMusicApi::new(source.clone()).with_auth(auth);

        let result = convert_to_api_music_api(&api).await.unwrap();

        assert!(!result.logged_in);
    }

    #[test_log::test(switchy_async::test)]
    async fn convert_to_api_music_api_without_scan_support_returns_false() {
        let source = ApiSource::register("test_no_scan", "Test No Scan");
        let api = MockMusicApi::new(source.clone()).with_supports_scan(false);

        let result = convert_to_api_music_api(&api).await.unwrap();

        assert!(!result.supports_scan);
        assert!(!result.scan_enabled);
    }

    #[test_log::test(switchy_async::test)]
    async fn convert_to_api_music_api_with_scan_support_disabled() {
        let source = ApiSource::register("test_scan_disabled", "Test Scan Disabled");
        let api = MockMusicApi::new(source.clone())
            .with_supports_scan(true)
            .with_scan_enabled(false);

        let result = convert_to_api_music_api(&api).await.unwrap();

        assert!(result.supports_scan);
        assert!(!result.scan_enabled);
    }

    #[test_log::test(switchy_async::test)]
    async fn convert_to_api_music_api_with_scan_support_enabled() {
        let source = ApiSource::register("test_scan_enabled", "Test Scan Enabled");
        let api = MockMusicApi::new(source.clone())
            .with_supports_scan(true)
            .with_scan_enabled(true);

        let result = convert_to_api_music_api(&api).await.unwrap();

        assert!(result.supports_scan);
        assert!(result.scan_enabled);
    }

    #[test_log::test(switchy_async::test)]
    async fn convert_to_api_music_api_maps_source_id_and_name_correctly() {
        let source = ApiSource::register("my_api_id", "My API Display Name");
        let api = MockMusicApi::new(source.clone());

        let result = convert_to_api_music_api(&api).await.unwrap();

        assert_eq!(result.id, "my_api_id");
        assert_eq!(result.name, "My API Display Name");
    }

    #[test_log::test]
    fn test_api_music_api_serialization() {
        let api = ApiMusicApi {
            id: "qobuz".to_string(),
            name: "Qobuz".to_string(),
            logged_in: true,
            supports_scan: true,
            scan_enabled: false,
            auth_method: Some(AuthMethod::UsernamePassword),
        };

        let json = serde_json::to_string(&api).unwrap();
        let deserialized: ApiMusicApi = serde_json::from_str(&json).unwrap();

        assert_eq!(api, deserialized);
    }

    #[test_log::test]
    fn test_api_music_api_camel_case_serialization() {
        let api = ApiMusicApi {
            id: "tidal".to_string(),
            name: "Tidal".to_string(),
            logged_in: false,
            supports_scan: true,
            scan_enabled: true,
            auth_method: None,
        };

        let json = serde_json::to_string(&api).unwrap();

        assert!(json.contains("\"loggedIn\":false"));
        assert!(json.contains("\"supportsScan\":true"));
        assert!(json.contains("\"scanEnabled\":true"));
        assert!(json.contains("\"authMethod\":null"));
    }

    #[test_log::test]
    fn test_auth_method_serialization() {
        let username_password = AuthMethod::UsernamePassword;
        let poll = AuthMethod::Poll;

        let json_up = serde_json::to_string(&username_password).unwrap();
        let json_poll = serde_json::to_string(&poll).unwrap();

        assert_eq!(json_up, "\"UsernamePassword\"");
        assert_eq!(json_poll, "\"Poll\"");

        let deserialized_up: AuthMethod = serde_json::from_str(&json_up).unwrap();
        let deserialized_poll: AuthMethod = serde_json::from_str(&json_poll).unwrap();

        assert_eq!(deserialized_up, AuthMethod::UsernamePassword);
        assert_eq!(deserialized_poll, AuthMethod::Poll);
    }

    #[test_log::test]
    fn test_auth_values_username_password_serialization() {
        let auth = AuthValues::UsernamePassword {
            username: "test_user".to_string(),
            password: "secret123".to_string(),
        };

        let json = serde_json::to_string(&auth).unwrap();
        let deserialized: AuthValues = serde_json::from_str(&json).unwrap();

        match deserialized {
            AuthValues::UsernamePassword { username, password } => {
                assert_eq!(username, "test_user");
                assert_eq!(password, "secret123");
            }
            AuthValues::Poll => panic!("Expected UsernamePassword variant"),
        }

        assert!(json.contains("\"type\":\"username-password\""));
    }

    #[test_log::test]
    fn test_auth_values_poll_serialization() {
        let auth = AuthValues::Poll;

        let json = serde_json::to_string(&auth).unwrap();
        let deserialized: AuthValues = serde_json::from_str(&json).unwrap();

        match deserialized {
            AuthValues::Poll => {}
            AuthValues::UsernamePassword { .. } => panic!("Expected Poll variant"),
        }

        assert!(json.contains("\"type\":\"poll\""));
    }

    #[test_log::test]
    fn test_auth_values_kebab_case_tag() {
        let auth = AuthValues::UsernamePassword {
            username: "user".to_string(),
            password: "pass".to_string(),
        };

        let json = serde_json::to_string(&auth).unwrap();

        // Verify kebab-case for both type tag and field names
        assert!(json.contains("\"type\":\"username-password\""));
        assert!(json.contains("\"username\":\"user\""));
        assert!(json.contains("\"password\":\"pass\""));
    }
}
