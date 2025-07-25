#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, time::Duration};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod forms;
pub mod http;
pub mod navigation;
pub mod workflow;

pub use forms::*;
pub use http::*;
pub use navigation::*;
pub use workflow::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WaitStep {
    ElementExists { selector: String },
    UrlContains { fragment: String },
    Duration { duration: Duration },
}

#[derive(Debug, Error)]
pub enum TestError {
    #[error("Test step failed: {step} - {reason}")]
    StepFailed { step: String, reason: String },
    #[error("Wait timeout: {condition}")]
    WaitTimeout { condition: String },
    #[error("Element not found: {selector}")]
    ElementNotFound { selector: String },
    #[error("HTTP request failed: {url} - {reason}")]
    HttpRequestFailed { url: String, reason: String },
    #[error("Form validation failed: {field} - {reason}")]
    FormValidationFailed { field: String, reason: String },
    #[error("Navigation failed: {url} - {reason}")]
    NavigationFailed { url: String, reason: String },
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("URL parsing error: {0}")]
    UrlParsing(#[from] url::ParseError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub success: bool,
    pub steps_executed: usize,
    pub execution_time: Duration,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl TestResult {
    #[must_use]
    pub const fn success() -> Self {
        Self {
            success: true,
            steps_executed: 0,
            execution_time: Duration::ZERO,
            errors: vec![],
            warnings: vec![],
        }
    }

    #[must_use]
    pub fn failure(error: impl Into<String>) -> Self {
        Self {
            success: false,
            steps_executed: 0,
            execution_time: Duration::ZERO,
            errors: vec![error.into()],
            warnings: vec![],
        }
    }

    #[must_use]
    pub const fn with_step_count(mut self, count: usize) -> Self {
        self.steps_executed = count;
        self
    }

    #[must_use]
    pub const fn with_execution_time(mut self, duration: Duration) -> Self {
        self.execution_time = duration;
        self
    }

    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
        self.success = false;
    }

    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestPlan {
    pub steps: Vec<TestStep>,
    pub setup: Option<SetupStep>,
    pub teardown: Option<TeardownStep>,
    pub timeout: Option<Duration>,
    pub retry_count: u32,
}

impl TestPlan {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            steps: vec![],
            setup: None,
            teardown: None,
            timeout: None,
            retry_count: 0,
        }
    }

    #[must_use]
    pub fn with_setup(mut self, setup: SetupStep) -> Self {
        self.setup = Some(setup);
        self
    }

    #[must_use]
    pub fn with_teardown(mut self, teardown: TeardownStep) -> Self {
        self.teardown = Some(teardown);
        self
    }

    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    #[must_use]
    pub const fn with_retry_count(mut self, count: u32) -> Self {
        self.retry_count = count;
        self
    }

    #[must_use]
    pub fn add_step(mut self, step: TestStep) -> Self {
        self.steps.push(step);
        self
    }

    // Navigation methods
    #[must_use]
    pub fn navigate_to(self, url: impl Into<String>) -> Self {
        self.add_step(TestStep::Navigation(NavigationStep::GoTo {
            url: url.into(),
        }))
    }

    #[must_use]
    pub fn go_back(self) -> Self {
        self.add_step(TestStep::Navigation(NavigationStep::GoBack))
    }

    #[must_use]
    pub fn go_forward(self) -> Self {
        self.add_step(TestStep::Navigation(NavigationStep::GoForward))
    }

    #[must_use]
    pub fn reload(self) -> Self {
        self.add_step(TestStep::Navigation(NavigationStep::Reload))
    }

    #[must_use]
    pub fn set_hash(self, hash: impl Into<String>) -> Self {
        self.add_step(TestStep::Navigation(NavigationStep::SetHash {
            hash: hash.into(),
        }))
    }

    // Interaction methods
    #[must_use]
    pub fn click(self, selector: impl Into<String>) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::Click {
            selector: selector.into(),
        }))
    }

    #[must_use]
    pub fn double_click(self, selector: impl Into<String>) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::DoubleClick {
            selector: selector.into(),
        }))
    }

    #[must_use]
    pub fn right_click(self, selector: impl Into<String>) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::RightClick {
            selector: selector.into(),
        }))
    }

    #[must_use]
    pub fn hover(self, selector: impl Into<String>) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::Hover {
            selector: selector.into(),
        }))
    }

    #[must_use]
    pub fn focus(self, selector: impl Into<String>) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::Focus {
            selector: selector.into(),
        }))
    }

    #[must_use]
    pub fn blur(self, selector: impl Into<String>) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::Blur {
            selector: selector.into(),
        }))
    }

    #[must_use]
    pub fn key_press(self, key: Key) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::KeyPress { key }))
    }

    #[must_use]
    pub fn key_sequence(self, keys: Vec<Key>) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::KeySequence { keys }))
    }

    #[must_use]
    pub fn scroll(self, direction: ScrollDirection, amount: i32) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::Scroll {
            direction,
            amount,
        }))
    }

    // Form methods
    #[must_use]
    pub fn fill_form(self, data: FormData) -> Self {
        self.add_step(TestStep::Form(FormStep::FillForm { data }))
    }

    #[must_use]
    pub fn fill_field(self, selector: impl Into<String>, value: impl Into<String>) -> Self {
        self.add_step(TestStep::Form(FormStep::FillField {
            selector: selector.into(),
            value: forms::FormValue::Text(value.into()),
        }))
    }

    #[must_use]
    pub fn select_option(self, selector: impl Into<String>, value: impl Into<String>) -> Self {
        self.add_step(TestStep::Form(FormStep::SelectOption {
            selector: selector.into(),
            value: value.into(),
        }))
    }

    #[must_use]
    pub fn upload_file(
        self,
        selector: impl Into<String>,
        file_path: impl Into<std::path::PathBuf>,
    ) -> Self {
        self.add_step(TestStep::Form(FormStep::UploadFile {
            selector: selector.into(),
            file_path: file_path.into(),
        }))
    }

    // HTTP methods
    #[must_use]
    pub fn send_request(self, request: HttpRequestStep) -> Self {
        self.add_step(TestStep::Http(request))
    }

    // Wait methods
    #[must_use]
    pub fn wait_for_element(self, selector: impl Into<String>) -> Self {
        self.add_step(TestStep::Wait(WaitStep::ElementExists {
            selector: selector.into(),
        }))
    }

    #[must_use]
    pub fn wait_for_url(self, url: impl Into<String>) -> Self {
        self.add_step(TestStep::Wait(WaitStep::UrlContains {
            fragment: url.into(),
        }))
    }

    #[must_use]
    pub fn sleep(self, duration: Duration) -> Self {
        self.add_step(TestStep::Wait(WaitStep::Duration { duration }))
    }

    // Control flow methods
    #[must_use]
    pub const fn repeat(self, count: u32) -> LoopBuilder {
        LoopBuilder::new(self, count)
    }

    #[must_use]
    pub const fn parallel(self) -> ParallelBuilder {
        ParallelBuilder::new(self)
    }

    #[must_use]
    pub fn include(mut self, other: Self) -> Self {
        self.steps.extend(other.steps);
        self
    }
}

impl Default for TestPlan {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestStep {
    Navigation(NavigationStep),
    Interaction(InteractionStep),
    Form(FormStep),
    Http(HttpRequestStep),
    Wait(WaitStep),
    Control(ControlStep),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupStep {
    pub description: String,
    pub steps: Vec<TestStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeardownStep {
    pub description: String,
    pub steps: Vec<TestStep>,
}

// Builder patterns for control flow

pub struct LoopBuilder {
    plan: TestPlan,
    count: u32,
    steps: Vec<TestStep>,
}

impl LoopBuilder {
    const fn new(plan: TestPlan, count: u32) -> Self {
        Self {
            plan,
            count,
            steps: vec![],
        }
    }

    #[must_use]
    pub fn step(mut self, step: TestStep) -> Self {
        self.steps.push(step);
        self
    }

    #[must_use]
    pub fn end_repeat(self) -> TestPlan {
        self.plan.add_step(TestStep::Control(ControlStep::Loop {
            count: self.count,
            steps: self.steps,
        }))
    }
}

pub struct ParallelBuilder {
    plan: TestPlan,
    branches: BTreeMap<String, Vec<TestStep>>,
}

impl ParallelBuilder {
    const fn new(plan: TestPlan) -> Self {
        Self {
            plan,
            branches: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn branch(self, name: impl Into<String>) -> BranchBuilder {
        BranchBuilder::new(self, name.into())
    }

    #[must_use]
    pub fn join_all(self) -> TestPlan {
        self.plan.add_step(TestStep::Control(ControlStep::Parallel {
            branches: self.branches,
        }))
    }
}

pub struct BranchBuilder {
    parallel_builder: ParallelBuilder,
    branch_name: String,
    steps: Vec<TestStep>,
}

impl BranchBuilder {
    const fn new(parallel_builder: ParallelBuilder, branch_name: String) -> Self {
        Self {
            parallel_builder,
            branch_name,
            steps: vec![],
        }
    }

    #[must_use]
    pub fn step(mut self, step: TestStep) -> Self {
        self.steps.push(step);
        self
    }

    #[must_use]
    pub fn branch(mut self, name: impl Into<String>) -> Self {
        self.parallel_builder
            .branches
            .insert(self.branch_name, self.steps);
        Self::new(self.parallel_builder, name.into())
    }

    #[must_use]
    pub fn join_all(mut self) -> TestPlan {
        self.parallel_builder
            .branches
            .insert(self.branch_name, self.steps);
        self.parallel_builder.join_all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_test_plan_creation() {
        let plan = TestPlan::new()
            .navigate_to("/login")
            .fill_form(FormData::new().text("username", "test"))
            .click("#submit")
            .wait_for_element("#dashboard")
            .wait_for_url("/dashboard");

        assert_eq!(plan.steps.len(), 5);
    }

    #[test]
    fn test_loop_builder() {
        let plan = TestPlan::new()
            .repeat(3)
            .step(TestStep::Interaction(InteractionStep::Click {
                selector: "#button".to_string(),
            }))
            .end_repeat();

        assert_eq!(plan.steps.len(), 1);
        if let TestStep::Control(ControlStep::Loop { count, steps }) = &plan.steps[0] {
            assert_eq!(*count, 3);
            assert_eq!(steps.len(), 1);
        } else {
            panic!("Expected loop step");
        }
    }

    #[test]
    fn test_test_result() {
        let mut result = TestResult::success();
        assert!(result.success);
        assert_eq!(result.errors.len(), 0);

        result.add_error("Test error");
        assert!(!result.success);
        assert_eq!(result.errors.len(), 1);
    }
}
