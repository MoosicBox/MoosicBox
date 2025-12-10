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
    use super::*;

    mod convert_to_api_music_api_tests {
        use std::sync::LazyLock;

        use async_trait::async_trait;
        use moosicbox_menu_models::AlbumVersion;
        use moosicbox_music_api::{
            Error, MusicApi, TrackOrId,
            auth::{ApiAuth, Auth},
            models::{
                AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder, ArtistOrderDirection,
                TrackAudioQuality, TrackOrder, TrackOrderDirection, TrackSource,
            },
        };
        use moosicbox_music_models::id::Id;
        use moosicbox_music_models::{Album, AlbumType, ApiSource, Artist, PlaybackQuality, Track};
        use moosicbox_paging::{PagingResponse, PagingResult};

        use super::convert_to_api_music_api;

        static TEST_SOURCE: LazyLock<ApiSource> =
            LazyLock::new(|| ApiSource::register("test_api", "Test API"));

        /// Mock `MusicApi` that doesn't support scanning or auth
        struct BasicMusicApi;

        #[async_trait]
        impl MusicApi for BasicMusicApi {
            fn source(&self) -> &ApiSource {
                &TEST_SOURCE
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
            ) -> PagingResult<AlbumVersion, Error> {
                Ok(PagingResponse::empty())
            }

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

        /// Mock `MusicApi` that supports scanning
        struct ScanSupportingMusicApi {
            scan_enabled: bool,
        }

        impl ScanSupportingMusicApi {
            const fn new(scan_enabled: bool) -> Self {
                Self { scan_enabled }
            }
        }

        #[async_trait]
        impl MusicApi for ScanSupportingMusicApi {
            fn source(&self) -> &ApiSource {
                &TEST_SOURCE
            }

            fn supports_scan(&self) -> bool {
                true
            }

            async fn scan_enabled(&self) -> Result<bool, Error> {
                Ok(self.scan_enabled)
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
            ) -> PagingResult<AlbumVersion, Error> {
                Ok(PagingResponse::empty())
            }

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

        /// Mock `MusicApi` with authentication
        struct AuthMusicApi {
            api_auth: ApiAuth,
        }

        impl AuthMusicApi {
            fn new(logged_in: bool) -> Self {
                Self {
                    api_auth: ApiAuth::builder()
                        .without_auth()
                        .with_logged_in(logged_in)
                        .build(),
                }
            }
        }

        #[async_trait]
        impl MusicApi for AuthMusicApi {
            fn source(&self) -> &ApiSource {
                &TEST_SOURCE
            }

            fn auth(&self) -> Option<&ApiAuth> {
                Some(&self.api_auth)
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
            ) -> PagingResult<AlbumVersion, Error> {
                Ok(PagingResponse::empty())
            }

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
        async fn converts_basic_api_without_scan_or_auth() {
            let api = BasicMusicApi;
            let result = convert_to_api_music_api(&api).await.unwrap();

            assert_eq!(result.id, "test_api");
            assert_eq!(result.name, "Test API");
            assert!(!result.logged_in);
            assert!(!result.supports_scan);
            assert!(!result.scan_enabled);
            assert!(result.auth_method.is_none());
        }

        #[test_log::test(switchy_async::test)]
        async fn converts_api_with_scan_support_enabled() {
            let api = ScanSupportingMusicApi::new(true);
            let result = convert_to_api_music_api(&api).await.unwrap();

            assert!(result.supports_scan);
            assert!(result.scan_enabled);
        }

        #[test_log::test(switchy_async::test)]
        async fn converts_api_with_scan_support_disabled() {
            let api = ScanSupportingMusicApi::new(false);
            let result = convert_to_api_music_api(&api).await.unwrap();

            assert!(result.supports_scan);
            assert!(!result.scan_enabled);
        }

        #[test_log::test(switchy_async::test)]
        async fn converts_api_with_auth_logged_in() {
            let api = AuthMusicApi::new(true);
            let result = convert_to_api_music_api(&api).await.unwrap();

            assert!(result.logged_in);
            // Auth method is None because ApiAuth with Auth::None doesn't map to a specific method
            assert!(result.auth_method.is_none());
        }

        #[test_log::test(switchy_async::test)]
        async fn converts_api_with_auth_not_logged_in() {
            let api = AuthMusicApi::new(false);
            let result = convert_to_api_music_api(&api).await.unwrap();

            assert!(!result.logged_in);
        }

        #[test_log::test]
        fn auth_method_returns_none_for_auth_none() {
            let auth = Auth::None;
            assert!(super::super::auth_method(&auth).is_none());
        }
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
