//! Multi-database profile management
//!
//! This module provides functionality for managing multiple database connections
//! identified by profile names. It enables applications to work with multiple
//! databases simultaneously, switching between them based on user profiles or
//! application contexts.
//!
//! # Usage
//!
//! Register database instances with profile names:
//!
//! ```rust,ignore
//! use switchy_database::profiles::PROFILES;
//! use std::sync::Arc;
//!
//! # async fn example(db1: Box<dyn switchy_database::Database>, db2: Box<dyn switchy_database::Database>) {
//! // Register databases for different profiles
//! PROFILES.add("user1".to_string(), Arc::new(db1));
//! PROFILES.add("user2".to_string(), Arc::new(db2));
//!
//! // Retrieve a database by profile name
//! if let Some(db) = PROFILES.get("user1") {
//!     // Use db as &dyn Database via Deref
//!     let results = db.select("users").execute(&*db).await?;
//! }
//!
//! // List all profile names
//! let profiles = PROFILES.names();
//! # Ok::<(), switchy_database::DatabaseError>(())
//! # }
//! ```
//!
//! # Actix-web Integration
//!
//! When the `api` feature is enabled, [`LibraryDatabase`](crate::profiles::LibraryDatabase) implements actix-web's
//! `FromRequest` trait, automatically extracting the database for the current
//! profile from request headers:
//!
//! ```rust,ignore
//! use actix_web::{web, HttpResponse};
//! use switchy_database::profiles::LibraryDatabase;
//!
//! async fn my_handler(db: LibraryDatabase) -> HttpResponse {
//!     // db is automatically resolved from the profile header
//!     let results = db.select("tracks").execute(&*db).await?;
//!     HttpResponse::Ok().json(results)
//! }
//! ```

use std::{
    collections::BTreeMap,
    ops::Deref,
    sync::{Arc, LazyLock, RwLock},
};

use crate::Database;

/// Global registry of database profiles
///
/// This static provides thread-safe access to the global database profiles registry.
/// Use this to register and retrieve database connections for different profiles.
///
/// # Examples
///
/// ```rust,ignore
/// use switchy_database::profiles::PROFILES;
/// use std::sync::Arc;
///
/// # async fn example(db: Box<dyn switchy_database::Database>) {
/// PROFILES.add("user1".to_string(), Arc::new(db));
/// let db = PROFILES.get("user1").unwrap();
/// # }
/// ```
pub static PROFILES: LazyLock<DatabaseProfiles> = LazyLock::new(DatabaseProfiles::default);

/// Manager for multiple database connections indexed by profile names
///
/// This struct maintains a thread-safe mapping of profile names to database instances,
/// enabling applications to manage multiple databases for different users or contexts.
///
/// ## Thread Safety
///
/// All methods use internal `RwLock` for thread-safe concurrent access. Write operations
/// (add/remove) are serialized, while read operations (get) can occur concurrently.
///
/// ## Examples
///
/// ```rust,ignore
/// use switchy_database::profiles::PROFILES;
/// use std::sync::Arc;
///
/// # async fn example(db1: Box<dyn switchy_database::Database>, db2: Box<dyn switchy_database::Database>) {
/// // Register databases for different profiles
/// PROFILES.add("user1".to_string(), Arc::new(db1));
/// PROFILES.add("user2".to_string(), Arc::new(db2));
///
/// // Retrieve by profile
/// if let Some(db) = PROFILES.get("user1") {
///     // Use database
/// }
/// # }
/// ```
#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct DatabaseProfiles {
    profiles: Arc<RwLock<BTreeMap<String, LibraryDatabase>>>,
}

impl DatabaseProfiles {
    /// Register a database instance for the specified profile
    ///
    /// Associates a database connection with a profile name, making it available
    /// for retrieval via [`get`](Self::get). If a database is already registered
    /// for this profile, it will be replaced.
    ///
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    pub fn add(&self, profile: String, database: Arc<Box<dyn Database>>) {
        moosicbox_profiles::PROFILES.add(profile.clone());
        self.profiles
            .write()
            .unwrap()
            .insert(profile, LibraryDatabase { database });
    }

    /// Remove a database profile
    ///
    /// Removes the database associated with the specified profile name.
    /// If the profile doesn't exist, this operation has no effect.
    ///
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    pub fn remove(&self, profile: &str) {
        self.profiles.write().unwrap().retain(|p, _| p != profile);
    }

    /// Register a database and immediately retrieve it
    ///
    /// Convenience method that combines [`add`](Self::add) and [`get`](Self::get)
    /// in a single operation. Registers the database for the specified profile
    /// and returns the wrapped database instance.
    ///
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned or the profile somehow wasn't added to the list of
    /// profiles
    #[must_use]
    pub fn add_fetch(&self, profile: &str, database: Arc<Box<dyn Database>>) -> LibraryDatabase {
        self.add(profile.to_owned(), database);
        self.get(profile).unwrap()
    }

    /// Retrieve the database for a specific profile
    ///
    /// Returns the database instance associated with the given profile name,
    /// or `None` if no database is registered for that profile.
    ///
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn get(&self, profile: &str) -> Option<LibraryDatabase> {
        self.profiles
            .read()
            .unwrap()
            .iter()
            .find_map(|(p, db)| if p == profile { Some(db.clone()) } else { None })
    }

    /// Get a list of all registered profile names
    ///
    /// Returns a vector containing the names of all profiles that have
    /// databases registered. The order is determined by the internal
    /// `BTreeMap` (alphabetically sorted).
    ///
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.profiles.read().unwrap().keys().cloned().collect()
    }
}

/// Wrapper for a profile-specific database instance
///
/// This struct wraps a database instance associated with a specific user profile.
/// It provides `Deref` to `dyn Database` for convenient access and implements
/// actix-web's `FromRequest` when the `api` feature is enabled.
///
/// ## Actix-web Integration
///
/// When the `api` feature is enabled, this implements `FromRequest` to automatically
/// extract the database for the current profile from request headers.
///
/// ## Examples
///
/// ```rust,ignore
/// use actix_web::{web, HttpResponse};
/// use switchy_database::profiles::LibraryDatabase;
///
/// async fn my_handler(db: LibraryDatabase) -> HttpResponse {
///     // db is automatically resolved from the profile header
///     let tracks = db.select("tracks").execute(&*db).await.unwrap();
///     HttpResponse::Ok().json(tracks)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct LibraryDatabase {
    /// The database instance for this profile
    pub database: Arc<Box<dyn Database>>,
}

impl From<&LibraryDatabase> for Arc<Box<dyn Database>> {
    fn from(value: &LibraryDatabase) -> Self {
        value.database.clone()
    }
}

impl From<LibraryDatabase> for Arc<Box<dyn Database>> {
    fn from(value: LibraryDatabase) -> Self {
        value.database
    }
}

impl From<Arc<Box<dyn Database>>> for LibraryDatabase {
    fn from(value: Arc<Box<dyn Database>>) -> Self {
        Self { database: value }
    }
}

impl<'a> From<&'a LibraryDatabase> for &'a dyn Database {
    fn from(value: &'a LibraryDatabase) -> Self {
        &**value.database
    }
}

impl Deref for LibraryDatabase {
    type Target = dyn Database;

    fn deref(&self) -> &Self::Target {
        &**self.database
    }
}

#[cfg(feature = "api")]
/// Actix-web integration for profile-based database access
pub mod api {
    use actix_web::{FromRequest, HttpRequest, dev::Payload, error::ErrorBadRequest};
    use futures::future::{Ready, err, ok};
    use moosicbox_profiles::api::ProfileName;

    use super::{LibraryDatabase, PROFILES};

    impl FromRequest for LibraryDatabase {
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

            let Some(database) = PROFILES.get(&profile.0) else {
                return err(ErrorBadRequest("Invalid profile"));
            };

            ok(database)
        }
    }
}

#[cfg(all(test, feature = "simulator"))]
mod tests {
    use super::*;
    use crate::simulator::SimulationDatabase;

    #[test_log::test]
    fn test_database_profiles_add_and_get() {
        let profiles = DatabaseProfiles::default();
        let db = Arc::new(
            Box::new(SimulationDatabase::new_for_path(None).unwrap()) as Box<dyn Database>
        );

        profiles.add("test_profile".to_string(), db.clone());

        let retrieved = profiles.get("test_profile");
        assert!(retrieved.is_some());
    }

    #[test_log::test]
    fn test_database_profiles_get_nonexistent() {
        let profiles = DatabaseProfiles::default();

        let retrieved = profiles.get("nonexistent_profile");
        assert!(retrieved.is_none());
    }

    #[test_log::test]
    fn test_database_profiles_remove() {
        let profiles = DatabaseProfiles::default();
        let db = Arc::new(
            Box::new(SimulationDatabase::new_for_path(None).unwrap()) as Box<dyn Database>
        );

        profiles.add("test_profile".to_string(), db);
        assert!(profiles.get("test_profile").is_some());

        profiles.remove("test_profile");
        assert!(profiles.get("test_profile").is_none());
    }

    #[test_log::test]
    fn test_database_profiles_remove_nonexistent() {
        let profiles = DatabaseProfiles::default();

        // Should not panic when removing nonexistent profile
        profiles.remove("nonexistent_profile");
    }

    #[test_log::test]
    fn test_database_profiles_add_fetch() {
        let profiles = DatabaseProfiles::default();
        let db = Arc::new(
            Box::new(SimulationDatabase::new_for_path(None).unwrap()) as Box<dyn Database>
        );

        let retrieved = profiles.add_fetch("test_profile", db);

        // Should return the added database
        assert!(std::ptr::addr_eq(
            std::ptr::addr_of!(**retrieved.database),
            std::ptr::addr_of!(**profiles.get("test_profile").unwrap().database)
        ));
    }

    #[test_log::test]
    fn test_database_profiles_names() {
        let profiles = DatabaseProfiles::default();
        let db1 = Arc::new(
            Box::new(SimulationDatabase::new_for_path(None).unwrap()) as Box<dyn Database>
        );
        let db2 = Arc::new(
            Box::new(SimulationDatabase::new_for_path(None).unwrap()) as Box<dyn Database>
        );

        profiles.add("profile1".to_string(), db1);
        profiles.add("profile2".to_string(), db2);

        let names = profiles.names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"profile1".to_string()));
        assert!(names.contains(&"profile2".to_string()));
    }

    #[test_log::test]
    fn test_database_profiles_names_empty() {
        let profiles = DatabaseProfiles::default();
        let names = profiles.names();
        assert!(names.is_empty());
    }

    #[test_log::test]
    fn test_database_profiles_replace_existing() {
        let profiles = DatabaseProfiles::default();
        let db1 = Arc::new(
            Box::new(SimulationDatabase::new_for_path(None).unwrap()) as Box<dyn Database>
        );
        let db2 = Arc::new(
            Box::new(SimulationDatabase::new_for_path(None).unwrap()) as Box<dyn Database>
        );

        profiles.add("test_profile".to_string(), db1.clone());
        let first = profiles.get("test_profile").unwrap();

        profiles.add("test_profile".to_string(), db2.clone());
        let second = profiles.get("test_profile").unwrap();

        // The second database should replace the first
        assert!(!std::ptr::addr_eq(
            std::ptr::addr_of!(**first.database),
            std::ptr::addr_of!(**second.database)
        ));
    }

    #[test_log::test]
    fn test_library_database_from_arc() {
        let db = Arc::new(
            Box::new(SimulationDatabase::new_for_path(None).unwrap()) as Box<dyn Database>
        );
        let library_db: LibraryDatabase = db.clone().into();

        assert!(std::ptr::addr_eq(
            std::ptr::addr_of!(**library_db.database),
            std::ptr::addr_of!(**db)
        ));
    }

    #[test_log::test]
    fn test_library_database_into_arc() {
        let db = Arc::new(
            Box::new(SimulationDatabase::new_for_path(None).unwrap()) as Box<dyn Database>
        );
        let library_db = LibraryDatabase {
            database: db.clone(),
        };

        let arc_db: Arc<Box<dyn Database>> = library_db.into();
        assert!(std::ptr::addr_eq(
            std::ptr::addr_of!(**arc_db),
            std::ptr::addr_of!(**db)
        ));
    }

    #[test_log::test]
    fn test_library_database_deref() {
        let db = Arc::new(
            Box::new(SimulationDatabase::new_for_path(None).unwrap()) as Box<dyn Database>
        );
        let library_db = LibraryDatabase {
            database: db.clone(),
        };

        // Should be able to use as &dyn Database via Deref
        let _db_ref: &dyn Database = &*library_db;
    }

    #[test_log::test]
    fn test_library_database_ref_into_dyn_database() {
        let db = Arc::new(
            Box::new(SimulationDatabase::new_for_path(None).unwrap()) as Box<dyn Database>
        );
        let library_db = LibraryDatabase { database: db };

        let db_ref: &dyn Database = (&library_db).into();
        // Just verify the conversion works
        let _ = db_ref;
    }
}
