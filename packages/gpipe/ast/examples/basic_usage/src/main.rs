#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for the `gpipe_ast` crate.
//!
//! This example demonstrates how to programmatically build workflow definitions
//! using the AST types, serialize them to YAML, and deserialize them back.

use gpipe_ast::{
    ActionConfig, ActionDef, ActionType, BinaryOperator, Expression, Job, Matrix, MatrixStrategy,
    Step, Trigger, TriggerConfig, TriggerType, Workflow,
};
use std::collections::BTreeMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== gpipe_ast Basic Usage Example ===\n");

    // Build a complete workflow programmatically
    let workflow = build_example_workflow();

    // Serialize to YAML
    println!("--- Serializing workflow to YAML ---");
    let yaml = gpipe_ast::serde_yaml::to_string(&workflow)?;
    println!("{yaml}");

    // Deserialize back from YAML
    println!("\n--- Deserializing workflow from YAML ---");
    let deserialized: Workflow = gpipe_ast::serde_yaml::from_str(&yaml)?;
    println!("Successfully deserialized workflow: {}", deserialized.name);
    println!("  Version: {}", deserialized.version);
    println!("  Triggers: {}", deserialized.triggers.len());
    println!("  Jobs: {}", deserialized.jobs.len());

    // Demonstrate expression builder API
    println!("\n--- Expression Builder Examples ---");
    demonstrate_expressions();

    // Demonstrate step types
    println!("\n--- Step Type Examples ---");
    demonstrate_steps();

    println!("\n=== Example completed successfully! ===");
    Ok(())
}

/// Builds a complete example workflow with multiple jobs, steps, and features.
fn build_example_workflow() -> Workflow {
    // Create triggers
    let triggers = vec![
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
        Trigger {
            trigger_type: TriggerType::Manual,
            config: TriggerConfig {
                branches: None,
                types: None,
                cron: None,
            },
        },
    ];

    // Create action definitions
    let mut actions = BTreeMap::new();

    // GitHub action reference
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

    // File-based action reference
    actions.insert(
        "custom-action".to_string(),
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

    // Create jobs
    let mut jobs = BTreeMap::new();

    // Build job with matrix strategy
    jobs.insert("build".to_string(), create_build_job());

    // Test job that depends on build
    jobs.insert("test".to_string(), create_test_job());

    // Deploy job with conditional execution
    jobs.insert("deploy".to_string(), create_deploy_job());

    Workflow {
        version: "1.0".to_string(),
        name: "Example CI/CD Pipeline".to_string(),
        triggers,
        actions,
        jobs,
    }
}

/// Creates a build job with matrix strategy.
fn create_build_job() -> Job {
    // Create matrix configuration
    let mut matrix_vars = BTreeMap::new();
    matrix_vars.insert(
        "os".to_string(),
        vec![
            "ubuntu-latest".to_string(),
            "windows-latest".to_string(),
            "macos-latest".to_string(),
        ],
    );
    matrix_vars.insert(
        "rust".to_string(),
        vec!["stable".to_string(), "nightly".to_string()],
    );

    // Exclude specific combinations
    let mut exclude = vec![];
    let mut exclude_combo = BTreeMap::new();
    exclude_combo.insert("os".to_string(), "windows-latest".to_string());
    exclude_combo.insert("rust".to_string(), "nightly".to_string());
    exclude.push(exclude_combo);

    let strategy = Some(MatrixStrategy {
        matrix: Matrix {
            variables: matrix_vars,
            exclude,
        },
    });

    // Create build steps
    let steps = vec![
        Step::UseAction {
            id: Some("checkout".to_string()),
            uses: "checkout".to_string(),
            with: BTreeMap::new(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
        },
        Step::RunScript {
            id: Some("setup".to_string()),
            run: "rustup toolchain install ${{ matrix.rust }}".to_string(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
            working_directory: None,
        },
        Step::RunScript {
            id: Some("build".to_string()),
            run: "cargo build --release".to_string(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
            working_directory: None,
        },
    ];

    // Environment variables
    let mut env = BTreeMap::new();
    env.insert("RUST_BACKTRACE".to_string(), "1".to_string());

    Job {
        needs: vec![],
        env,
        strategy,
        steps,
        if_condition: None,
    }
}

/// Creates a test job that depends on build job.
fn create_test_job() -> Job {
    let steps = vec![
        Step::UseAction {
            id: None,
            uses: "checkout".to_string(),
            with: BTreeMap::new(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
        },
        Step::RunScript {
            id: Some("test".to_string()),
            run: "cargo test --all-features".to_string(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
            working_directory: None,
        },
        Step::RunScript {
            id: Some("clippy".to_string()),
            run: "cargo clippy -- -D warnings".to_string(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: true, // Don't fail job on clippy warnings
            working_directory: None,
        },
    ];

    Job {
        needs: vec!["build".to_string()],
        env: BTreeMap::new(),
        strategy: None,
        steps,
        if_condition: None,
    }
}

/// Creates a deploy job with conditional execution.
fn create_deploy_job() -> Job {
    let steps = vec![
        Step::UseAction {
            id: None,
            uses: "checkout".to_string(),
            with: BTreeMap::new(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
        },
        Step::RunScript {
            id: Some("deploy".to_string()),
            run: "cargo publish".to_string(),
            env: BTreeMap::new(),
            if_condition: None,
            continue_on_error: false,
            working_directory: None,
        },
    ];

    // Only run on main branch
    let condition = Expression::binary_op(
        Expression::variable(["github", "ref"]),
        BinaryOperator::Equal,
        Expression::string("refs/heads/main"),
    );

    Job {
        needs: vec!["build".to_string(), "test".to_string()],
        env: BTreeMap::new(),
        strategy: None,
        steps,
        if_condition: Some(condition),
    }
}

/// Demonstrates the expression builder API.
fn demonstrate_expressions() {
    // Simple literals
    let str_expr = Expression::string("hello");
    let num_expr = Expression::number(42.0);
    let bool_expr = Expression::boolean(true);
    let null_expr = Expression::null();

    println!("String: {str_expr:?}");
    println!("Number: {num_expr:?}");
    println!("Boolean: {bool_expr:?}");
    println!("Null: {null_expr:?}");

    // Variable references
    let var_expr = Expression::variable(["github", "actor"]);
    println!("Variable: {var_expr:?}");

    // Binary operations
    let equals_expr = Expression::binary_op(
        Expression::variable(["github", "event_name"]),
        BinaryOperator::Equal,
        Expression::string("push"),
    );
    println!("Equality: {equals_expr:?}");

    let and_expr = Expression::binary_op(
        Expression::boolean(true),
        BinaryOperator::And,
        Expression::boolean(false),
    );
    println!("Logical AND: {and_expr:?}");

    // Unary operations
    let not_expr = Expression::unary_op(gpipe_ast::UnaryOperator::Not, Expression::boolean(true));
    println!("Negation: {not_expr:?}");

    // Function calls
    let func_expr = Expression::function_call(
        "contains",
        vec![
            Expression::variable(["github", "ref"]),
            Expression::string("release"),
        ],
    );
    println!("Function call: {func_expr:?}");

    // Index expressions
    let index_expr = Expression::index(
        Expression::variable(["matrix", "os"]),
        Expression::number(0.0),
    );
    println!("Index: {index_expr:?}");
}

/// Demonstrates different step types.
fn demonstrate_steps() {
    // UseAction step
    let mut with_params = BTreeMap::new();
    with_params.insert("ref".to_string(), "main".to_string());

    let use_step = Step::UseAction {
        id: Some("checkout-step".to_string()),
        uses: "checkout".to_string(),
        with: with_params,
        env: BTreeMap::new(),
        if_condition: None,
        continue_on_error: false,
    };

    println!("UseAction step ID: {:?}", use_step.id());
    println!("  Continue on error: {}", use_step.continue_on_error());

    // RunScript step
    let mut env = BTreeMap::new();
    env.insert("DEBUG".to_string(), "true".to_string());

    let run_step = Step::RunScript {
        id: Some("build-step".to_string()),
        run: "cargo build".to_string(),
        env,
        if_condition: Some(Expression::boolean(true)),
        continue_on_error: true,
        working_directory: Some("./subproject".to_string()),
    };

    println!("RunScript step ID: {:?}", run_step.id());
    println!("  Has condition: {}", run_step.if_condition().is_some());
    println!("  Environment vars: {}", run_step.env().len());
}
