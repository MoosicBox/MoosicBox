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
