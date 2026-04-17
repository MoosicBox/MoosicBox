/// Custom event payload for harness-driven event dispatch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomEvent {
    pub name: String,
    pub value: Option<String>,
}

impl CustomEvent {
    /// Creates a new custom event.
    #[must_use]
    pub fn new(name: impl Into<String>, value: Option<String>) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }
}
