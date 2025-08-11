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
//! * [`Query<T>`] - Extract URL query parameters
//! * [`Json<T>`] - Extract JSON request body
//! * [`Path<T>`] - Extract URL path parameters
//! * [`Header<T>`] - Extract specific headers (coming soon)
//! * [`State<T>`] - Extract application state (coming soon)
//!
//! # Usage Examples
//!
//! ```rust,ignore
//! use moosicbox_web_server::extractors::{Query, Json};
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
//! // Combine multiple extractors
//! async fn complex_handler(
//!     Query(params): Query<SearchParams>,
//!     Json(data): Json<CreateUser>,
//! ) -> Result<HttpResponse, Error> {
//!     // Use both query parameters and JSON body
//!     Ok(HttpResponse::ok())
//! }
//! ```

#[cfg(feature = "serde")]
pub mod query;

#[cfg(feature = "serde")]
pub mod json;

#[cfg(feature = "serde")]
pub mod path;

// Re-export extractors for convenient access
#[cfg(feature = "serde")]
pub use query::{Query, QueryError};

#[cfg(feature = "serde")]
pub use json::{Json, JsonError};

#[cfg(feature = "serde")]
pub use path::{Path, PathError};

// pub mod header;
// pub use header::{Header, HeaderError};

// pub mod state;
// pub use state::{State, StateError};
