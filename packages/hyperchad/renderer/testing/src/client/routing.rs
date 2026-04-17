use std::collections::BTreeMap;

use hyperchad_renderer::View;

/// Full + optional partial route response pair.
#[derive(Debug, Clone)]
pub struct RouteResponse {
    pub full: View,
    pub partial: Option<View>,
}

impl RouteResponse {
    /// Creates a response with full content only.
    #[must_use]
    pub fn full(view: View) -> Self {
        Self {
            full: view,
            partial: None,
        }
    }

    /// Creates a response with explicit full and partial views.
    #[must_use]
    pub fn full_and_partial(full: View, partial: View) -> Self {
        Self {
            full,
            partial: Some(partial),
        }
    }
}

/// In-memory route table used by the harness.
#[derive(Debug, Clone, Default)]
pub struct RouteTable {
    routes: BTreeMap<String, RouteResponse>,
}

impl RouteTable {
    /// Inserts a route with full view only.
    pub fn insert_full(&mut self, path: impl Into<String>, full: View) {
        self.routes.insert(path.into(), RouteResponse::full(full));
    }

    /// Inserts a route with explicit full and partial views.
    pub fn insert_full_and_partial(&mut self, path: impl Into<String>, full: View, partial: View) {
        self.routes
            .insert(path.into(), RouteResponse::full_and_partial(full, partial));
    }

    /// Returns the route view for the requested mode.
    #[must_use]
    pub fn resolve(&self, path: &str, htmx_request: bool) -> Option<View> {
        let response = self.routes.get(path)?;
        if htmx_request {
            response
                .partial
                .as_ref()
                .cloned()
                .or_else(|| Some(response.full.clone()))
        } else {
            Some(response.full.clone())
        }
    }

    /// Returns true if the path exists.
    #[must_use]
    pub fn contains(&self, path: &str) -> bool {
        self.routes.contains_key(path)
    }
}
