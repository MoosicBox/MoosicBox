//! Structured expressions for persistent schema predicates and trigger guards.

use crate::DatabaseError;

/// A persistent predicate with no SQL-fragment inputs.
#[derive(Debug, Clone)]
pub enum SchemaExpression {
    /// An exact column, optionally qualified by an exact table name.
    Column {
        table: Option<String>,
        column: String,
    },
    /// A trigger's new row column.
    New(String),
    /// A trigger's old row column.
    Old(String),
    /// A string constant.
    Text(String),
    /// An integer constant.
    Integer(i64),
    /// SQL NULL.
    Null,
    /// Typed comparison.
    Compare(Box<Self>, SchemaComparison, Box<Self>),
    /// Membership in a typed persistent value list.
    In(Box<Self>, Vec<Self>),
    /// Conjunction.
    And(Vec<Self>),
    /// Disjunction.
    Or(Vec<Self>),
    /// Correlated existence test over an exact table.
    Exists { table: String, predicate: Box<Self> },
}

/// Comparison operators allowed in persistent schema predicates.
#[derive(Debug, Clone, Copy)]
pub enum SchemaComparison {
    /// Equality (including SQL NULL semantics).
    Equal,
    /// Inequality.
    NotEqual,
    /// Greater than.
    Greater,
    /// Greater than or equal.
    GreaterEqual,
    /// Less than.
    Less,
    /// Less than or equal.
    LessEqual,
    /// NULL-safe equality.
    Is,
    /// NULL-safe inequality.
    IsNot,
}

impl SchemaExpression {
    /// Reference a trigger's new row by exact column name.
    #[must_use]
    pub fn new_column(column: impl Into<String>) -> Self {
        Self::New(column.into())
    }

    /// Reference a trigger's old row by exact column name.
    #[must_use]
    pub fn old_column(column: impl Into<String>) -> Self {
        Self::Old(column.into())
    }

    /// An exact unqualified column reference.
    #[must_use]
    pub fn column(column: impl Into<String>) -> Self {
        Self::Column {
            table: None,
            column: column.into(),
        }
    }

    /// An exact table-qualified column reference.
    #[must_use]
    pub fn qualified(table: impl Into<String>, column: impl Into<String>) -> Self {
        Self::Column {
            table: Some(table.into()),
            column: column.into(),
        }
    }

    /// Compare this expression to another typed expression.
    #[must_use]
    pub fn compare(self, comparison: SchemaComparison, right: Self) -> Self {
        Self::Compare(Box::new(self), comparison, Box::new(right))
    }

    /// Test for an existing row satisfying a correlated predicate.
    #[must_use]
    pub fn exists(table: impl Into<String>, predicate: Self) -> Self {
        Self::Exists {
            table: table.into(),
            predicate: Box::new(predicate),
        }
    }

    pub(crate) fn render(&self, quote: char) -> Result<String, DatabaseError> {
        let identifier = |name: &str| {
            format!(
                "{quote}{}{quote}",
                name.replace(quote, &format!("{quote}{quote}"))
            )
        };
        Ok(match self {
            Self::Column { table, column } => table.as_ref().map_or_else(
                || identifier(column),
                |table| format!("{}.{}", identifier(table), identifier(column)),
            ),
            Self::New(column) => format!("NEW.{}", identifier(column)),
            Self::Old(column) => format!("OLD.{}", identifier(column)),
            Self::Text(value) => {
                if value.contains('\0') {
                    return Err(DatabaseError::InvalidSchema(
                        "NUL in schema constant".into(),
                    ));
                }
                format!("'{}'", value.replace('\'', "''"))
            }
            Self::Integer(value) => value.to_string(),
            Self::Null => "NULL".into(),
            Self::Compare(left, comparison, right) => format!(
                "({} {} {})",
                left.render(quote)?,
                match comparison {
                    SchemaComparison::Equal => "=",
                    SchemaComparison::NotEqual => "!=",
                    SchemaComparison::Greater => ">",
                    SchemaComparison::GreaterEqual => ">=",
                    SchemaComparison::Less => "<",
                    SchemaComparison::LessEqual => "<=",
                    SchemaComparison::Is => "IS",
                    SchemaComparison::IsNot => "IS NOT",
                },
                right.render(quote)?
            ),
            Self::In(left, values) => {
                if values.is_empty() {
                    return Err(DatabaseError::InvalidSchema(
                        "Empty persistent IN list".into(),
                    ));
                }
                format!(
                    "({} IN ({}))",
                    left.render(quote)?,
                    values
                        .iter()
                        .map(|value| value.render(quote))
                        .collect::<Result<Vec<_>, _>>()?
                        .join(", ")
                )
            }
            Self::And(values) | Self::Or(values) => {
                if values.is_empty() {
                    return Err(DatabaseError::InvalidSchema(
                        "Empty schema boolean expression".into(),
                    ));
                }
                let separator = if matches!(self, Self::And(_)) {
                    " AND "
                } else {
                    " OR "
                };
                format!(
                    "({})",
                    values
                        .iter()
                        .map(|value| value.render(quote))
                        .collect::<Result<Vec<_>, _>>()?
                        .join(separator)
                )
            }
            Self::Exists { table, predicate } => format!(
                "EXISTS (SELECT 1 FROM {} WHERE {})",
                identifier(table),
                predicate.render(quote)?
            ),
        })
    }
}
