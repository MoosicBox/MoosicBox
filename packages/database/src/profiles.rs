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
