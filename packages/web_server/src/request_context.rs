use crate::simulator::PathParams;

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
