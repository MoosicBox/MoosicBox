//! HTTP request extractors for the `MoosicBox` web server
//!
//! This module provides extractors that implement the `FromRequest` trait,
//! allowing automatic extraction of data from HTTP requests in handler functions.
//!
//! # Dual-Mode Support
//!
//! All extractors support both synchronous and asynchronous extraction:
//!
//! * **Actix backend**: Uses synchronous extraction to avoid Send bounds issues
//! * **Simulator backend**: Uses async extraction for deterministic testing
//!
//! # Available Extractors
//!
//! ## Serde-based Extractors (require `serde` feature)
//!
//! * [`Query<T>`] - Extract URL query parameters using `serde_urlencoded`
//! * [`Json<T>`] - Extract JSON request body using `serde_json`
//! * [`Path<T>`] - Extract URL path parameters with flexible mapping
//! * [`Header<T>`] - Extract typed headers with multiple strategies
//!
//! ## Core Extractors (always available)
//!
//! * [`State<T>`] - Extract application state with backend-specific storage
//!
//! # Error Handling
//!
//! Each extractor provides comprehensive error types:
//!
//! * [`QueryError`] - Query parameter parsing and validation errors
//! * [`JsonError`] - JSON parsing and content-type validation errors
//! * [`PathError`] - Path segment extraction and deserialization errors
//! * [`HeaderError`] - Header parsing and type conversion errors
//! * [`StateError`] - State lookup and initialization errors
//!
//! # Usage Examples
//!
//! ```rust,ignore
//! use switchy_web_server::extractors::{Query, Json, Path, Header, State};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct SearchParams {
//!     q: String,
//!     limit: Option<u32>,
//! }
//!
//! #[derive(Deserialize)]
//! struct CreateUser {
//!     name: String,
//!     email: String,
//! }
//!
//! // Extract query parameters
//! async fn search(Query(params): Query<SearchParams>) -> Result<HttpResponse, Error> {
//!     // Use params.q and params.limit
//!     Ok(HttpResponse::ok())
//! }
//!
//! // Extract JSON body
//! async fn create_user(Json(user): Json<CreateUser>) -> Result<HttpResponse, Error> {
//!     // Use user.name and user.email
//!     Ok(HttpResponse::ok())
//! }
//!
//! // Extract path parameters
//! async fn get_user(Path(user_id): Path<u64>) -> Result<HttpResponse, Error> {
//!     // Use user_id from URL path
//!     Ok(HttpResponse::ok())
//! }
//!
//! // Extract headers
//! async fn auth_handler(Header(auth): Header<String>) -> Result<HttpResponse, Error> {
//!     // Use authorization header value
//!     Ok(HttpResponse::ok())
//! }
//!
//! // Extract application state
//! async fn with_state(State(db): State<DatabasePool>) -> Result<HttpResponse, Error> {
//!     // Use shared database pool
//!     Ok(HttpResponse::ok())
//! }
//!
//! // Combine multiple extractors
//! async fn complex_handler(
//!     Path(id): Path<u64>,
//!     Query(params): Query<SearchParams>,
//!     Json(data): Json<CreateUser>,
//!     Header(auth): Header<String>,
//!     State(db): State<DatabasePool>,
//! ) -> Result<HttpResponse, Error> {
//!     // Use all extracted data together
//!     Ok(HttpResponse::ok())
//! }
//! ```
//!
//! # Feature Gates
//!
//! Most extractors require the `serde` feature to be enabled. Only the [`State`]
//! extractor is available without additional features, as it doesn't require
//! serialization/deserialization capabilities.

/// Query parameter extraction module
#[cfg(feature = "serde")]
pub mod query;

/// JSON body extraction module
#[cfg(feature = "serde")]
pub mod json;

/// Path parameter extraction module
#[cfg(feature = "serde")]
pub mod path;

/// HTTP header extraction module
#[cfg(feature = "serde")]
pub mod header;

/// Application state extraction module
pub mod state;

// Re-export extractors for convenient access
#[cfg(feature = "serde")]
pub use query::{Query, QueryError};

#[cfg(feature = "serde")]
pub use json::{Json, JsonError};

#[cfg(feature = "serde")]
pub use path::{Path, PathError};

#[cfg(feature = "serde")]
pub use header::{Header, HeaderError};

pub use state::{State, StateContainer, StateError};

/// Prelude module for convenient importing of common extractor types
///
/// This module re-exports the most commonly used extractor types and their
/// associated error types, allowing for convenient glob imports.
///
/// # Usage
///
/// ```rust,ignore
/// use switchy_web_server::extractors::prelude::*;
///
/// // Now you can use Query, Json, Path, Header, State directly
/// async fn handler(
///     Query(params): Query<MyParams>,
///     Json(data): Json<MyData>,
/// ) -> Result<HttpResponse, Error> {
///     // Handler implementation
///     Ok(HttpResponse::ok())
/// }
/// ```
pub mod prelude {
    #[cfg(feature = "serde")]
    pub use super::{Header, HeaderError, Json, JsonError, Path, PathError, Query, QueryError};

    pub use super::{State, StateContainer, StateError};
}
