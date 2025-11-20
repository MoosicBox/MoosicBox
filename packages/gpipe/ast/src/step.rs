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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_use_action_id() {
        let step = Step::UseAction {
            id: Some("test-id".to_string()),
            uses: "checkout".to_string(),
            with: BTreeMap::new(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
        };

        assert_eq!(step.id(), Some("test-id"));
    }

    #[test]
    fn test_use_action_id_none() {
        let step = Step::UseAction {
            id: None,
            uses: "checkout".to_string(),
            with: BTreeMap::new(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
        };

        assert_eq!(step.id(), None);
    }

    #[test]
    fn test_run_script_id() {
        let step = Step::RunScript {
            id: Some("build".to_string()),
            run: "cargo build".to_string(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
            working_directory: None,
        };

        assert_eq!(step.id(), Some("build"));
    }

    #[test]
    fn test_use_action_if_condition() {
        let condition = Expression::boolean(true);
        let step = Step::UseAction {
            id: None,
            uses: "checkout".to_string(),
            with: BTreeMap::new(),
            env: BTreeMap::new(),
            if_condition: Some(condition.clone()),
            continue_on_error: false,
        };

        assert_eq!(step.if_condition(), Some(&condition));
    }

    #[test]
    fn test_run_script_if_condition() {
        let condition = Expression::variable(["success"]);
        let step = Step::RunScript {
            id: None,
            run: "echo done".to_string(),
            env: BTreeMap::new(),
            if_condition: Some(condition.clone()),
            continue_on_error: false,
            working_directory: None,
        };

        assert_eq!(step.if_condition(), Some(&condition));
    }

    #[test]
    fn test_use_action_continue_on_error_true() {
        let step = Step::UseAction {
            id: None,
            uses: "test".to_string(),
            with: BTreeMap::new(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: true,
        };

        assert!(step.continue_on_error());
    }

    #[test]
    fn test_use_action_continue_on_error_false() {
        let step = Step::UseAction {
            id: None,
            uses: "test".to_string(),
            with: BTreeMap::new(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
        };

        assert!(!step.continue_on_error());
    }

    #[test]
    fn test_run_script_continue_on_error_true() {
        let step = Step::RunScript {
            id: None,
            run: "cargo test".to_string(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: true,
            working_directory: None,
        };

        assert!(step.continue_on_error());
    }

    #[test]
    fn test_use_action_env() {
        let mut env = BTreeMap::new();
        env.insert("KEY1".to_string(), "value1".to_string());
        env.insert("KEY2".to_string(), "value2".to_string());

        let step = Step::UseAction {
            id: None,
            uses: "test".to_string(),
            with: BTreeMap::new(),
            env: env.clone(),
            if_condition: None,
            continue_on_error: false,
        };

        assert_eq!(step.env(), &env);
    }

    #[test]
    fn test_run_script_env() {
        let mut env = BTreeMap::new();
        env.insert("PATH".to_string(), "/usr/bin".to_string());

        let step = Step::RunScript {
            id: None,
            run: "echo $PATH".to_string(),
            env: env.clone(),
            if_condition: None,
            continue_on_error: false,
            working_directory: None,
        };

        assert_eq!(step.env(), &env);
    }

    #[test]
    fn test_run_script_env_empty() {
        let step = Step::RunScript {
            id: None,
            run: "echo test".to_string(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
            working_directory: None,
        };

        assert!(step.env().is_empty());
    }

    #[test]
    fn test_use_action_serialization_roundtrip() {
        let mut with = BTreeMap::new();
        with.insert("ref".to_string(), "main".to_string());

        let step = Step::UseAction {
            id: Some("checkout".to_string()),
            uses: "checkout".to_string(),
            with,
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
        };

        let yaml = serde_yaml::to_string(&step).unwrap();
        let deserialized: Step = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(deserialized, step);
    }

    #[test]
    fn test_run_script_serialization_roundtrip() {
        let step = Step::RunScript {
            id: Some("build".to_string()),
            run: "cargo build --release".to_string(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: true,
            working_directory: Some("./packages/core".to_string()),
        };

        let yaml = serde_yaml::to_string(&step).unwrap();
        let deserialized: Step = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(deserialized, step);
    }

    #[test]
    fn test_step_with_condition() {
        let condition = Expression::variable(["success"]);
        let step = Step::RunScript {
            id: None,
            run: "echo 'Success!'".to_string(),
            env: BTreeMap::new(),
            if_condition: Some(condition.clone()),
            continue_on_error: false,
            working_directory: None,
        };

        // Verify the condition is accessible via the getter
        assert_eq!(step.if_condition(), Some(&condition));
    }

    #[test]
    fn test_step_untagged_discrimination() {
        // Test that UseAction is properly deserialized when 'uses' field is present
        let yaml_use_action = r#"
uses: my-action
with:
  param: value
"#;
        let step: Step = serde_yaml::from_str(yaml_use_action).unwrap();
        assert!(matches!(step, Step::UseAction { .. }));

        // Test that RunScript is properly deserialized when 'run' field is present
        let yaml_run_script = r#"
run: echo hello
"#;
        let step: Step = serde_yaml::from_str(yaml_run_script).unwrap();
        assert!(matches!(step, Step::RunScript { .. }));
    }
}
