//! Profile management for library music API instances.
//!
//! This module provides functionality for managing multiple library music API instances
//! across different profiles, allowing applications to work with multiple library databases
//! simultaneously. It includes a global registry ([`crate::profiles::PROFILES`]) for storing
//! and retrieving profile-specific API instances.

use std::{
    collections::BTreeMap,
    sync::{Arc, LazyLock, RwLock},
};

use switchy_database::profiles::LibraryDatabase;

use crate::LibraryMusicApi;

/// Global registry of library music API instances by profile.
pub static PROFILES: LazyLock<LibraryMusicApiProfiles> =
    LazyLock::new(LibraryMusicApiProfiles::default);

/// Manager for library music API instances across multiple profiles.
#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct LibraryMusicApiProfiles {
    profiles: Arc<RwLock<BTreeMap<String, LibraryMusicApi>>>,
}

impl LibraryMusicApiProfiles {
    /// Adds a library music API instance for the specified profile.
    ///
    /// # Panics
    ///
    /// * Will panic if `RwLock` is poisoned
    pub fn add(&self, profile: String, db: LibraryDatabase) {
        moosicbox_profiles::PROFILES.add(profile.clone());
        self.profiles
            .write()
            .unwrap()
            .insert(profile, LibraryMusicApi { db });
    }

    /// Removes the library music API instance for the specified profile.
    ///
    /// # Panics
    ///
    /// * Will panic if `RwLock` is poisoned
    pub fn remove(&self, profile: &str) {
        self.profiles.write().unwrap().remove(profile);
    }

    /// Adds a library music API instance for the specified profile and returns it.
    ///
    /// # Panics
    ///
    /// * Will panic if `RwLock` is poisoned or the profile somehow wasn't added to the list of
    ///   profiles
    #[must_use]
    pub fn add_fetch(&self, profile: &str, db: LibraryDatabase) -> LibraryMusicApi {
        self.add(profile.to_owned(), db);
        self.get(profile).unwrap()
    }

    /// Gets the library music API instance for the specified profile.
    ///
    /// # Panics
    ///
    /// * Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn get(&self, profile: &str) -> Option<LibraryMusicApi> {
        self.profiles
            .read()
            .unwrap()
            .iter()
            .find_map(|(p, db)| if p == profile { Some(db.clone()) } else { None })
    }

    /// Returns all profile names.
    ///
    /// # Panics
    ///
    /// * Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.profiles.read().unwrap().keys().cloned().collect()
    }
}

/// Actix-web integration for library music API profiles.
///
/// This module provides `FromRequest` implementation for `LibraryMusicApi`, enabling
/// automatic extraction of the appropriate library music API instance based on the
/// profile specified in HTTP requests.
#[cfg(feature = "api")]
pub mod api {
    use actix_web::{FromRequest, HttpRequest, dev::Payload, error::ErrorBadRequest};
    use futures::future::{Ready, err, ok};
    use moosicbox_profiles::api::ProfileName;

    use super::{LibraryMusicApi, PROFILES};

    impl FromRequest for LibraryMusicApi {
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

#[cfg(test)]
mod tests {
    // Note: Comprehensive profile management tests require database setup and are
    // better suited for integration tests. The profile management code is relatively
    // straightforward (add/get/remove operations on a BTreeMap) and is well-tested
    // through the application's usage and integration tests.
    //
    // Unit tests here would require mocking or creating actual database instances,
    // which adds complexity without providing significant additional value over the
    // integration test coverage that already exists.
}
