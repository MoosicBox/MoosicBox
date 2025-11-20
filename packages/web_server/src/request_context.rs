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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test_log::test]
    fn test_request_context_new() {
        let mut params = BTreeMap::new();
        params.insert("id".to_string(), "123".to_string());

        let context = RequestContext::new(params.clone());
        assert_eq!(context.path_params, params);
    }

    #[test_log::test]
    fn test_request_context_default() {
        let context = RequestContext::default();
        assert!(context.path_params.is_empty());
    }

    #[test_log::test]
    fn test_request_context_with_path_params() {
        let mut params1 = BTreeMap::new();
        params1.insert("id".to_string(), "123".to_string());

        let mut params2 = BTreeMap::new();
        params2.insert("user_id".to_string(), "456".to_string());
        params2.insert("post_id".to_string(), "789".to_string());

        let context = RequestContext::new(params1).with_path_params(params2.clone());

        assert_eq!(context.path_params, params2);
        assert_eq!(context.path_params.get("user_id"), Some(&"456".to_string()));
        assert_eq!(context.path_params.get("post_id"), Some(&"789".to_string()));
    }

    #[test_log::test]
    fn test_request_context_path_params_empty() {
        let context = RequestContext::new(BTreeMap::new());
        assert!(context.path_params.is_empty());
    }

    #[test_log::test]
    fn test_request_context_path_params_multiple() {
        let mut params = BTreeMap::new();
        params.insert("category".to_string(), "electronics".to_string());
        params.insert("product_id".to_string(), "12345".to_string());
        params.insert("variant".to_string(), "blue".to_string());

        let context = RequestContext::new(params.clone());

        assert_eq!(context.path_params.len(), 3);
        assert_eq!(
            context.path_params.get("category"),
            Some(&"electronics".to_string())
        );
        assert_eq!(
            context.path_params.get("product_id"),
            Some(&"12345".to_string())
        );
        assert_eq!(
            context.path_params.get("variant"),
            Some(&"blue".to_string())
        );
    }

    #[test_log::test]
    fn test_request_context_clone() {
        let mut params = BTreeMap::new();
        params.insert("id".to_string(), "123".to_string());

        let context1 = RequestContext::new(params);
        let context2 = context1.clone();

        assert_eq!(context1.path_params, context2.path_params);
    }

    #[test_log::test]
    fn test_request_context_builder_chaining() {
        let mut params = BTreeMap::new();
        params.insert("id".to_string(), "999".to_string());

        let context = RequestContext::default().with_path_params(params.clone());

        assert_eq!(context.path_params, params);
    }
}
