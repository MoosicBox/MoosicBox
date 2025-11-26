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
    use crate::serde_yaml;

    #[test_log::test]
    fn test_run_script_id_present() {
        let step = Step::RunScript {
            id: Some("test-step".to_string()),
            run: "echo test".to_string(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
            working_directory: None,
        };
        assert_eq!(step.id(), Some("test-step"));
    }

    #[test_log::test]
    fn test_run_script_id_absent() {
        let step = Step::RunScript {
            id: None,
            run: "echo test".to_string(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
            working_directory: None,
        };
        assert_eq!(step.id(), None);
    }

    #[test_log::test]
    fn test_use_action_id_present() {
        let step = Step::UseAction {
            id: Some("action-step".to_string()),
            uses: "checkout".to_string(),
            with: BTreeMap::new(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
        };
        assert_eq!(step.id(), Some("action-step"));
    }

    #[test_log::test]
    fn test_use_action_id_absent() {
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

    #[test_log::test]
    fn test_run_script_if_condition_present() {
        let condition = Expression::boolean(true);
        let step = Step::RunScript {
            id: None,
            run: "echo test".to_string(),
            env: BTreeMap::new(),
            if_condition: Some(condition.clone()),
            continue_on_error: false,
            working_directory: None,
        };
        assert_eq!(step.if_condition(), Some(&condition));
    }

    #[test_log::test]
    fn test_run_script_if_condition_absent() {
        let step = Step::RunScript {
            id: None,
            run: "echo test".to_string(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
            working_directory: None,
        };
        assert_eq!(step.if_condition(), None);
    }

    #[test_log::test]
    fn test_use_action_if_condition_present() {
        let condition = Expression::boolean(false);
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

    #[test_log::test]
    fn test_use_action_if_condition_absent() {
        let step = Step::UseAction {
            id: None,
            uses: "checkout".to_string(),
            with: BTreeMap::new(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
        };
        assert_eq!(step.if_condition(), None);
    }

    #[test_log::test]
    fn test_run_script_continue_on_error_true() {
        let step = Step::RunScript {
            id: None,
            run: "echo test".to_string(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: true,
            working_directory: None,
        };
        assert!(step.continue_on_error());
    }

    #[test_log::test]
    fn test_run_script_continue_on_error_false() {
        let step = Step::RunScript {
            id: None,
            run: "echo test".to_string(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
            working_directory: None,
        };
        assert!(!step.continue_on_error());
    }

    #[test_log::test]
    fn test_use_action_continue_on_error_true() {
        let step = Step::UseAction {
            id: None,
            uses: "checkout".to_string(),
            with: BTreeMap::new(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: true,
        };
        assert!(step.continue_on_error());
    }

    #[test_log::test]
    fn test_use_action_continue_on_error_false() {
        let step = Step::UseAction {
            id: None,
            uses: "checkout".to_string(),
            with: BTreeMap::new(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
        };
        assert!(!step.continue_on_error());
    }

    #[test_log::test]
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

    #[test_log::test]
    fn test_run_script_env_with_values() {
        let env = BTreeMap::from([
            ("VAR1".to_string(), "value1".to_string()),
            ("VAR2".to_string(), "value2".to_string()),
        ]);
        let step = Step::RunScript {
            id: None,
            run: "echo test".to_string(),
            env: env.clone(),
            if_condition: None,
            continue_on_error: false,
            working_directory: None,
        };
        assert_eq!(step.env(), &env);
    }

    #[test_log::test]
    fn test_use_action_env_empty() {
        let step = Step::UseAction {
            id: None,
            uses: "checkout".to_string(),
            with: BTreeMap::new(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
        };
        assert!(step.env().is_empty());
    }

    #[test_log::test]
    fn test_use_action_env_with_values() {
        let env = BTreeMap::from([
            ("ENV_VAR".to_string(), "env_value".to_string()),
            ("TOKEN".to_string(), "secret".to_string()),
        ]);
        let step = Step::UseAction {
            id: None,
            uses: "checkout".to_string(),
            with: BTreeMap::new(),
            env: env.clone(),
            if_condition: None,
            continue_on_error: false,
        };
        assert_eq!(step.env(), &env);
    }

    #[test_log::test]
    fn test_run_script_serde() {
        let step = Step::RunScript {
            id: Some("test".to_string()),
            run: "cargo test".to_string(),
            env: BTreeMap::from([("RUST_BACKTRACE".to_string(), "1".to_string())]),
            if_condition: Some(Expression::boolean(true)),
            continue_on_error: true,
            working_directory: Some("./packages/core".to_string()),
        };

        let json = serde_json::to_string(&step).unwrap();
        let deserialized: Step = serde_json::from_str(&json).unwrap();
        assert_eq!(step, deserialized);
    }

    #[test_log::test]
    fn test_use_action_serde() {
        let step = Step::UseAction {
            id: Some("checkout".to_string()),
            uses: "actions/checkout@v4".to_string(),
            with: BTreeMap::from([("ref".to_string(), "main".to_string())]),
            env: BTreeMap::from([("GITHUB_TOKEN".to_string(), "token".to_string())]),
            if_condition: Some(Expression::variable(["github", "ref"])),
            continue_on_error: false,
        };

        let json = serde_json::to_string(&step).unwrap();
        let deserialized: Step = serde_json::from_str(&json).unwrap();
        assert_eq!(step, deserialized);
    }

    #[test_log::test]
    fn test_step_yaml_untagged_run_script() {
        // Test that Step properly deserializes as RunScript variant based on 'run' field
        // Expression requires tagged format since it's not #[serde(untagged)]
        let yaml = r#"
id: build-step
run: cargo build --release
env:
  RUSTFLAGS: "-D warnings"
if:
  Boolean: true
continue-on-error: true
working-directory: ./packages/core
"#;
        let step: Step = serde_yaml::from_str(yaml).unwrap();

        // Verify it parsed as RunScript (not UseAction)
        match &step {
            Step::RunScript {
                id,
                run,
                env,
                if_condition,
                continue_on_error,
                working_directory,
            } => {
                assert_eq!(id.as_deref(), Some("build-step"));
                assert_eq!(run, "cargo build --release");
                assert_eq!(env.get("RUSTFLAGS"), Some(&"-D warnings".to_string()));
                assert!(if_condition.is_some());
                assert!(*continue_on_error);
                assert_eq!(working_directory.as_deref(), Some("./packages/core"));
            }
            Step::UseAction { .. } => panic!("Expected RunScript, got UseAction"),
        }
    }

    #[test_log::test]
    fn test_step_yaml_untagged_use_action() {
        // Test that Step properly deserializes as UseAction variant based on 'uses' field
        // Expression requires tagged format since it's not #[serde(untagged)]
        let yaml = r#"
id: checkout-step
uses: actions/checkout@v4
with:
  ref: main
  fetch-depth: "0"
env:
  GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
if:
  Boolean: false
continue-on-error: false
"#;
        let step: Step = serde_yaml::from_str(yaml).unwrap();

        // Verify it parsed as UseAction (not RunScript)
        match &step {
            Step::UseAction {
                id,
                uses,
                with,
                env,
                if_condition,
                continue_on_error,
            } => {
                assert_eq!(id.as_deref(), Some("checkout-step"));
                assert_eq!(uses, "actions/checkout@v4");
                assert_eq!(with.get("ref"), Some(&"main".to_string()));
                assert_eq!(with.get("fetch-depth"), Some(&"0".to_string()));
                assert!(env.contains_key("GITHUB_TOKEN"));
                assert!(if_condition.is_some());
                assert!(!*continue_on_error);
            }
            Step::RunScript { .. } => panic!("Expected UseAction, got RunScript"),
        }
    }

    #[test_log::test]
    fn test_step_yaml_minimal_run_script() {
        // Test minimal RunScript with only required field
        let yaml = "run: echo hello";
        let step: Step = serde_yaml::from_str(yaml).unwrap();

        match &step {
            Step::RunScript {
                id,
                run,
                env,
                if_condition,
                continue_on_error,
                working_directory,
            } => {
                assert_eq!(id, &None);
                assert_eq!(run, "echo hello");
                assert!(env.is_empty());
                assert_eq!(if_condition, &None);
                // Default value for continue_on_error is false
                assert!(!*continue_on_error);
                assert_eq!(working_directory, &None);
            }
            Step::UseAction { .. } => panic!("Expected RunScript, got UseAction"),
        }
    }

    #[test_log::test]
    fn test_step_yaml_minimal_use_action() {
        // Test minimal UseAction with only required field
        let yaml = "uses: checkout";
        let step: Step = serde_yaml::from_str(yaml).unwrap();

        match &step {
            Step::UseAction {
                id,
                uses,
                with,
                env,
                if_condition,
                continue_on_error,
            } => {
                assert_eq!(id, &None);
                assert_eq!(uses, "checkout");
                assert!(with.is_empty());
                assert!(env.is_empty());
                assert_eq!(if_condition, &None);
                assert!(!*continue_on_error);
            }
            Step::RunScript { .. } => panic!("Expected UseAction, got RunScript"),
        }
    }

    #[test_log::test]
    fn test_step_with_complex_condition() {
        let condition = Expression::binary_op(
            Expression::variable(["github", "event_name"]),
            crate::BinaryOperator::Equal,
            Expression::string("push"),
        );

        let step = Step::RunScript {
            id: Some("conditional-step".to_string()),
            run: "echo 'Running on push'".to_string(),
            env: BTreeMap::new(),
            if_condition: Some(condition.clone()),
            continue_on_error: false,
            working_directory: None,
        };

        assert_eq!(step.if_condition(), Some(&condition));
    }
}
