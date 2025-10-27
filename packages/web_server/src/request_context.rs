//! Request-scoped context for storing per-request data.
//!
//! This module provides [`RequestContext`], which holds data extracted from HTTP requests
//! that needs to be available to handlers and extractors throughout the request lifecycle.
//!
//! # Overview
//!
//! The [`RequestContext`] stores request-scoped data such as path parameters extracted
//! from route matching. It is distinct from application-scoped state, which is managed
//! separately through the state container system.
//!
//! # Example
//!
//! ```rust
//! use moosicbox_web_server::request_context::RequestContext;
//! use std::collections::BTreeMap;
//!
//! let mut params = BTreeMap::new();
//! params.insert("id".to_string(), "123".to_string());
//!
//! let context = RequestContext::new(params);
//! assert_eq!(context.path_params.get("id"), Some(&"123".to_string()));
//! ```

use crate::PathParams;

/// Type-safe context for request-scoped data
///
/// This struct holds data extracted from the request that needs to be
/// available to handlers and extractors. App-scoped data (like state)
/// remains accessible through the inner request.
#[derive(Debug, Clone, Default)]
pub struct RequestContext {
    /// Path parameters extracted from route matching
    pub path_params: PathParams,
    // Future additions should be Options with defaults:
    // pub request_id: Option<Uuid>,
    // pub auth: Option<AuthContext>,
}

impl RequestContext {
    /// Create new context with path parameters
    #[must_use]
    pub const fn new(path_params: PathParams) -> Self {
        Self { path_params }
    }

    /// Builder method for setting path parameters
    #[must_use]
    pub fn with_path_params(mut self, params: PathParams) -> Self {
        self.path_params = params;
        self
    }
}
