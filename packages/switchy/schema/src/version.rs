pub const DEFAULT_MIGRATIONS_TABLE: &str = "__switchy_migrations";

pub struct VersionTracker {
    table_name: String,
}

impl VersionTracker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            table_name: DEFAULT_MIGRATIONS_TABLE.to_string(),
        }
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_table_name(table_name: String) -> Self {
        Self { table_name }
    }

    #[must_use]
    pub fn table_name(&self) -> &str {
        &self.table_name
    }
}

impl Default for VersionTracker {
    fn default() -> Self {
        Self::new()
    }
}
