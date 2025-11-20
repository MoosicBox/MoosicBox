//! Profile-based music API registry.
//!
//! This module provides [`MusicApisProfiles`], a global registry for managing collections
//! of music APIs associated with different user profiles. Each profile can have its own
//! set of configured music API sources.

#![allow(clippy::type_complexity)]

use std::{
    collections::BTreeMap,
    sync::{Arc, LazyLock, RwLock},
};

use moosicbox_music_models::ApiSource;

use crate::{MusicApi, MusicApis};

/// Global registry of music API collections by profile.
pub static PROFILES: LazyLock<MusicApisProfiles> = LazyLock::new(MusicApisProfiles::default);

/// Registry for managing music API collections associated with profiles.
#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct MusicApisProfiles {
    profiles: Arc<RwLock<BTreeMap<String, MusicApis>>>,
}

impl MusicApisProfiles {
    /// Adds a music API collection for the specified profile.
    ///
    /// # Panics
    ///
    /// * If the `RwLock` is poisoned
    pub fn add(
        &self,
        profile: String,
        music_apis: Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>>,
    ) {
        moosicbox_profiles::PROFILES.add(profile.clone());
        self.profiles
            .write()
            .unwrap()
            .insert(profile, MusicApis(music_apis));
    }

    /// Inserts or updates a music API collection for the specified profile.
    ///
    /// # Panics
    ///
    /// * If the `RwLock` is poisoned
    pub fn upsert(
        &self,
        profile: String,
        music_apis: Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>>,
    ) {
        let mut profiles = self.profiles.write().unwrap();

        if let Some(existing) = profiles.iter_mut().find(|(p, _)| *p == &profile) {
            *existing.1 = MusicApis(music_apis);
        } else {
            profiles.insert(profile, MusicApis(music_apis));
        }
    }

    /// Removes the music API collection for the specified profile.
    ///
    /// # Panics
    ///
    /// * If the `RwLock` is poisoned
    pub fn remove(&self, profile: &str) {
        self.profiles.write().unwrap().remove(profile);
    }

    /// Adds a music API collection and returns it.
    ///
    /// # Panics
    ///
    /// * If the `RwLock` is poisoned
    /// * If the profile was not added successfully
    #[must_use]
    pub fn add_fetch(
        &self,
        profile: &str,
        music_apis: Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>>,
    ) -> MusicApis {
        self.add(profile.to_owned(), music_apis);
        self.get(profile).unwrap()
    }

    /// Retrieves the music API collection for the specified profile.
    ///
    /// # Panics
    ///
    /// * If the `RwLock` is poisoned
    #[must_use]
    pub fn get(&self, profile: &str) -> Option<MusicApis> {
        self.profiles.read().unwrap().iter().find_map(|(p, api)| {
            if p == profile {
                Some(api.clone())
            } else {
                None
            }
        })
    }

    /// Returns the names of all registered profiles.
    ///
    /// # Panics
    ///
    /// * If the `RwLock` is poisoned
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.profiles.read().unwrap().keys().cloned().collect()
    }
}

#[cfg(test)]
mod test {
    use std::{collections::BTreeMap, sync::Arc};

    use moosicbox_music_api_models::{
        AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder, ArtistOrderDirection,
        TrackAudioQuality, TrackOrder, TrackOrderDirection, TrackSource,
    };
    use moosicbox_music_models::{
        Album, AlbumType, ApiSource, Artist, PlaybackQuality, Track, id::Id,
    };
    use moosicbox_paging::{PagingResponse, PagingResult};

    use crate::{Error, MusicApi, SourceToMusicApi};

    use super::MusicApisProfiles;

    pub struct TestMusicApi {
        source: ApiSource,
    }

    impl TestMusicApi {
        fn new(source: ApiSource) -> Self {
            Self { source }
        }
    }

    #[async_trait::async_trait]
    impl MusicApi for TestMusicApi {
        fn source(&self) -> &ApiSource {
            &self.source
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
            _track: crate::TrackOrId,
            _quality: TrackAudioQuality,
        ) -> Result<Option<TrackSource>, Error> {
            Ok(None)
        }

        async fn track_size(
            &self,
            _track: crate::TrackOrId,
            _source: &TrackSource,
            _quality: PlaybackQuality,
        ) -> Result<Option<u64>, Error> {
            Ok(None)
        }
    }

    #[test_log::test]
    fn profiles_add_stores_profile() {
        let profiles = MusicApisProfiles::default();
        let source = ApiSource::register("test_profiles_add", "test");
        let api: Arc<Box<dyn MusicApi>> = Arc::new(Box::new(TestMusicApi::new(source.clone())));
        let mut map = BTreeMap::new();
        map.insert(source, api);
        let music_apis = Arc::new(map);

        profiles.add("test_profile".to_owned(), music_apis);

        let retrieved = profiles.get("test_profile");
        assert!(retrieved.is_some());
    }

    #[test_log::test]
    fn profiles_get_returns_none_for_unknown_profile() {
        let profiles = MusicApisProfiles::default();

        let retrieved = profiles.get("unknown_profile");
        assert!(retrieved.is_none());
    }

    #[test_log::test]
    fn profiles_upsert_adds_new_profile() {
        let profiles = MusicApisProfiles::default();
        let source = ApiSource::register("test_profiles_upsert_add", "test");
        let api: Arc<Box<dyn MusicApi>> = Arc::new(Box::new(TestMusicApi::new(source.clone())));
        let mut map = BTreeMap::new();
        map.insert(source, api);
        let music_apis = Arc::new(map);

        profiles.upsert("new_profile".to_owned(), music_apis);

        let retrieved = profiles.get("new_profile");
        assert!(retrieved.is_some());
    }

    #[test_log::test]
    fn profiles_upsert_updates_existing_profile() {
        let profiles = MusicApisProfiles::default();
        let source1 = ApiSource::register("test_profiles_upsert_update_1", "test");
        let source2 = ApiSource::register("test_profiles_upsert_update_2", "test");

        let api1: Arc<Box<dyn MusicApi>> = Arc::new(Box::new(TestMusicApi::new(source1.clone())));
        let mut map1 = BTreeMap::new();
        map1.insert(source1, api1);
        let music_apis1 = Arc::new(map1);

        let api2: Arc<Box<dyn MusicApi>> = Arc::new(Box::new(TestMusicApi::new(source2.clone())));
        let mut map2 = BTreeMap::new();
        map2.insert(source2.clone(), api2);
        let music_apis2 = Arc::new(map2);

        profiles.add("update_profile".to_owned(), music_apis1);
        profiles.upsert("update_profile".to_owned(), music_apis2);

        let retrieved = profiles.get("update_profile");
        assert!(retrieved.is_some());
        let retrieved_apis = retrieved.unwrap();
        assert!(retrieved_apis.get(&source2).is_some());
    }

    #[test_log::test]
    fn profiles_remove_removes_profile() {
        let profiles = MusicApisProfiles::default();
        let source = ApiSource::register("test_profiles_remove", "test");
        let api: Arc<Box<dyn MusicApi>> = Arc::new(Box::new(TestMusicApi::new(source.clone())));
        let mut map = BTreeMap::new();
        map.insert(source, api);
        let music_apis = Arc::new(map);

        profiles.add("remove_profile".to_owned(), music_apis);
        assert!(profiles.get("remove_profile").is_some());

        profiles.remove("remove_profile");
        assert!(profiles.get("remove_profile").is_none());
    }

    #[test_log::test]
    fn profiles_add_fetch_returns_added_profile() {
        let profiles = MusicApisProfiles::default();
        let source = ApiSource::register("test_profiles_add_fetch", "test");
        let api: Arc<Box<dyn MusicApi>> = Arc::new(Box::new(TestMusicApi::new(source.clone())));
        let mut map = BTreeMap::new();
        map.insert(source.clone(), api);
        let music_apis = Arc::new(map);

        let retrieved = profiles.add_fetch("add_fetch_profile", music_apis);

        assert!(retrieved.get(&source).is_some());
    }

    #[test_log::test]
    fn profiles_names_returns_all_profile_names() {
        let profiles = MusicApisProfiles::default();
        let source1 = ApiSource::register("test_profiles_names_1", "test");
        let source2 = ApiSource::register("test_profiles_names_2", "test");

        let api1: Arc<Box<dyn MusicApi>> = Arc::new(Box::new(TestMusicApi::new(source1.clone())));
        let mut map1 = BTreeMap::new();
        map1.insert(source1, api1);

        let api2: Arc<Box<dyn MusicApi>> = Arc::new(Box::new(TestMusicApi::new(source2.clone())));
        let mut map2 = BTreeMap::new();
        map2.insert(source2, api2);

        profiles.add("profile1".to_owned(), Arc::new(map1));
        profiles.add("profile2".to_owned(), Arc::new(map2));

        let names = profiles.names();
        assert!(names.contains(&"profile1".to_owned()));
        assert!(names.contains(&"profile2".to_owned()));
    }
}

/// Actix-web integration for extracting `MusicApis` from HTTP requests.
#[cfg(feature = "api")]
pub mod api {
    use actix_web::{FromRequest, HttpRequest, dev::Payload, error::ErrorBadRequest};
    use futures::future::{Ready, err, ok};
    use moosicbox_profiles::api::ProfileName;

    use super::{MusicApis, PROFILES};

    impl FromRequest for MusicApis {
        type Error = actix_web::Error;
        type Future = Ready<Result<Self, actix_web::Error>>;

        fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
            let profile = ProfileName::from_request_inner(req);
            let profile = match profile {
                Ok(profile) => profile,
                Err(e) => {
                    return err(e);
                }
            };

            let Some(music_apis) = PROFILES.get(&profile.0) else {
                return err(ErrorBadRequest("Invalid profile"));
            };

            ok(music_apis)
        }
    }
}
