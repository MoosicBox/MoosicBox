/// Simplified form submission model used by the test harness.
#[derive(Debug, Clone, Default)]
pub struct FormSubmission {
    /// Optional `hx-*` route that runs first.
    pub hx_route: Option<String>,
    /// Optional standard form action route that runs after `hx-*`.
    pub action_route: Option<String>,
}

impl FormSubmission {
    /// Creates a submission with no routes.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            hx_route: None,
            action_route: None,
        }
    }

    /// Sets the `hx-*` route.
    #[must_use]
    pub fn with_hx_route(mut self, route: impl Into<String>) -> Self {
        self.hx_route = Some(route.into());
        self
    }

    /// Sets the action route.
    #[must_use]
    pub fn with_action_route(mut self, route: impl Into<String>) -> Self {
        self.action_route = Some(route.into());
        self
    }
}
