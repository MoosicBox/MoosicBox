//! Global database configuration and initialization
//!
//! This module provides global database instance management for applications
//! that need a singleton database connection. It includes integration with
//! the actix-web framework for dependency injection.
//!
//! # Usage
//!
//! Initialize the global database instance at application startup:
//!
//! ```rust,ignore
//! use switchy_database::config;
//! use std::sync::Arc;
//!
//! # async fn example(db: Box<dyn switchy_database::Database>) {
//! // Initialize global database
//! config::init(Arc::new(db));
//!
//! // In actix-web handlers, use ConfigDatabase for dependency injection
//! // It will automatically extract the global database instance
//! # }
//! ```
//!
//! # Actix-web Integration
//!
//! When the `api` feature is enabled, [`ConfigDatabase`](crate::config::ConfigDatabase) implements actix-web's
//! `FromRequest` trait, allowing automatic extraction in handlers:
//!
//! ```rust,ignore
//! use actix_web::{web, HttpResponse};
//! use switchy_database::config::ConfigDatabase;
//!
//! async fn my_handler(db: ConfigDatabase) -> HttpResponse {
//!     // Use db as &dyn Database via Deref
//!     let results = db.select("users").execute(&*db).await?;
//!     HttpResponse::Ok().json(results)
//! }
//! ```

use std::{
    ops::Deref,
    sync::{Arc, LazyLock, RwLock},
};

use crate::Database;

#[allow(clippy::type_complexity)]
static DATABASE: LazyLock<Arc<RwLock<Option<Arc<Box<dyn Database>>>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(None)));

/// Initialize the global database instance
///
/// Sets the global database singleton that will be used by [`ConfigDatabase`]
/// throughout the application. This should be called once at application startup
/// before any database operations.
///
/// # Panics
///
/// * If fails to get a writer to the `DATABASE` `RwLock`
pub fn init(database: Arc<Box<dyn Database>>) {
    *DATABASE.write().unwrap() = Some(database);
}

/// Wrapper for the global database instance that implements actix-web's `FromRequest`
///
/// This struct provides access to the global database configured via [`init`].
/// It dereferences to `dyn Database` for convenient access.
///
/// ## Actix-web Integration
///
/// When the `api` feature is enabled, this implements `FromRequest` for automatic
/// dependency injection in actix-web handlers.
///
/// ## Examples
///
/// ```rust,ignore
/// use actix_web::{web, HttpResponse};
/// use switchy_database::config::ConfigDatabase;
///
/// async fn my_handler(db: ConfigDatabase) -> HttpResponse {
///     let users = db.select("users").execute(&*db).await.unwrap();
///     HttpResponse::Ok().json(users)
/// }
/// ```
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone)]
pub struct ConfigDatabase {
    /// The global database instance wrapped in Arc for thread-safe sharing
    pub database: Arc<Box<dyn Database>>,
}

impl From<&ConfigDatabase> for Arc<Box<dyn Database>> {
    fn from(value: &ConfigDatabase) -> Self {
        value.database.clone()
    }
}

impl From<ConfigDatabase> for Arc<Box<dyn Database>> {
    fn from(value: ConfigDatabase) -> Self {
        value.database
    }
}

impl From<Arc<Box<dyn Database>>> for ConfigDatabase {
    fn from(value: Arc<Box<dyn Database>>) -> Self {
        Self { database: value }
    }
}

impl<'a> From<&'a ConfigDatabase> for &'a dyn Database {
    fn from(value: &'a ConfigDatabase) -> Self {
        &**value.database
    }
}

impl Deref for ConfigDatabase {
    type Target = dyn Database;

    fn deref(&self) -> &Self::Target {
        &**self.database
    }
}

#[cfg(feature = "api")]
mod api {
    use actix_web::{FromRequest, HttpRequest, dev::Payload, error::ErrorInternalServerError};
    use futures::future::{Ready, err, ok};

    use super::DATABASE;

    impl FromRequest for super::ConfigDatabase {
        type Error = actix_web::Error;
        type Future = Ready<Result<Self, actix_web::Error>>;

        fn from_request(_req: &HttpRequest, _: &mut Payload) -> Self::Future {
            let Some(database) = DATABASE.read().unwrap().clone() else {
                return err(ErrorInternalServerError("Config database not initialized"));
            };

            ok(Self { database })
        }
    }
}
