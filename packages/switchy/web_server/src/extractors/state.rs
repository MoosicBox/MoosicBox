//! Application state extractor for HTTP handlers.
//!
//! This module provides the [`State<T>`] extractor for accessing shared application
//! state within request handlers, along with [`StateContainer`] for managing state
//! in the simulator backend.
//!
//! # Overview
//!
//! The state extractor provides thread-safe access to application-wide state that
//! is shared across all request handlers. State is stored as `Arc<T>` for efficient
//! sharing.
//!
//! # Example
//!
//! ```rust,ignore
//! use switchy_web_server::extractors::State;
//! use std::sync::Arc;
//!
//! #[derive(Clone)]
//! struct AppConfig {
//!     database_url: String,
//!     api_key: String,
//! }
//!
//! async fn handler(State(config): State<AppConfig>) -> Result<HttpResponse, Error> {
//!     println!("Database: {}", config.database_url);
//!     Ok(HttpResponse::ok())
//! }
//! ```

#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use crate::{
    Error, HttpRequest,
    from_request::{FromRequest, IntoHandlerError},
};
use std::{collections::BTreeMap, fmt, sync::Arc};

/// Error types that can occur during state extraction
#[derive(Debug)]
pub enum StateError {
    /// Requested state type is not found in the application
    NotFound {
        /// The type name that was requested
        type_name: &'static str,
    },
    /// State container is not initialized
    NotInitialized {
        /// The backend that failed to initialize state
        backend: String,
    },
    /// State type mismatch (wrong type requested)
    TypeMismatch {
        /// The type that was requested
        requested_type: &'static str,
        /// The type that was found (if available)
        found_type: Option<String>,
    },
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound { type_name } => {
                write!(f, "State of type '{type_name}' not found in application")
            }
            Self::NotInitialized { backend } => {
                write!(f, "State container not initialized for backend '{backend}'")
            }
            Self::TypeMismatch {
                requested_type,
                found_type,
            } => {
                if let Some(found) = found_type {
                    write!(
                        f,
                        "State type mismatch: requested '{requested_type}', found '{found}'"
                    )
                } else {
                    write!(
                        f,
                        "State type mismatch: requested '{requested_type}', no matching type found"
                    )
                }
            }
        }
    }
}

impl std::error::Error for StateError {}

impl IntoHandlerError for StateError {
    fn into_handler_error(self) -> Error {
        Error::internal_server_error(self.to_string())
    }
}

/// Extractor for application state with backend-specific storage
///
/// This extractor provides access to application state that is shared across
/// request handlers. The implementation differs between backends due to different
/// state storage mechanisms.
///
/// # Backend-Specific Implementation
///
/// * **Actix backend**: Uses `actix_web::web::Data<T>` for thread-safe state sharing
/// * **Simulator backend**: Uses custom state container for deterministic testing
///
/// # State Storage
///
/// State is stored as `Arc<T>` to enable efficient sharing across multiple handlers
/// and threads. The state must be registered with the application before it can
/// be extracted.
///
/// # Examples
///
/// ## Basic State Usage
///
/// ```rust,ignore
/// use switchy_web_server::extractors::State;
/// use std::sync::Arc;
///
/// #[derive(Clone)]
/// struct AppConfig {
///     database_url: String,
///     api_key: String,
/// }
///
/// async fn handler(State(config): State<AppConfig>) -> Result<HttpResponse, Error> {
///     println!("Database URL: {}", config.database_url);
///     println!("API Key: {}", config.api_key);
///     Ok(HttpResponse::ok())
/// }
/// ```
///
/// ## Multiple State Types
///
/// ```rust,ignore
/// use switchy_web_server::extractors::State;
///
/// #[derive(Clone)]
/// struct DatabasePool {
///     // database connection pool
/// }
///
/// #[derive(Clone)]
/// struct CacheClient {
///     // redis client
/// }
///
/// async fn handler(
///     State(db): State<DatabasePool>,
///     State(cache): State<CacheClient>,
/// ) -> Result<HttpResponse, Error> {
///     // Use both database and cache
///     Ok(HttpResponse::ok())
/// }
/// ```
///
/// # State Registration
///
/// State must be registered with the web server before it can be extracted:
///
/// ```rust,ignore
/// // For Actix backend
/// let app_data = web::Data::new(AppConfig {
///     database_url: "postgresql://...".to_string(),
///     api_key: "secret".to_string(),
/// });
///
/// // For Simulator backend
/// let mut state_container = StateContainer::new();
/// state_container.insert(AppConfig {
///     database_url: "postgresql://...".to_string(),
///     api_key: "secret".to_string(),
/// });
/// ```
///
/// # Error Handling
///
/// * **State not found**: Returns `StateError::NotFound` when requested state type is not registered
/// * **Not initialized**: Returns `StateError::NotInitialized` when state container is not set up
/// * **Type mismatch**: Returns `StateError::TypeMismatch` when wrong type is requested
///
/// All errors are automatically converted to HTTP 500 Internal Server Error responses.
#[derive(Debug)]
pub struct State<T>(pub Arc<T>);

impl<T> State<T> {
    /// Create a new State extractor with the given value
    #[must_use]
    pub const fn new(value: Arc<T>) -> Self {
        Self(value)
    }

    /// Get the inner `Arc<T>` value
    #[must_use]
    pub fn into_inner(self) -> Arc<T> {
        self.0
    }

    /// Get a reference to the inner value
    #[must_use]
    pub fn get(&self) -> &T {
        &self.0
    }
}

impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> std::ops::Deref for State<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// State container for the Simulator backend
///
/// This provides a simple type-erased state storage mechanism for testing
/// and simulation purposes. Unlike Actix's `web::Data`, this is designed
/// for single-threaded deterministic testing.
#[derive(Debug, Default)]
pub struct StateContainer {
    /// Type-erased state storage using `TypeId` as keys
    states: BTreeMap<std::any::TypeId, crate::request::ErasedState>,
    /// Type names for debugging purposes
    type_names: BTreeMap<std::any::TypeId, &'static str>,
}

impl StateContainer {
    /// Create a new empty state container
    #[must_use]
    pub fn new() -> Self {
        Self {
            states: BTreeMap::new(),
            type_names: BTreeMap::new(),
        }
    }

    /// Insert state of type T into the container
    pub fn insert<T: Send + Sync + 'static>(&mut self, state: T) {
        let type_id = std::any::TypeId::of::<T>();
        let type_name = std::any::type_name::<T>();
        self.states.insert(type_id, Arc::new(state));
        self.type_names.insert(type_id, type_name);
    }

    /// Get state of type T from the container
    #[must_use]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        let type_id = std::any::TypeId::of::<T>();
        self.states
            .get(&type_id)
            .and_then(|arc| Arc::clone(arc).downcast::<T>().ok())
    }

    /// Get type-erased state by `TypeId`
    #[must_use]
    pub fn get_any(&self, type_id: std::any::TypeId) -> Option<crate::request::ErasedState> {
        self.states.get(&type_id).cloned()
    }

    /// Check if state of type T exists in the container
    #[must_use]
    pub fn contains<T: Send + Sync + 'static>(&self) -> bool {
        let type_id = std::any::TypeId::of::<T>();
        self.states.contains_key(&type_id)
    }

    /// Remove state of type T from the container
    pub fn remove<T: Send + Sync + 'static>(&mut self) -> Option<Arc<T>> {
        let type_id = std::any::TypeId::of::<T>();
        self.type_names.remove(&type_id);
        self.states
            .remove(&type_id)
            .and_then(|arc| arc.downcast::<T>().ok())
    }

    /// Get all registered type names
    #[must_use]
    pub fn type_names(&self) -> Vec<&'static str> {
        self.type_names.values().copied().collect()
    }

    /// Clear all state from the container
    pub fn clear(&mut self) {
        self.states.clear();
        self.type_names.clear();
    }
}

// Unified implementation for all backends using the trait-based HttpRequest
impl<T: Send + Sync + 'static> FromRequest for State<T> {
    type Error = StateError;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        // Use the app_state method from HttpRequestTrait
        req.app_state::<T>()
            .map(Self::new)
            .ok_or_else(|| StateError::NotFound {
                type_name: std::any::type_name::<T>(),
            })
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct TestConfig {
        name: String,
        value: u32,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct DatabaseConfig {
        url: String,
        max_connections: usize,
    }

    #[test]
    fn test_state_container_insert_and_get() {
        let mut container = StateContainer::new();

        let config = TestConfig {
            name: "test".to_string(),
            value: 123,
        };

        container.insert(config.clone());

        let retrieved = container.get::<TestConfig>();
        assert!(retrieved.is_some());
        assert_eq!(*retrieved.unwrap(), config);
    }

    #[test]
    fn test_state_container_multiple_types() {
        let mut container = StateContainer::new();

        let test_config = TestConfig {
            name: "test".to_string(),
            value: 123,
        };

        let db_config = DatabaseConfig {
            url: "postgresql://localhost/test".to_string(),
            max_connections: 5,
        };

        container.insert(test_config.clone());
        container.insert(db_config.clone());

        let retrieved_test = container.get::<TestConfig>();
        let retrieved_db = container.get::<DatabaseConfig>();

        assert!(retrieved_test.is_some());
        assert!(retrieved_db.is_some());
        assert_eq!(*retrieved_test.unwrap(), test_config);
        assert_eq!(*retrieved_db.unwrap(), db_config);
    }

    #[test]
    fn test_state_container_not_found() {
        let container = StateContainer::new();
        let result = container.get::<TestConfig>();
        assert!(result.is_none());
    }

    #[test]
    fn test_state_container_contains() {
        let mut container = StateContainer::new();

        assert!(!container.contains::<TestConfig>());

        container.insert(TestConfig {
            name: "test".to_string(),
            value: 123,
        });

        assert!(container.contains::<TestConfig>());
        assert!(!container.contains::<DatabaseConfig>());
    }

    #[test]
    fn test_state_container_remove() {
        let mut container = StateContainer::new();

        let config = TestConfig {
            name: "test".to_string(),
            value: 123,
        };

        container.insert(config.clone());
        assert!(container.contains::<TestConfig>());

        let removed = container.remove::<TestConfig>();
        assert!(removed.is_some());
        assert_eq!(*removed.unwrap(), config);
        assert!(!container.contains::<TestConfig>());
    }

    #[test]
    fn test_state_container_clear() {
        let mut container = StateContainer::new();

        container.insert(TestConfig {
            name: "test".to_string(),
            value: 123,
        });
        container.insert(DatabaseConfig {
            url: "postgresql://localhost/test".to_string(),
            max_connections: 5,
        });

        assert!(container.contains::<TestConfig>());
        assert!(container.contains::<DatabaseConfig>());

        container.clear();

        assert!(!container.contains::<TestConfig>());
        assert!(!container.contains::<DatabaseConfig>());
    }

    #[test]
    fn test_state_container_type_names() {
        let mut container = StateContainer::new();

        container.insert(TestConfig {
            name: "test".to_string(),
            value: 123,
        });
        container.insert(DatabaseConfig {
            url: "postgresql://localhost/test".to_string(),
            max_connections: 5,
        });

        let type_names = container.type_names();
        assert_eq!(type_names.len(), 2);
        assert!(type_names.contains(&std::any::type_name::<TestConfig>()));
        assert!(type_names.contains(&std::any::type_name::<DatabaseConfig>()));
    }

    #[test]
    fn test_state_error_display() {
        let error = StateError::NotFound {
            type_name: "TestConfig",
        };
        assert_eq!(
            error.to_string(),
            "State of type 'TestConfig' not found in application"
        );

        let error = StateError::NotInitialized {
            backend: "simulator".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "State container not initialized for backend 'simulator'"
        );

        let error = StateError::TypeMismatch {
            requested_type: "TestConfig",
            found_type: Some("DatabaseConfig".to_string()),
        };
        assert!(error.to_string().contains("State type mismatch"));
    }

    #[test]
    fn test_state_new_and_into_inner() {
        let config = TestConfig {
            name: "test".to_string(),
            value: 123,
        };
        let arc_config = Arc::new(config.clone());
        let state = State::new(Arc::clone(&arc_config));

        assert_eq!(state.get(), &config);
        assert_eq!(*state.into_inner(), config);
    }

    #[test]
    fn test_state_clone() {
        let config = TestConfig {
            name: "test".to_string(),
            value: 123,
        };
        let state1 = State::new(Arc::new(config.clone()));
        let state2 = state1.clone();

        assert_eq!(state1.get(), state2.get());
        assert_eq!(*state1.get(), config);
        assert_eq!(*state2.get(), config);
    }

    #[test]
    fn test_state_deref() {
        let config = TestConfig {
            name: "test".to_string(),
            value: 123,
        };
        let state = State::new(Arc::new(config.clone()));

        // Test Deref trait
        assert_eq!(state.name, config.name);
        assert_eq!(state.value, config.value);
    }

    // Note: FromRequest tests would require actual Actix or Simulator integration
    // which is complex to set up in unit tests. These would be better as integration tests.
}
