//! Typed SQLite trigger definitions.

use super::SchemaExpression;
use crate::{Database, DatabaseError};

/// A BEFORE trigger event.
#[derive(Debug, Clone)]
pub enum TriggerEvent {
    /// Insertion.
    Insert,
    /// Any update, or updates of the listed exact columns.
    Update(Vec<String>),
    /// Deletion.
    Delete,
}

/// A trigger's supported control-flow action.
#[derive(Debug, Clone)]
pub enum TriggerAction {
    /// Abort the statement with this literal diagnostic.
    Abort(String),
    /// Ignore the triggering operation.
    Ignore,
}

/// A typed persistent trigger guard. No caller SQL is accepted.
#[derive(Debug, Clone)]
pub struct CreateTriggerStatement {
    /// Exact trigger name.
    pub name: String,
    /// Exact target table.
    pub table: String,
    /// Event, required before execution.
    pub event: Option<TriggerEvent>,
    /// Optional row predicate.
    pub predicate: Option<SchemaExpression>,
    /// Action, required before execution.
    pub action: Option<TriggerAction>,
    /// Preserve an existing trigger with the same name.
    pub if_not_exists: bool,
    /// Connection-local TEMP trigger instead of a persistent trigger.
    pub temporary: bool,
}

/// Construct a typed trigger guard.
#[must_use]
pub fn create_trigger(name: impl Into<String>, table: impl Into<String>) -> CreateTriggerStatement {
    CreateTriggerStatement {
        name: name.into(),
        table: table.into(),
        event: None,
        predicate: None,
        action: None,
        if_not_exists: false,
        temporary: false,
    }
}

impl CreateTriggerStatement {
    /// Create a connection-local TEMP guard.
    #[must_use]
    pub const fn temporary(mut self, enabled: bool) -> Self {
        self.temporary = enabled;
        self
    }
    /// Fire before insertion.
    #[must_use]
    pub fn before_insert(mut self) -> Self {
        self.event = Some(TriggerEvent::Insert);
        self
    }
    /// Fire before any update.
    #[must_use]
    pub fn before_update(mut self) -> Self {
        self.event = Some(TriggerEvent::Update(Vec::new()));
        self
    }
    /// Fire before updates naming these exact columns.
    #[must_use]
    pub fn before_update_of(
        mut self,
        columns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.event = Some(TriggerEvent::Update(
            columns.into_iter().map(Into::into).collect(),
        ));
        self
    }
    /// Fire before deletion.
    #[must_use]
    pub fn before_delete(mut self) -> Self {
        self.event = Some(TriggerEvent::Delete);
        self
    }
    /// Limit execution to this typed predicate.
    #[must_use]
    pub fn when(mut self, predicate: SchemaExpression) -> Self {
        self.predicate = Some(predicate);
        self
    }
    /// Abort the statement with a literal diagnostic.
    #[must_use]
    pub fn raise_abort(mut self, message: impl Into<String>) -> Self {
        self.action = Some(TriggerAction::Abort(message.into()));
        self
    }
    /// Ignore the operation without an error.
    #[must_use]
    pub fn raise_ignore(mut self) -> Self {
        self.action = Some(TriggerAction::Ignore);
        self
    }
    /// Preserve an existing trigger with the same name.
    #[must_use]
    pub const fn if_not_exists(mut self, enabled: bool) -> Self {
        self.if_not_exists = enabled;
        self
    }
    /// Install the trigger on this database or transaction.
    ///
    /// # Errors
    /// * Returns invalid schema for missing events/actions or invalid constants.
    /// * Returns backend execution errors or unsupported operation.
    pub async fn execute(self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_create_trigger(&self).await
    }

    pub(crate) fn render_sqlite(&self) -> Result<String, DatabaseError> {
        let quote = |name: &str| format!("\"{}\"", name.replace('"', "\"\""));
        let event = match &self.event {
            Some(TriggerEvent::Insert) => "INSERT".into(),
            Some(TriggerEvent::Delete) => "DELETE".into(),
            Some(TriggerEvent::Update(columns)) if columns.is_empty() => "UPDATE".into(),
            Some(TriggerEvent::Update(columns)) => format!(
                "UPDATE OF {}",
                columns
                    .iter()
                    .map(|name| quote(name))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None => return Err(DatabaseError::InvalidSchema("Missing trigger event".into())),
        };
        let action = match &self.action {
            Some(TriggerAction::Abort(message)) => format!(
                "RAISE(ABORT, {})",
                SchemaExpression::Text(message.clone()).render('"')?
            ),
            Some(TriggerAction::Ignore) => "RAISE(IGNORE)".into(),
            None => {
                return Err(DatabaseError::InvalidSchema(
                    "Missing trigger action".into(),
                ));
            }
        };
        let predicate = self
            .predicate
            .as_ref()
            .map(|predicate| predicate.render('"').map(|sql| format!(" WHEN {sql}")))
            .transpose()?
            .unwrap_or_default();
        Ok(format!(
            "CREATE {}TRIGGER {}{} BEFORE {event} ON {}{predicate} BEGIN SELECT {action}; END",
            if self.temporary { "TEMP " } else { "" },
            if self.if_not_exists {
                "IF NOT EXISTS "
            } else {
                ""
            },
            quote(&self.name),
            quote(&self.table)
        ))
    }
}
