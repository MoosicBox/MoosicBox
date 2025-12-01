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

        /// Extracts the `LibraryMusicApi` instance from an HTTP request based on the profile.
        ///
        /// Reads the profile name from the request and retrieves the corresponding
        /// library music API instance from the global registry.
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

#[cfg(all(test, feature = "simulator"))]
mod tests {
    use std::sync::Arc;

    use moosicbox_music_api::MusicApi;
    use switchy_database::{Database, profiles::LibraryDatabase, simulator::SimulationDatabase};

    use super::*;

    /// Creates a test `LibraryDatabase` using a simulation database.
    fn create_test_db() -> LibraryDatabase {
        let db = Arc::new(Box::new(SimulationDatabase::new().unwrap()) as Box<dyn Database>);
        LibraryDatabase::from(db)
    }

    /// Tests that adding and retrieving a profile works correctly.
    /// Verifies that after adding a profile, it can be retrieved with `get()`.
    #[test_log::test]
    fn test_library_music_api_profiles_add_and_get() {
        let profiles = LibraryMusicApiProfiles::default();
        let db = create_test_db();

        profiles.add("test_profile".to_string(), db);

        let retrieved = profiles.get("test_profile");
        assert!(retrieved.is_some());
    }

    /// Tests that getting a non-existent profile returns `None`.
    #[test_log::test]
    fn test_library_music_api_profiles_get_nonexistent() {
        let profiles = LibraryMusicApiProfiles::default();

        let retrieved = profiles.get("nonexistent_profile");
        assert!(retrieved.is_none());
    }

    /// Tests that removing a profile works correctly.
    /// Verifies that after removal, the profile is no longer accessible.
    #[test_log::test]
    fn test_library_music_api_profiles_remove() {
        let profiles = LibraryMusicApiProfiles::default();
        let db = create_test_db();

        profiles.add("test_profile".to_string(), db);
        assert!(profiles.get("test_profile").is_some());

        profiles.remove("test_profile");
        assert!(profiles.get("test_profile").is_none());
    }

    /// Tests that removing a non-existent profile does not panic.
    #[test_log::test]
    fn test_library_music_api_profiles_remove_nonexistent() {
        let profiles = LibraryMusicApiProfiles::default();

        // Should not panic when removing nonexistent profile
        profiles.remove("nonexistent_profile");
    }

    /// Tests that `add_fetch` both adds the profile and returns the API instance.
    #[test_log::test]
    fn test_library_music_api_profiles_add_fetch() {
        let profiles = LibraryMusicApiProfiles::default();
        let db = create_test_db();

        let api = profiles.add_fetch("test_profile", db);

        // Should return a valid library API
        assert!(api.source().is_library());

        // Profile should be retrievable
        let retrieved = profiles.get("test_profile");
        assert!(retrieved.is_some());
    }

    /// Tests that `names()` returns all registered profile names.
    #[test_log::test]
    fn test_library_music_api_profiles_names() {
        let profiles = LibraryMusicApiProfiles::default();
        let db1 = create_test_db();
        let db2 = create_test_db();

        profiles.add("profile_alpha".to_string(), db1);
        profiles.add("profile_beta".to_string(), db2);

        let names = profiles.names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"profile_alpha".to_string()));
        assert!(names.contains(&"profile_beta".to_string()));
    }

    /// Tests that `names()` returns an empty vector when no profiles exist.
    #[test_log::test]
    fn test_library_music_api_profiles_names_empty() {
        let profiles = LibraryMusicApiProfiles::default();
        let names = profiles.names();
        assert!(names.is_empty());
    }

    /// Tests that adding a profile with the same name replaces the existing one.
    #[test_log::test]
    fn test_library_music_api_profiles_replace_existing() {
        let profiles = LibraryMusicApiProfiles::default();
        let db1 = create_test_db();
        let db2 = create_test_db();

        profiles.add("test_profile".to_string(), db1);
        let _first = profiles.get("test_profile").unwrap();

        profiles.add("test_profile".to_string(), db2);
        let _second = profiles.get("test_profile").unwrap();

        // Should still only have one profile with this name
        let names = profiles.names();
        assert_eq!(names.iter().filter(|n| *n == "test_profile").count(), 1);
    }

    /// Tests that profile names are returned in sorted order (`BTreeMap` behavior).
    #[test_log::test]
    fn test_library_music_api_profiles_names_sorted() {
        let profiles = LibraryMusicApiProfiles::default();

        // Add profiles in non-alphabetical order
        profiles.add("zebra".to_string(), create_test_db());
        profiles.add("apple".to_string(), create_test_db());
        profiles.add("mango".to_string(), create_test_db());

        let names = profiles.names();
        assert_eq!(names, vec!["apple", "mango", "zebra"]);
    }
}
