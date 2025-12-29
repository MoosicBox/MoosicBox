//! Query transformation utilities for handling non-bindable parameters
//!
//! This module provides utilities to transform SQL queries by replacing
//! placeholder parameters with SQL expressions for values that cannot be
//! bound as regular parameters (e.g., `NOW()`, database functions).

use crate::DatabaseValue;

#[cfg(any(
    feature = "placeholder-dollar-number",
    feature = "placeholder-at-number",
    feature = "placeholder-colon-number"
))]
use regex::Regex;

#[cfg(any(
    feature = "placeholder-dollar-number",
    feature = "placeholder-at-number",
    feature = "placeholder-colon-number"
))]
use std::sync::LazyLock;

/// Transform a SQL query by replacing non-bindable parameters with SQL expressions
///
/// # Arguments
/// * `query` - The SQL query with placeholders
/// * `params` - The parameters for the query
/// * `handler` - The placeholder handler for the SQL dialect
/// * `to_sql` - Function that converts a `DatabaseValue` to SQL expression (returns None for bindable params)
///
/// # Returns
/// * `Ok((transformed_query, bindable_params))` - The transformed query and remaining bindable parameters
/// * `Err(String)` - Error message if transformation fails
///
/// # Errors
///
/// * Returns `Err(String)` if placeholder replacement fails
/// * Returns `Err(String)` if renumbering of remaining placeholders fails
/// * Error messages indicate which placeholder index could not be found or what went wrong during transformation
///
/// # Example
/// ```
/// use switchy_database::query_transform::{transform_query_for_params, QuestionMarkHandler};
/// use switchy_database::DatabaseValue;
///
/// let query = "INSERT INTO users (created_at, name) VALUES (?, ?)";
/// let params = vec![DatabaseValue::Now, DatabaseValue::String("Alice".to_string())];
///
/// let (transformed, bindable) = transform_query_for_params(
///     query,
///     &params,
///     &QuestionMarkHandler,
///     |param| match param {
///         DatabaseValue::Now => Some("NOW()".to_string()),
///         _ => None,
///     }
/// ).unwrap();
///
/// assert_eq!(transformed, "INSERT INTO users (created_at, name) VALUES (NOW(), ?)");
/// assert_eq!(bindable.len(), 1);
/// ```
pub fn transform_query_for_params<H, F>(
    query: &str,
    params: &[DatabaseValue],
    handler: &H,
    to_sql: F,
) -> Result<(String, Vec<DatabaseValue>), String>
where
    H: PlaceholderHandler,
    F: Fn(&DatabaseValue) -> Option<String>,
{
    let mut filtered_params = Vec::new();
    let mut replacements = Vec::new();

    // Determine what needs replacing vs what can be bound
    for (idx, param) in params.iter().enumerate() {
        match to_sql(param) {
            Some(sql_expr) => replacements.push((idx, sql_expr)),
            None => filtered_params.push(param.clone()),
        }
    }

    // Use the handler to apply replacements
    let transformed = if replacements.is_empty() {
        query.to_string()
    } else {
        let replaced = handler.replace_placeholders(query, &replacements)?;
        handler.renumber_remaining(&replaced)?
    };

    Ok((transformed, filtered_params))
}

/// Trait for handling different placeholder styles in SQL queries
pub trait PlaceholderHandler {
    /// Replace placeholders in the query with SQL expressions
    ///
    /// # Errors
    ///
    /// * Returns `Err(String)` if a placeholder at the specified index cannot be found in the query
    /// * Returns `Err(String)` if the replacement would result in invalid SQL syntax
    fn replace_placeholders(
        &self,
        query: &str,
        replacements: &[(usize, String)],
    ) -> Result<String, String>;

    /// Optionally renumber remaining placeholders after replacements
    /// Default implementation does nothing (for ? style that doesn't need renumbering)
    ///
    /// # Errors
    ///
    /// * Returns `Err(String)` if placeholder renumbering fails
    /// * Default implementation never fails (returns `Ok`)
    fn renumber_remaining(&self, query: &str) -> Result<String, String> {
        Ok(query.to_string())
    }
}

// ===== Base Numbered Handler Trait =====

/// Base trait for numbered placeholder handlers (e.g., $1, @p1, :1)
/// Provides common implementation for replace and renumber operations
#[cfg(any(
    feature = "placeholder-dollar-number",
    feature = "placeholder-at-number",
    feature = "placeholder-colon-number"
))]
trait NumberedPlaceholderHandler: PlaceholderHandler {
    /// Get the prefix for this placeholder style (e.g., "$", "@p", ":")
    fn prefix(&self) -> &str;

    /// Get the regex pattern for matching this placeholder style
    ///
    /// # Errors
    ///
    /// * This function does not return a Result, but the regex pattern must be valid
    /// * Implementations should ensure the regex compiles successfully
    fn regex(&self) -> &Regex;

    /// Format a placeholder with the given number
    fn format_placeholder(&self, number: usize) -> String {
        format!("{}{}", self.prefix(), number)
    }
}

// Default implementation for all numbered handlers
#[cfg(any(
    feature = "placeholder-dollar-number",
    feature = "placeholder-at-number",
    feature = "placeholder-colon-number"
))]
impl<T: NumberedPlaceholderHandler> PlaceholderHandler for T {
    fn replace_placeholders(
        &self,
        query: &str,
        replacements: &[(usize, String)],
    ) -> Result<String, String> {
        let mut result = query.to_string();

        // Replace each numbered placeholder with its SQL expression
        for (idx, sql_expr) in replacements {
            let placeholder = self.format_placeholder(idx + 1);
            result = result.replace(&placeholder, sql_expr);
        }

        Ok(result)
    }

    fn renumber_remaining(&self, query: &str) -> Result<String, String> {
        let mut counter = 0;
        let result = self.regex().replace_all(query, |_caps: &regex::Captures| {
            counter += 1;
            self.format_placeholder(counter)
        });

        Ok(result.into_owned())
    }
}

// ===== Question Mark Handler (SQLite, MySQL) =====

/// Handler for ? placeholders (`SQLite`, `MySQL`)
/// Enabled when `SQLite` or `MySQL` backends are used
#[cfg(feature = "placeholder-question-mark")]
#[derive(Debug, Clone, Default)]
pub struct QuestionMarkHandler;

#[cfg(feature = "placeholder-question-mark")]
impl PlaceholderHandler for QuestionMarkHandler {
    fn replace_placeholders(
        &self,
        query: &str,
        replacements: &[(usize, String)],
    ) -> Result<String, String> {
        log::trace!("replace_placeholders: query={query} replacements={replacements:?}");
        let mut result = query.to_string();

        // Work backwards to preserve indices
        for (idx, sql_expr) in replacements.iter().rev() {
            let mut count = 0;
            let mut new_result = String::new();
            let mut found = false;

            for ch in result.chars() {
                if ch == '?' {
                    if count == *idx {
                        new_result.push_str(sql_expr);
                        found = true;
                    } else {
                        new_result.push(ch);
                    }
                    count += 1;
                } else {
                    new_result.push(ch);
                }
            }

            if !found {
                return Err(format!("Could not find placeholder at index {idx}"));
            }

            result = new_result;
        }

        Ok(result)
    }

    // No renumbering needed for ? style - uses default implementation
}

// ===== Dollar Number Handler (PostgreSQL) =====

#[cfg(feature = "placeholder-dollar-number")]
static DOLLAR_NUM_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\$(\d+)").unwrap());

/// Handler for $1, $2 placeholders (`PostgreSQL`)
#[cfg(feature = "placeholder-dollar-number")]
#[derive(Debug, Clone, Default)]
pub struct DollarNumberHandler;

#[cfg(feature = "placeholder-dollar-number")]
impl NumberedPlaceholderHandler for DollarNumberHandler {
    fn prefix(&self) -> &'static str {
        "$"
    }

    fn regex(&self) -> &Regex {
        &DOLLAR_NUM_REGEX
    }
}

// ===== At Number Handler (SQL Server style) =====

#[cfg(feature = "placeholder-at-number")]
static AT_NUM_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"@p(\d+)").unwrap());

/// Handler for @p1, @p2 placeholders (SQL Server style)
#[cfg(feature = "placeholder-at-number")]
#[derive(Debug, Clone, Default)]
pub struct AtNumberHandler;

#[cfg(feature = "placeholder-at-number")]
impl NumberedPlaceholderHandler for AtNumberHandler {
    fn prefix(&self) -> &'static str {
        "@p"
    }
    fn regex(&self) -> &Regex {
        &AT_NUM_REGEX
    }
}

// ===== Colon Number Handler (Oracle style) =====

#[cfg(feature = "placeholder-colon-number")]
static COLON_NUM_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r":(\d+)").unwrap());

/// Handler for :1, :2 placeholders (Oracle style)
#[cfg(feature = "placeholder-colon-number")]
#[derive(Debug, Clone, Default)]
pub struct ColonNumberHandler;

#[cfg(feature = "placeholder-colon-number")]
impl NumberedPlaceholderHandler for ColonNumberHandler {
    fn prefix(&self) -> &'static str {
        ":"
    }
    fn regex(&self) -> &Regex {
        &COLON_NUM_REGEX
    }
}

// ===== Named Colon Handler (special case - not numbered) =====

/// Handler for :name placeholders (some ORMs)
#[cfg(feature = "placeholder-named-colon")]
#[derive(Debug, Clone)]
pub struct NamedColonHandler {
    param_names: Vec<String>,
}

#[cfg(feature = "placeholder-named-colon")]
impl NamedColonHandler {
    /// Creates a new `NamedColonHandler` with the given parameter names
    ///
    /// # Arguments
    ///
    /// * `param_names` - Vector of parameter names in order. Names should not include the ':' prefix.
    ///
    /// # Examples
    ///
    /// ```
    /// # use switchy_database::query_transform::NamedColonHandler;
    /// let handler = NamedColonHandler::new(vec!["user_id".to_string(), "name".to_string()]);
    /// ```
    #[must_use]
    pub const fn new(param_names: Vec<String>) -> Self {
        Self { param_names }
    }
}

#[cfg(feature = "placeholder-named-colon")]
impl PlaceholderHandler for NamedColonHandler {
    fn replace_placeholders(
        &self,
        query: &str,
        replacements: &[(usize, String)],
    ) -> Result<String, String> {
        let mut result = query.to_string();

        for (idx, sql_expr) in replacements {
            if let Some(name) = self.param_names.get(*idx) {
                let placeholder = format!(":{name}");
                result = result.replace(&placeholder, sql_expr);
            } else {
                return Err(format!("No parameter name for index {idx}"));
            }
        }

        Ok(result)
    }

    // No renumbering for named parameters
}

// ===== Tests =====

#[cfg(test)]
mod tests {
    use super::*;

    // ===== QuestionMarkHandler Tests =====

    #[cfg(feature = "placeholder-question-mark")]
    mod question_mark_tests {
        use super::*;

        #[test]
        fn test_simple_replacement() {
            let handler = QuestionMarkHandler;
            let query = "SELECT * FROM users WHERE created_at > ? AND status = ?";
            let replacements = vec![(0, "NOW()".to_string())];

            let result = handler.replace_placeholders(query, &replacements).unwrap();
            assert_eq!(
                result,
                "SELECT * FROM users WHERE created_at > NOW() AND status = ?"
            );
        }

        #[test]
        fn test_multiple_replacements() {
            let handler = QuestionMarkHandler;
            let query = "INSERT INTO logs (time, user, action, data) VALUES (?, ?, ?, ?)";
            let replacements = vec![(0, "NOW()".to_string()), (2, "'system'".to_string())];

            let result = handler.replace_placeholders(query, &replacements).unwrap();
            assert_eq!(
                result,
                "INSERT INTO logs (time, user, action, data) VALUES (NOW(), ?, 'system', ?)"
            );
        }

        #[test]
        fn test_all_replaced() {
            let handler = QuestionMarkHandler;
            let query = "SELECT ? AS now, ? AS tomorrow";
            let replacements = vec![
                (0, "NOW()".to_string()),
                (1, "DATE_ADD(NOW(), INTERVAL 1 DAY)".to_string()),
            ];

            let result = handler.replace_placeholders(query, &replacements).unwrap();
            assert_eq!(
                result,
                "SELECT NOW() AS now, DATE_ADD(NOW(), INTERVAL 1 DAY) AS tomorrow"
            );
        }

        #[test]
        fn test_index_out_of_bounds() {
            let handler = QuestionMarkHandler;
            let query = "SELECT ? AS value";
            let replacements = vec![(1, "NOW()".to_string())]; // Index 1 doesn't exist

            let result = handler.replace_placeholders(query, &replacements);
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("index"));
        }

        #[test]
        fn test_no_renumbering() {
            let handler = QuestionMarkHandler;
            let query = "SELECT ?, ?, ?";

            // Question marks don't get renumbered
            let result = handler.renumber_remaining(query).unwrap();
            assert_eq!(result, query);
        }

        #[test]
        fn test_transform_integration() {
            let query = "INSERT INTO events (timestamp, user, action) VALUES (?, ?, ?)";
            let params = vec![
                DatabaseValue::Now,
                DatabaseValue::String("alice".to_string()),
                DatabaseValue::String("login".to_string()),
            ];

            let (transformed, bindable) = transform_query_for_params(
                query,
                &params,
                &QuestionMarkHandler,
                |param| match param {
                    DatabaseValue::Now => Some("NOW()".to_string()),
                    _ => None,
                },
            )
            .unwrap();

            assert_eq!(
                transformed,
                "INSERT INTO events (timestamp, user, action) VALUES (NOW(), ?, ?)"
            );
            assert_eq!(bindable.len(), 2);
            assert_eq!(bindable[0], DatabaseValue::String("alice".to_string()));
            assert_eq!(bindable[1], DatabaseValue::String("login".to_string()));
        }
    }

    // ===== DollarNumberHandler Tests =====

    #[cfg(feature = "placeholder-dollar-number")]
    mod dollar_number_tests {
        use super::*;

        #[test]
        fn test_base_implementation() {
            let handler = DollarNumberHandler;

            // Test that the base trait properly formats placeholders
            assert_eq!(handler.format_placeholder(1), "$1");
            assert_eq!(handler.format_placeholder(42), "$42");
        }

        #[test]
        fn test_simple_replacement() {
            let handler = DollarNumberHandler;
            let query = "SELECT * FROM users WHERE id = $1 AND created_at > $2";
            let replacements = vec![(1, "NOW()".to_string())];

            let result = handler.replace_placeholders(query, &replacements).unwrap();
            assert_eq!(
                result,
                "SELECT * FROM users WHERE id = $1 AND created_at > NOW()"
            );
        }

        #[test]
        fn test_renumbering() {
            let handler = DollarNumberHandler;
            let after_replace = "INSERT INTO data (a, b, c) VALUES ($1, NOW(), $3)";

            let result = handler.renumber_remaining(after_replace).unwrap();
            assert_eq!(result, "INSERT INTO data (a, b, c) VALUES ($1, NOW(), $2)");
        }

        #[test]
        fn test_complex_renumbering() {
            let handler = DollarNumberHandler;
            let after_replace = "SELECT $1, NOW(), CURRENT_USER, $4, DEFAULT, $6";

            let result = handler.renumber_remaining(after_replace).unwrap();
            assert_eq!(result, "SELECT $1, NOW(), CURRENT_USER, $2, DEFAULT, $3");
        }

        #[test]
        fn test_non_sequential() {
            let handler = DollarNumberHandler;
            let query = "SELECT $1, $3, $5";

            let result = handler.renumber_remaining(query).unwrap();
            assert_eq!(result, "SELECT $1, $2, $3");
        }

        #[test]
        fn test_transform_integration() {
            let query = "UPDATE users SET updated_at = $1, status = $2 WHERE id = $3";
            let params = vec![
                DatabaseValue::Now,
                DatabaseValue::String("active".to_string()),
                DatabaseValue::Int64(42),
            ];

            let (transformed, bindable) = transform_query_for_params(
                query,
                &params,
                &DollarNumberHandler,
                |param| match param {
                    DatabaseValue::Now => Some("NOW()".to_string()),
                    _ => None,
                },
            )
            .unwrap();

            assert_eq!(
                transformed,
                "UPDATE users SET updated_at = NOW(), status = $1 WHERE id = $2"
            );
            assert_eq!(bindable.len(), 2);
        }
    }

    // ===== AtNumberHandler Tests =====

    #[cfg(feature = "placeholder-at-number")]
    mod at_number_tests {
        use super::*;

        #[test]
        fn test_base_implementation() {
            let handler = AtNumberHandler;

            // Test that the base trait properly formats placeholders
            assert_eq!(handler.format_placeholder(1), "@p1");
            assert_eq!(handler.format_placeholder(99), "@p99");
        }

        #[test]
        fn test_replacement() {
            let handler = AtNumberHandler;
            let query = "EXEC stored_proc @p1, @p2, @p3";
            let replacements = vec![(0, "GETDATE()".to_string())];

            let result = handler.replace_placeholders(query, &replacements).unwrap();
            assert_eq!(result, "EXEC stored_proc GETDATE(), @p2, @p3");
        }

        #[test]
        fn test_renumbering() {
            let handler = AtNumberHandler;
            let query = "EXEC stored_proc GETDATE(), @p2, @p3";

            let result = handler.renumber_remaining(query).unwrap();
            assert_eq!(result, "EXEC stored_proc GETDATE(), @p1, @p2");
        }
    }

    // ===== ColonNumberHandler Tests =====

    #[cfg(feature = "placeholder-colon-number")]
    mod colon_number_tests {
        use super::*;

        #[test]
        fn test_base_implementation() {
            let handler = ColonNumberHandler;

            // Test that the base trait properly formats placeholders
            assert_eq!(handler.format_placeholder(1), ":1");
            assert_eq!(handler.format_placeholder(10), ":10");
        }

        #[test]
        fn test_replacement() {
            let handler = ColonNumberHandler;
            let query = "SELECT :1, :2, :3 FROM dual";
            let replacements = vec![(1, "SYSDATE".to_string())];

            let result = handler.replace_placeholders(query, &replacements).unwrap();
            assert_eq!(result, "SELECT :1, SYSDATE, :3 FROM dual");
        }
    }

    // ===== NamedColonHandler Tests =====

    #[cfg(feature = "placeholder-named-colon")]
    mod named_colon_tests {
        use super::*;

        #[test]
        fn test_named_replacement() {
            let handler = NamedColonHandler::new(vec![
                "timestamp".to_string(),
                "user".to_string(),
                "action".to_string(),
            ]);

            let query = "INSERT INTO logs (time, user, action) VALUES (:timestamp, :user, :action)";
            let replacements = vec![(0, "NOW()".to_string())];

            let result = handler.replace_placeholders(query, &replacements).unwrap();
            assert_eq!(
                result,
                "INSERT INTO logs (time, user, action) VALUES (NOW(), :user, :action)"
            );
        }

        #[test]
        fn test_named_index_error() {
            let handler = NamedColonHandler::new(vec!["param1".to_string()]);

            let query = "SELECT :param1, :param2";
            let replacements = vec![(1, "VALUE".to_string())]; // Index 1 doesn't exist

            let result = handler.replace_placeholders(query, &replacements);
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("No parameter name"));
        }
    }

    // ===== Cross-cutting Integration Tests =====

    #[test]
    fn test_empty_params() {
        // Mock handler for testing
        struct MockHandler;
        impl PlaceholderHandler for MockHandler {
            fn replace_placeholders(
                &self,
                query: &str,
                _: &[(usize, String)],
            ) -> Result<String, String> {
                Ok(query.to_string())
            }
        }

        let query = "SELECT * FROM users";
        let params = vec![];

        let (transformed, bindable) =
            transform_query_for_params(query, &params, &MockHandler, |_| None).unwrap();

        assert_eq!(transformed, query);
        assert_eq!(bindable.len(), 0);
    }

    #[test]
    fn test_no_replacements_needed() {
        struct MockHandler;
        impl PlaceholderHandler for MockHandler {
            fn replace_placeholders(
                &self,
                query: &str,
                _: &[(usize, String)],
            ) -> Result<String, String> {
                Ok(query.to_string())
            }
        }

        let query = "INSERT INTO users (name) VALUES (?)";
        let params = vec![DatabaseValue::String("Alice".to_string())];

        let (transformed, bindable) = transform_query_for_params(
            query,
            &params,
            &MockHandler,
            |_| None, // Nothing needs SQL replacement
        )
        .unwrap();

        assert_eq!(transformed, query);
        assert_eq!(bindable.len(), 1);
    }

    #[test]
    fn test_all_params_replaced() {
        struct MockHandler;
        impl PlaceholderHandler for MockHandler {
            fn replace_placeholders(
                &self,
                _query: &str,
                _: &[(usize, String)],
            ) -> Result<String, String> {
                Ok("SELECT NOW(), NOW()".to_string())
            }
        }

        let query = "SELECT ?, ?";
        let params = vec![DatabaseValue::Now, DatabaseValue::Now];

        let (transformed, bindable) =
            transform_query_for_params(query, &params, &MockHandler, |param| match param {
                DatabaseValue::Now => Some("NOW()".to_string()),
                _ => None,
            })
            .unwrap();

        assert_eq!(transformed, "SELECT NOW(), NOW()");
        assert_eq!(bindable.len(), 0);
    }

    // Test that multiple handlers can coexist when multiple features are enabled
    #[cfg(all(
        feature = "placeholder-dollar-number",
        feature = "placeholder-at-number"
    ))]
    #[test]
    fn test_multiple_handlers_coexist() {
        let dollar_handler = DollarNumberHandler;
        let at_handler = AtNumberHandler;

        // Each handler works with its own query style
        let pg_query = "SELECT $1, $2";
        let sql_server_query = "SELECT @p1, @p2";

        let replacements = vec![(0, "CURRENT_TIMESTAMP".to_string())];

        let pg_result = dollar_handler
            .replace_placeholders(pg_query, &replacements)
            .unwrap();
        assert_eq!(pg_result, "SELECT CURRENT_TIMESTAMP, $2");

        let sql_result = at_handler
            .replace_placeholders(sql_server_query, &replacements)
            .unwrap();
        assert_eq!(sql_result, "SELECT CURRENT_TIMESTAMP, @p2");
    }

    // ===== Performance/Stress Tests =====

    #[cfg(feature = "placeholder-question-mark")]
    #[test]
    fn test_large_query_many_params() {
        let mut query_parts = Vec::new();
        let mut params = Vec::new();

        // Create a query with 100 parameters
        for i in 0..100 {
            query_parts.push("?");

            // Every 3rd param is NOW()
            if i % 3 == 0 {
                params.push(DatabaseValue::Now);
            } else {
                params.push(DatabaseValue::Int64(i));
            }
        }

        let query = format!("INSERT INTO big_table VALUES ({})", query_parts.join(", "));

        let (transformed, bindable) = transform_query_for_params(
            &query,
            &params,
            &QuestionMarkHandler,
            |param| match param {
                DatabaseValue::Now => Some("NOW()".to_string()),
                _ => None,
            },
        )
        .unwrap();

        // Should have replaced 34 NOW() values (indices 0, 3, 6, ... 99)
        assert_eq!(bindable.len(), 66);

        // Verify some replacements
        assert!(transformed.contains("NOW()"));
        assert!(transformed.contains('?'));
    }
}
