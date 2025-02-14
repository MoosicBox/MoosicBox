#![allow(clippy::type_complexity)]

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, RwLock},
};

use moosicbox_music_models::ApiSource;

use crate::{MusicApi, MusicApis};

pub static PROFILES: LazyLock<MusicApisProfiles<std::hash::RandomState>> =
    LazyLock::new(MusicApisProfiles::default);

#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct MusicApisProfiles<S: ::std::hash::BuildHasher + Clone = std::hash::RandomState> {
    profiles: Arc<RwLock<Vec<(String, MusicApis<S>)>>>,
}

impl<S: ::std::hash::BuildHasher + Clone> MusicApisProfiles<S> {
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    pub fn add(
        &self,
        profile: String,
        music_apis: Arc<HashMap<ApiSource, Arc<Box<dyn MusicApi>>, S>>,
    ) {
        self.profiles
            .write()
            .unwrap()
            .push((profile, MusicApis(music_apis)));
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    pub fn remove(&self, profile: &str) {
        self.profiles.write().unwrap().retain(|(p, _)| p != profile);
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned or the profile somehow wasn't added to the list of
    /// profiles
    #[must_use]
    pub fn fetch_add(
        &self,
        profile: &str,
        music_apis: Arc<HashMap<ApiSource, Arc<Box<dyn MusicApi>>, S>>,
    ) -> MusicApis<S> {
        self.add(profile.to_owned(), music_apis);
        self.get(profile).unwrap()
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn get(&self, profile: &str) -> Option<MusicApis<S>> {
        self.profiles.read().unwrap().iter().find_map(|(p, api)| {
            if p == profile {
                Some(api.clone())
            } else {
                None
            }
        })
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.profiles
            .read()
            .unwrap()
            .iter()
            .map(|(profile, _)| profile.clone())
            .collect()
    }
}

#[cfg(feature = "api")]
pub mod api {
    use actix_web::{dev::Payload, error::ErrorBadRequest, FromRequest, HttpRequest};
    use futures::future::{err, ok, Ready};
    use moosicbox_database::profiles::api::ProfileName;

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
