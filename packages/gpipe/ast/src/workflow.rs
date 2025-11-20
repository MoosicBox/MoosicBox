//! Workflow definition types for the generic pipeline AST.
//!
//! This module provides the core data structures for representing complete workflows,
//! including triggers, jobs, actions, and matrix strategies. These types form the
//! top-level structure of workflow definitions that can be parsed from YAML files.

use crate::Expression;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Top-level workflow definition according to the generic workflow schema.
///
/// A workflow represents a complete CI/CD pipeline configuration with triggers,
/// reusable actions, and jobs that execute steps. Workflows can be serialized
/// to/from YAML and executed by compatible runners.
///
/// # Structure
///
/// ```yaml
/// version: 1.0
/// name: string
/// triggers:
///   push:
///     branches: [string]
///   pull_request:
///     types: [string]
///   schedule:
///     cron: string
///   manual:
/// actions:
///   name:
///     type: github|file|inline
///     repo: string  # for github
///     path: string  # for file
///     # inline has full action definition
/// jobs:
///   job-name:
///     needs: [string]
///     env:
///       KEY: value
///     strategy:
///       matrix:
///         os: [ubuntu-latest, windows-latest, macos-latest]
///         exclude:
///           - os: windows-latest
///     steps:
///       - uses: action-name
///         with:
///           param: value
///       - run: shell command
///         id: step-id
///         if: ${{ expression }}
///         continue-on-error: boolean
/// ```
///
/// # Examples
///
/// ```
/// use gpipe_ast::{Workflow, Trigger, TriggerType, TriggerConfig, Job, Step};
/// use std::collections::BTreeMap;
///
/// let workflow = Workflow {
///     version: "1.0".to_string(),
///     name: "Test Workflow".to_string(),
///     triggers: vec![Trigger {
///         trigger_type: TriggerType::Push,
///         config: TriggerConfig {
///             branches: Some(vec!["main".to_string()]),
///             types: None,
///             cron: None,
///         },
///     }],
///     actions: BTreeMap::new(),
///     jobs: BTreeMap::from([(
///         "test".to_string(),
///         Job {
///             needs: vec![],
///             env: BTreeMap::new(),
///             strategy: None,
///             steps: vec![Step::RunScript {
///                 id: None,
///                 run: "echo 'Hello, World!'".to_string(),
///                 env: BTreeMap::new(),
///                 if_condition: None,
///                 continue_on_error: false,
///                 working_directory: None,
///             }],
///             if_condition: None,
///         },
///     )]),
/// };
///
/// // Serialize to YAML
/// let yaml = gpipe_ast::serde_yaml::to_string(&workflow).unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workflow {
    /// Workflow format version (e.g., "1.0")
    pub version: String,

    /// Human-readable workflow name
    pub name: String,

    /// Trigger conditions for when the workflow should run
    pub triggers: Vec<Trigger>,

    /// Action definitions that can be referenced in steps
    pub actions: BTreeMap<String, ActionDef>,

    /// Job definitions with their steps
    pub jobs: BTreeMap<String, Job>,
}

/// Trigger conditions for workflow execution
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trigger {
    /// Trigger type: push, `pull_request`, schedule, manual
    #[serde(rename = "type")]
    pub trigger_type: TriggerType,

    /// Additional trigger configuration
    #[serde(flatten)]
    pub config: TriggerConfig,
}

/// Available trigger types that map to backend-specific triggers
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerType {
    /// Git push events (maps to GitHub push, GitLab push)
    Push,
    /// Pull/merge request events (maps to GitHub `pull_request`, GitLab `merge_request`)
    PullRequest,
    /// Scheduled execution (maps to GitHub schedule, GitLab schedule)
    Schedule,
    /// Manual execution (maps to GitHub `workflow_dispatch`, GitLab manual)
    Manual,
}

/// Configuration for different trigger types
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerConfig {
    /// Branches to trigger on (for `push/pull_request`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branches: Option<Vec<String>>,

    /// Event types (for `pull_request`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<String>>,

    /// Cron schedule (for schedule trigger)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cron: Option<String>,
}

/// Action definition that can be referenced in workflow steps
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionDef {
    /// Action type determines how it's resolved and executed
    #[serde(rename = "type")]
    pub action_type: ActionType,

    /// Action-specific configuration
    #[serde(flatten)]
    pub config: ActionConfig,
}

/// Available action types for the action system
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionType {
    /// Reference to a GitHub action repository
    Github,
    /// Reference to a local action file
    File,
    /// Inline action definition
    Inline,
}

/// Configuration specific to each action type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionConfig {
    /// GitHub repository reference (for github type)
    /// Format: "owner/name@ref" or "actions/checkout@v4"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,

    /// Path to action file (for file type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Inline action definition (for inline type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Action description (for inline type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Action inputs (for inline type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inputs: Option<BTreeMap<String, ActionInput>>,

    /// Action outputs (for inline type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<BTreeMap<String, ActionOutput>>,

    /// Action runs configuration (for inline type)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runs: Option<ActionRuns>,
}

/// Action input definition
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionInput {
    /// Input description
    pub description: String,

    /// Whether the input is required
    #[serde(default)]
    pub required: bool,

    /// Default value if not provided
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
}

/// Action output definition
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionOutput {
    /// Output description
    pub description: String,
}

/// Action runs definition for inline actions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionRuns {
    /// Steps to execute for this action
    pub steps: Vec<crate::Step>,
}

/// Job definition with dependencies, environment, and steps
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Job {
    /// Jobs this job depends on (must complete successfully first)
    #[serde(default)]
    pub needs: Vec<String>,

    /// Environment variables for this job
    #[serde(default)]
    pub env: BTreeMap<String, String>,

    /// Matrix strategy for parallel execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<MatrixStrategy>,

    /// Steps to execute in this job
    pub steps: Vec<crate::Step>,

    /// Condition for running this job
    #[serde(skip_serializing_if = "Option::is_none")]
    pub if_condition: Option<Expression>,
}

/// Matrix strategy for running jobs with different configurations
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatrixStrategy {
    /// Matrix configuration
    pub matrix: Matrix,
}

/// Matrix configuration with variables and exclusions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Matrix {
    /// Matrix variables and their possible values
    #[serde(flatten)]
    pub variables: BTreeMap<String, Vec<String>>,

    /// Combinations to exclude from the matrix
    #[serde(default)]
    pub exclude: Vec<BTreeMap<String, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_workflow_serde_basic() {
        let workflow = Workflow {
            version: "1.0".to_string(),
            name: "Test Workflow".to_string(),
            triggers: vec![],
            actions: BTreeMap::new(),
            jobs: BTreeMap::new(),
        };

        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let deserialized: Workflow = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(workflow, deserialized);
    }

    #[test_log::test]
    fn test_workflow_serde_with_triggers() {
        let workflow = Workflow {
            version: "1.0".to_string(),
            name: "CI Workflow".to_string(),
            triggers: vec![
                Trigger {
                    trigger_type: TriggerType::Push,
                    config: TriggerConfig {
                        branches: Some(vec!["main".to_string(), "develop".to_string()]),
                        types: None,
                        cron: None,
                    },
                },
                Trigger {
                    trigger_type: TriggerType::PullRequest,
                    config: TriggerConfig {
                        branches: Some(vec!["main".to_string()]),
                        types: Some(vec!["opened".to_string(), "synchronize".to_string()]),
                        cron: None,
                    },
                },
            ],
            actions: BTreeMap::new(),
            jobs: BTreeMap::new(),
        };

        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let deserialized: Workflow = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(workflow, deserialized);
    }

    #[test_log::test]
    fn test_workflow_serde_with_schedule_trigger() {
        let workflow = Workflow {
            version: "1.0".to_string(),
            name: "Scheduled Workflow".to_string(),
            triggers: vec![Trigger {
                trigger_type: TriggerType::Schedule,
                config: TriggerConfig {
                    branches: None,
                    types: None,
                    cron: Some("0 0 * * *".to_string()),
                },
            }],
            actions: BTreeMap::new(),
            jobs: BTreeMap::new(),
        };

        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let deserialized: Workflow = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(workflow, deserialized);
    }

    #[test_log::test]
    fn test_workflow_serde_with_manual_trigger() {
        let workflow = Workflow {
            version: "1.0".to_string(),
            name: "Manual Workflow".to_string(),
            triggers: vec![Trigger {
                trigger_type: TriggerType::Manual,
                config: TriggerConfig {
                    branches: None,
                    types: None,
                    cron: None,
                },
            }],
            actions: BTreeMap::new(),
            jobs: BTreeMap::new(),
        };

        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let deserialized: Workflow = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(workflow, deserialized);
    }

    #[test_log::test]
    fn test_workflow_with_github_action() {
        let mut actions = BTreeMap::new();
        actions.insert(
            "checkout".to_string(),
            ActionDef {
                action_type: ActionType::Github,
                config: ActionConfig {
                    repo: Some("actions/checkout@v4".to_string()),
                    path: None,
                    name: None,
                    description: None,
                    inputs: None,
                    outputs: None,
                    runs: None,
                },
            },
        );

        let workflow = Workflow {
            version: "1.0".to_string(),
            name: "Workflow with Actions".to_string(),
            triggers: vec![],
            actions,
            jobs: BTreeMap::new(),
        };

        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let deserialized: Workflow = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(workflow, deserialized);
    }

    #[test_log::test]
    fn test_workflow_with_file_action() {
        let mut actions = BTreeMap::new();
        actions.insert(
            "custom".to_string(),
            ActionDef {
                action_type: ActionType::File,
                config: ActionConfig {
                    repo: None,
                    path: Some(".github/actions/custom/action.yml".to_string()),
                    name: None,
                    description: None,
                    inputs: None,
                    outputs: None,
                    runs: None,
                },
            },
        );

        let workflow = Workflow {
            version: "1.0".to_string(),
            name: "Workflow with File Action".to_string(),
            triggers: vec![],
            actions,
            jobs: BTreeMap::new(),
        };

        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let deserialized: Workflow = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(workflow, deserialized);
    }

    #[test_log::test]
    fn test_workflow_with_inline_action() {
        let mut actions = BTreeMap::new();
        actions.insert(
            "inline-action".to_string(),
            ActionDef {
                action_type: ActionType::Inline,
                config: ActionConfig {
                    repo: None,
                    path: None,
                    name: Some("Test Action".to_string()),
                    description: Some("A test inline action".to_string()),
                    inputs: Some(BTreeMap::from([(
                        "input1".to_string(),
                        ActionInput {
                            description: "First input".to_string(),
                            required: true,
                            default: None,
                        },
                    )])),
                    outputs: Some(BTreeMap::from([(
                        "output1".to_string(),
                        ActionOutput {
                            description: "First output".to_string(),
                        },
                    )])),
                    runs: Some(ActionRuns {
                        steps: vec![crate::Step::RunScript {
                            id: None,
                            run: "echo 'test'".to_string(),
                            env: BTreeMap::new(),
                            if_condition: None,
                            continue_on_error: false,
                            working_directory: None,
                        }],
                    }),
                },
            },
        );

        let workflow = Workflow {
            version: "1.0".to_string(),
            name: "Workflow with Inline Action".to_string(),
            triggers: vec![],
            actions,
            jobs: BTreeMap::new(),
        };

        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let deserialized: Workflow = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(workflow, deserialized);
    }

    #[test_log::test]
    fn test_workflow_with_jobs() {
        let mut jobs = BTreeMap::new();
        jobs.insert(
            "build".to_string(),
            Job {
                needs: vec![],
                env: BTreeMap::from([("NODE_ENV".to_string(), "production".to_string())]),
                strategy: None,
                steps: vec![crate::Step::RunScript {
                    id: Some("build-step".to_string()),
                    run: "npm run build".to_string(),
                    env: BTreeMap::new(),
                    if_condition: None,
                    continue_on_error: false,
                    working_directory: None,
                }],
                if_condition: None,
            },
        );

        let workflow = Workflow {
            version: "1.0".to_string(),
            name: "Build Workflow".to_string(),
            triggers: vec![],
            actions: BTreeMap::new(),
            jobs,
        };

        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let deserialized: Workflow = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(workflow, deserialized);
    }

    #[test_log::test]
    fn test_workflow_with_job_dependencies() {
        let mut jobs = BTreeMap::new();
        jobs.insert(
            "build".to_string(),
            Job {
                needs: vec![],
                env: BTreeMap::new(),
                strategy: None,
                steps: vec![],
                if_condition: None,
            },
        );
        jobs.insert(
            "test".to_string(),
            Job {
                needs: vec!["build".to_string()],
                env: BTreeMap::new(),
                strategy: None,
                steps: vec![],
                if_condition: None,
            },
        );
        jobs.insert(
            "deploy".to_string(),
            Job {
                needs: vec!["build".to_string(), "test".to_string()],
                env: BTreeMap::new(),
                strategy: None,
                steps: vec![],
                if_condition: None,
            },
        );

        let workflow = Workflow {
            version: "1.0".to_string(),
            name: "Pipeline with Dependencies".to_string(),
            triggers: vec![],
            actions: BTreeMap::new(),
            jobs,
        };

        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let deserialized: Workflow = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(workflow, deserialized);
    }

    #[test_log::test]
    fn test_workflow_with_matrix_strategy() {
        let matrix = Matrix {
            variables: BTreeMap::from([
                (
                    "os".to_string(),
                    vec![
                        "ubuntu-latest".to_string(),
                        "windows-latest".to_string(),
                        "macos-latest".to_string(),
                    ],
                ),
                (
                    "rust".to_string(),
                    vec!["stable".to_string(), "nightly".to_string()],
                ),
            ]),
            exclude: vec![],
        };

        let mut jobs = BTreeMap::new();
        jobs.insert(
            "test".to_string(),
            Job {
                needs: vec![],
                env: BTreeMap::new(),
                strategy: Some(MatrixStrategy { matrix }),
                steps: vec![crate::Step::RunScript {
                    id: None,
                    run: "cargo test".to_string(),
                    env: BTreeMap::new(),
                    if_condition: None,
                    continue_on_error: false,
                    working_directory: None,
                }],
                if_condition: None,
            },
        );

        let workflow = Workflow {
            version: "1.0".to_string(),
            name: "Matrix Workflow".to_string(),
            triggers: vec![],
            actions: BTreeMap::new(),
            jobs,
        };

        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let deserialized: Workflow = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(workflow, deserialized);
    }

    #[test_log::test]
    fn test_workflow_with_matrix_exclusions() {
        let matrix = Matrix {
            variables: BTreeMap::from([
                (
                    "os".to_string(),
                    vec![
                        "ubuntu-latest".to_string(),
                        "windows-latest".to_string(),
                        "macos-latest".to_string(),
                    ],
                ),
                (
                    "rust".to_string(),
                    vec!["stable".to_string(), "nightly".to_string()],
                ),
            ]),
            exclude: vec![
                BTreeMap::from([
                    ("os".to_string(), "windows-latest".to_string()),
                    ("rust".to_string(), "nightly".to_string()),
                ]),
                BTreeMap::from([
                    ("os".to_string(), "macos-latest".to_string()),
                    ("rust".to_string(), "nightly".to_string()),
                ]),
            ],
        };

        let mut jobs = BTreeMap::new();
        jobs.insert(
            "test".to_string(),
            Job {
                needs: vec![],
                env: BTreeMap::new(),
                strategy: Some(MatrixStrategy { matrix }),
                steps: vec![],
                if_condition: None,
            },
        );

        let workflow = Workflow {
            version: "1.0".to_string(),
            name: "Matrix with Exclusions".to_string(),
            triggers: vec![],
            actions: BTreeMap::new(),
            jobs,
        };

        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let deserialized: Workflow = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(workflow, deserialized);
    }

    #[test_log::test]
    fn test_workflow_with_job_condition() {
        let condition = Expression::binary_op(
            Expression::variable(["github", "ref"]),
            crate::BinaryOperator::Equal,
            Expression::string("refs/heads/main"),
        );

        let mut jobs = BTreeMap::new();
        jobs.insert(
            "deploy".to_string(),
            Job {
                needs: vec![],
                env: BTreeMap::new(),
                strategy: None,
                steps: vec![],
                if_condition: Some(condition),
            },
        );

        let workflow = Workflow {
            version: "1.0".to_string(),
            name: "Conditional Job Workflow".to_string(),
            triggers: vec![],
            actions: BTreeMap::new(),
            jobs,
        };

        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let deserialized: Workflow = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(workflow, deserialized);
    }

    #[test_log::test]
    fn test_complete_workflow() {
        let mut actions = BTreeMap::new();
        actions.insert(
            "checkout".to_string(),
            ActionDef {
                action_type: ActionType::Github,
                config: ActionConfig {
                    repo: Some("actions/checkout@v4".to_string()),
                    path: None,
                    name: None,
                    description: None,
                    inputs: None,
                    outputs: None,
                    runs: None,
                },
            },
        );

        let mut jobs = BTreeMap::new();
        jobs.insert(
            "test".to_string(),
            Job {
                needs: vec![],
                env: BTreeMap::from([("RUST_BACKTRACE".to_string(), "1".to_string())]),
                strategy: Some(MatrixStrategy {
                    matrix: Matrix {
                        variables: BTreeMap::from([(
                            "os".to_string(),
                            vec!["ubuntu-latest".to_string(), "macos-latest".to_string()],
                        )]),
                        exclude: vec![],
                    },
                }),
                steps: vec![
                    crate::Step::UseAction {
                        id: Some("checkout".to_string()),
                        uses: "checkout".to_string(),
                        with: BTreeMap::new(),
                        env: BTreeMap::new(),
                        if_condition: None,
                        continue_on_error: false,
                    },
                    crate::Step::RunScript {
                        id: Some("test".to_string()),
                        run: "cargo test".to_string(),
                        env: BTreeMap::new(),
                        if_condition: None,
                        continue_on_error: false,
                        working_directory: None,
                    },
                ],
                if_condition: None,
            },
        );

        let workflow = Workflow {
            version: "1.0".to_string(),
            name: "Complete Test Workflow".to_string(),
            triggers: vec![
                Trigger {
                    trigger_type: TriggerType::Push,
                    config: TriggerConfig {
                        branches: Some(vec!["main".to_string()]),
                        types: None,
                        cron: None,
                    },
                },
                Trigger {
                    trigger_type: TriggerType::PullRequest,
                    config: TriggerConfig {
                        branches: Some(vec!["main".to_string()]),
                        types: Some(vec!["opened".to_string()]),
                        cron: None,
                    },
                },
            ],
            actions,
            jobs,
        };

        let yaml = serde_yaml::to_string(&workflow).unwrap();
        let deserialized: Workflow = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(workflow, deserialized);
    }
}
