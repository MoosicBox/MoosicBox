//! Step definition types for workflow jobs.
//!
//! This module provides the `Step` enum and related types for representing individual
//! steps within a workflow job. Steps can either use predefined actions or run shell
//! commands, with support for conditions, environment variables, and error handling.

use crate::Expression;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A single step in a workflow job.
///
/// Steps can either use an action or run a shell command.
/// According to the specification, steps should be represented as enum variants
/// (`UseAction` vs `RunScript`) rather than optional fields.
///
/// # Examples
///
/// ```
/// use gpipe_ast::Step;
/// use std::collections::BTreeMap;
///
/// // Create a step that runs a shell command
/// let run_step = Step::RunScript {
///     id: Some("test".to_string()),
///     run: "cargo test".to_string(),
///     env: BTreeMap::new(),
///     if_condition: None,
///     continue_on_error: false,
///     working_directory: Some("./packages/core".to_string()),
/// };
///
/// // Create a step that uses an action
/// let action_step = Step::UseAction {
///     id: None,
///     uses: "checkout".to_string(),
///     with: BTreeMap::from([("ref".to_string(), "main".to_string())]),
///     env: BTreeMap::new(),
///     if_condition: None,
///     continue_on_error: false,
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Step {
    /// Step that uses a predefined action
    UseAction {
        /// Optional step identifier for referencing outputs
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,

        /// Action name to use (must exist in workflow actions map)
        uses: String,

        /// Parameters to pass to the action
        #[serde(default)]
        #[serde(rename = "with")]
        with: BTreeMap<String, String>,

        /// Environment variables for this step
        #[serde(default)]
        env: BTreeMap<String, String>,

        /// Condition for running this step
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "if")]
        if_condition: Option<Expression>,

        /// Continue job execution even if this step fails
        #[serde(default)]
        #[serde(rename = "continue-on-error")]
        continue_on_error: bool,
    },

    /// Step that runs a shell command
    RunScript {
        /// Optional step identifier for referencing outputs
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,

        /// Shell command or script to run
        run: String,

        /// Environment variables for this step
        #[serde(default)]
        env: BTreeMap<String, String>,

        /// Condition for running this step
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "if")]
        if_condition: Option<Expression>,

        /// Continue job execution even if this step fails
        #[serde(default)]
        #[serde(rename = "continue-on-error")]
        continue_on_error: bool,

        /// Working directory for command execution
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "working-directory")]
        working_directory: Option<String>,
    },
}

impl Step {
    /// Get the step ID if present
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::UseAction { id, .. } | Self::RunScript { id, .. } => id.as_deref(),
        }
    }

    /// Get the step's condition expression if present
    #[must_use]
    pub const fn if_condition(&self) -> Option<&Expression> {
        match self {
            Self::UseAction { if_condition, .. } | Self::RunScript { if_condition, .. } => {
                if_condition.as_ref()
            }
        }
    }

    /// Check if this step should continue on error
    #[must_use]
    pub const fn continue_on_error(&self) -> bool {
        match self {
            Self::UseAction {
                continue_on_error, ..
            }
            | Self::RunScript {
                continue_on_error, ..
            } => *continue_on_error,
        }
    }

    /// Get the environment variables for this step
    #[must_use]
    pub const fn env(&self) -> &BTreeMap<String, String> {
        match self {
            Self::UseAction { env, .. } | Self::RunScript { env, .. } => env,
        }
    }
}
