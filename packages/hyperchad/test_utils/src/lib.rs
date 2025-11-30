#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![doc = include_str!("../README.md")]

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

/// Represents a condition to wait for during test execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WaitStep {
    /// Wait for an element matching the selector to exist in the DOM.
    ElementExists {
        /// CSS selector to match the element.
        selector: String,
    },
    /// Wait for the URL to contain the specified fragment.
    UrlContains {
        /// Fragment that should appear in the URL.
        fragment: String,
    },
    /// Wait for a fixed duration.
    Duration {
        /// Duration to wait.
        duration: Duration,
    },
}

/// Errors that can occur during test execution.
#[derive(Debug, Error)]
pub enum TestError {
    /// A test step failed with a specific reason.
    #[error("Test step failed: {step} - {reason}")]
    StepFailed {
        /// Description of the step that failed.
        step: String,
        /// Reason for the failure.
        reason: String,
    },
    /// A wait condition timed out.
    #[error("Wait timeout: {condition}")]
    WaitTimeout {
        /// Description of the condition that timed out.
        condition: String,
    },
    /// An element could not be found in the DOM.
    #[error("Element not found: {selector}")]
    ElementNotFound {
        /// CSS selector of the element that was not found.
        selector: String,
    },
    /// An HTTP request failed.
    #[error("HTTP request failed: {url} - {reason}")]
    HttpRequestFailed {
        /// URL of the failed request.
        url: String,
        /// Reason for the failure.
        reason: String,
    },
    /// Form validation failed.
    #[error("Form validation failed: {field} - {reason}")]
    FormValidationFailed {
        /// Name of the field that failed validation.
        field: String,
        /// Reason for the validation failure.
        reason: String,
    },
    /// Navigation to a URL failed.
    #[error("Navigation failed: {url} - {reason}")]
    NavigationFailed {
        /// URL that failed to navigate to.
        url: String,
        /// Reason for the failure.
        reason: String,
    },
    /// JSON serialization or deserialization failed.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    /// URL parsing failed.
    #[error("URL parsing error: {0}")]
    UrlParsing(#[from] url::ParseError),
}

/// The result of executing a test plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Whether the test succeeded.
    pub success: bool,
    /// The number of steps executed.
    pub steps_executed: usize,
    /// The total execution time.
    pub execution_time: Duration,
    /// Errors encountered during execution.
    pub errors: Vec<String>,
    /// Warnings encountered during execution.
    pub warnings: Vec<String>,
}

impl TestResult {
    /// Creates a successful test result.
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

    /// Creates a failed test result with an error message.
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

    /// Sets the number of steps executed.
    #[must_use]
    pub const fn with_step_count(mut self, count: usize) -> Self {
        self.steps_executed = count;
        self
    }

    /// Sets the execution time.
    #[must_use]
    pub const fn with_execution_time(mut self, duration: Duration) -> Self {
        self.execution_time = duration;
        self
    }

    /// Adds an error message and marks the result as failed.
    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
        self.success = false;
    }

    /// Adds a warning message.
    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }
}

/// A declarative test plan consisting of steps to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestPlan {
    /// The test steps to execute.
    pub steps: Vec<TestStep>,
    /// Optional setup step to run before the test.
    pub setup: Option<SetupStep>,
    /// Optional teardown step to run after the test.
    pub teardown: Option<TeardownStep>,
    /// Optional timeout for the entire test plan.
    pub timeout: Option<Duration>,
    /// Number of times to retry the test on failure.
    pub retry_count: u32,
}

impl TestPlan {
    /// Creates a new empty test plan.
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

    /// Adds a setup step to run before the test.
    #[must_use]
    pub fn with_setup(mut self, setup: SetupStep) -> Self {
        self.setup = Some(setup);
        self
    }

    /// Adds a teardown step to run after the test.
    #[must_use]
    pub fn with_teardown(mut self, teardown: TeardownStep) -> Self {
        self.teardown = Some(teardown);
        self
    }

    /// Sets the timeout for the entire test plan.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Sets the number of times to retry the test on failure.
    #[must_use]
    pub const fn with_retry_count(mut self, count: u32) -> Self {
        self.retry_count = count;
        self
    }

    /// Adds a test step to the plan.
    #[must_use]
    pub fn add_step(mut self, step: TestStep) -> Self {
        self.steps.push(step);
        self
    }

    // Navigation methods
    /// Navigates to the specified URL.
    #[must_use]
    pub fn navigate_to(self, url: impl Into<String>) -> Self {
        self.add_step(TestStep::Navigation(NavigationStep::GoTo {
            url: url.into(),
        }))
    }

    /// Navigates back in browser history.
    #[must_use]
    pub fn go_back(self) -> Self {
        self.add_step(TestStep::Navigation(NavigationStep::GoBack))
    }

    /// Navigates forward in browser history.
    #[must_use]
    pub fn go_forward(self) -> Self {
        self.add_step(TestStep::Navigation(NavigationStep::GoForward))
    }

    /// Reloads the current page.
    #[must_use]
    pub fn reload(self) -> Self {
        self.add_step(TestStep::Navigation(NavigationStep::Reload))
    }

    /// Sets the URL hash fragment.
    #[must_use]
    pub fn set_hash(self, hash: impl Into<String>) -> Self {
        self.add_step(TestStep::Navigation(NavigationStep::SetHash {
            hash: hash.into(),
        }))
    }

    // Interaction methods
    /// Clicks an element matching the selector.
    #[must_use]
    pub fn click(self, selector: impl Into<String>) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::Click {
            selector: selector.into(),
        }))
    }

    /// Double-clicks an element matching the selector.
    #[must_use]
    pub fn double_click(self, selector: impl Into<String>) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::DoubleClick {
            selector: selector.into(),
        }))
    }

    /// Right-clicks an element matching the selector.
    #[must_use]
    pub fn right_click(self, selector: impl Into<String>) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::RightClick {
            selector: selector.into(),
        }))
    }

    /// Hovers over an element matching the selector.
    #[must_use]
    pub fn hover(self, selector: impl Into<String>) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::Hover {
            selector: selector.into(),
        }))
    }

    /// Focuses an element matching the selector.
    #[must_use]
    pub fn focus(self, selector: impl Into<String>) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::Focus {
            selector: selector.into(),
        }))
    }

    /// Removes focus from an element matching the selector.
    #[must_use]
    pub fn blur(self, selector: impl Into<String>) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::Blur {
            selector: selector.into(),
        }))
    }

    /// Presses a keyboard key.
    #[must_use]
    pub fn key_press(self, key: Key) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::KeyPress { key }))
    }

    /// Presses a sequence of keyboard keys.
    #[must_use]
    pub fn key_sequence(self, keys: Vec<Key>) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::KeySequence { keys }))
    }

    /// Scrolls the page in the specified direction by the given amount.
    #[must_use]
    pub fn scroll(self, direction: ScrollDirection, amount: i32) -> Self {
        self.add_step(TestStep::Interaction(InteractionStep::Scroll {
            direction,
            amount,
        }))
    }

    // Form methods
    /// Fills multiple form fields at once.
    #[must_use]
    pub fn fill_form(self, data: FormData) -> Self {
        self.add_step(TestStep::Form(FormStep::FillForm { data }))
    }

    /// Fills a single form field with a text value.
    #[must_use]
    pub fn fill_field(self, selector: impl Into<String>, value: impl Into<String>) -> Self {
        self.add_step(TestStep::Form(FormStep::FillField {
            selector: selector.into(),
            value: forms::FormValue::Text(value.into()),
        }))
    }

    /// Selects an option from a dropdown.
    #[must_use]
    pub fn select_option(self, selector: impl Into<String>, value: impl Into<String>) -> Self {
        self.add_step(TestStep::Form(FormStep::SelectOption {
            selector: selector.into(),
            value: value.into(),
        }))
    }

    /// Uploads a file to a file input element.
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
    /// Sends an HTTP request as part of the test.
    #[must_use]
    pub fn send_request(self, request: HttpRequestStep) -> Self {
        self.add_step(TestStep::Http(request))
    }

    // Wait methods
    /// Waits for an element matching the selector to exist in the DOM.
    #[must_use]
    pub fn wait_for_element(self, selector: impl Into<String>) -> Self {
        self.add_step(TestStep::Wait(WaitStep::ElementExists {
            selector: selector.into(),
        }))
    }

    /// Waits for the URL to contain the specified fragment.
    #[must_use]
    pub fn wait_for_url(self, url: impl Into<String>) -> Self {
        self.add_step(TestStep::Wait(WaitStep::UrlContains {
            fragment: url.into(),
        }))
    }

    /// Waits for a fixed duration.
    #[must_use]
    pub fn sleep(self, duration: Duration) -> Self {
        self.add_step(TestStep::Wait(WaitStep::Duration { duration }))
    }

    // Control flow methods
    /// Repeats the following steps a fixed number of times.
    #[must_use]
    pub const fn repeat(self, count: u32) -> LoopBuilder {
        LoopBuilder::new(self, count)
    }

    /// Creates branches for parallel execution.
    #[must_use]
    pub const fn parallel(self) -> ParallelBuilder {
        ParallelBuilder::new(self)
    }

    /// Includes steps from another test plan.
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

/// A single step in a test plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestStep {
    /// A navigation action.
    Navigation(NavigationStep),
    /// An interaction with the page.
    Interaction(InteractionStep),
    /// A form operation.
    Form(FormStep),
    /// An HTTP request.
    Http(HttpRequestStep),
    /// A wait condition.
    Wait(WaitStep),
    /// A control flow operation.
    Control(ControlStep),
}

/// Setup steps to run before a test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetupStep {
    /// Description of what the setup does.
    pub description: String,
    /// Steps to execute during setup.
    pub steps: Vec<TestStep>,
}

/// Teardown steps to run after a test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeardownStep {
    /// Description of what the teardown does.
    pub description: String,
    /// Steps to execute during teardown.
    pub steps: Vec<TestStep>,
}

// Builder patterns for control flow

/// Builder for creating loop control flow.
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

    /// Adds a step to be executed in each loop iteration.
    #[must_use]
    pub fn step(mut self, step: TestStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Completes the loop definition and returns to the test plan.
    #[must_use]
    pub fn end_repeat(self) -> TestPlan {
        self.plan.add_step(TestStep::Control(ControlStep::Loop {
            count: self.count,
            steps: self.steps,
        }))
    }
}

/// Builder for creating parallel execution branches.
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

    /// Creates a new parallel execution branch with the given name.
    #[must_use]
    pub fn branch(self, name: impl Into<String>) -> BranchBuilder {
        BranchBuilder::new(self, name.into())
    }

    /// Completes the parallel definition and waits for all branches to complete.
    #[must_use]
    pub fn join_all(self) -> TestPlan {
        self.plan.add_step(TestStep::Control(ControlStep::Parallel {
            branches: self.branches,
        }))
    }
}

/// Builder for adding steps to a parallel execution branch.
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

    /// Adds a step to the current branch.
    #[must_use]
    pub fn step(mut self, step: TestStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Completes the current branch and starts a new branch.
    #[must_use]
    pub fn branch(mut self, name: impl Into<String>) -> Self {
        self.parallel_builder
            .branches
            .insert(self.branch_name, self.steps);
        Self::new(self.parallel_builder, name.into())
    }

    /// Completes all branches and waits for parallel execution to finish.
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

    #[test_log::test]
    fn test_test_plan_creation() {
        let plan = TestPlan::new()
            .navigate_to("/login")
            .fill_form(FormData::new().text("username", "test"))
            .click("#submit")
            .wait_for_element("#dashboard")
            .wait_for_url("/dashboard");

        assert_eq!(plan.steps.len(), 5);
    }

    #[test_log::test]
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

    #[test_log::test]
    fn test_test_result_success() {
        let result = TestResult::success();
        assert!(result.success);
        assert_eq!(result.errors.len(), 0);
        assert_eq!(result.warnings.len(), 0);
        assert_eq!(result.steps_executed, 0);
        assert_eq!(result.execution_time, Duration::ZERO);
    }

    #[test_log::test]
    fn test_test_result_failure() {
        let result = TestResult::failure("Something went wrong");
        assert!(!result.success);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0], "Something went wrong");
    }

    #[test_log::test]
    fn test_test_result_add_error_marks_as_failed() {
        let mut result = TestResult::success();
        assert!(result.success);

        result.add_error("Test error");
        assert!(!result.success);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0], "Test error");

        // Adding another error should keep it failed
        result.add_error("Another error");
        assert!(!result.success);
        assert_eq!(result.errors.len(), 2);
    }

    #[test_log::test]
    fn test_test_result_add_warning_does_not_affect_success() {
        let mut result = TestResult::success();
        assert!(result.success);

        result.add_warning("This is a warning");
        assert!(result.success);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0], "This is a warning");
    }

    #[test_log::test]
    fn test_test_result_with_step_count() {
        let result = TestResult::success().with_step_count(42);
        assert_eq!(result.steps_executed, 42);
    }

    #[test_log::test]
    fn test_test_result_with_execution_time() {
        let result = TestResult::success().with_execution_time(Duration::from_secs(5));
        assert_eq!(result.execution_time, Duration::from_secs(5));
    }

    #[test_log::test]
    fn test_test_plan_with_setup_and_teardown() {
        let setup = SetupStep {
            description: "Initialize test".to_string(),
            steps: vec![TestStep::Navigation(NavigationStep::GoTo {
                url: "/setup".to_string(),
            })],
        };
        let teardown = TeardownStep {
            description: "Cleanup".to_string(),
            steps: vec![TestStep::Navigation(NavigationStep::GoTo {
                url: "/logout".to_string(),
            })],
        };

        let plan = TestPlan::new()
            .with_setup(setup)
            .with_teardown(teardown)
            .navigate_to("/home");

        assert!(plan.setup.is_some());
        assert!(plan.teardown.is_some());
        assert_eq!(plan.setup.as_ref().unwrap().description, "Initialize test");
        assert_eq!(plan.teardown.as_ref().unwrap().description, "Cleanup");
    }

    #[test_log::test]
    fn test_test_plan_with_timeout_and_retry() {
        let plan = TestPlan::new()
            .with_timeout(Duration::from_secs(30))
            .with_retry_count(3);

        assert_eq!(plan.timeout, Some(Duration::from_secs(30)));
        assert_eq!(plan.retry_count, 3);
    }

    #[test_log::test]
    fn test_parallel_builder_single_branch() {
        let plan = TestPlan::new()
            .parallel()
            .branch("branch1")
            .step(TestStep::Interaction(InteractionStep::Click {
                selector: "#button1".to_string(),
            }))
            .join_all();

        assert_eq!(plan.steps.len(), 1);
        if let TestStep::Control(ControlStep::Parallel { branches }) = &plan.steps[0] {
            assert_eq!(branches.len(), 1);
            assert!(branches.contains_key("branch1"));
            assert_eq!(branches.get("branch1").unwrap().len(), 1);
        } else {
            panic!("Expected parallel step");
        }
    }

    #[test_log::test]
    fn test_parallel_builder_multiple_branches() {
        let plan = TestPlan::new()
            .parallel()
            .branch("branch1")
            .step(TestStep::Interaction(InteractionStep::Click {
                selector: "#button1".to_string(),
            }))
            .branch("branch2")
            .step(TestStep::Interaction(InteractionStep::Click {
                selector: "#button2".to_string(),
            }))
            .step(TestStep::Interaction(InteractionStep::Hover {
                selector: "#link".to_string(),
            }))
            .join_all();

        assert_eq!(plan.steps.len(), 1);
        if let TestStep::Control(ControlStep::Parallel { branches }) = &plan.steps[0] {
            assert_eq!(branches.len(), 2);
            assert_eq!(branches.get("branch1").unwrap().len(), 1);
            assert_eq!(branches.get("branch2").unwrap().len(), 2);
        } else {
            panic!("Expected parallel step");
        }
    }

    #[test_log::test]
    fn test_test_plan_include_combines_steps() {
        let plan1 = TestPlan::new().navigate_to("/page1").click("#button1");

        let plan2 = TestPlan::new().navigate_to("/page2").click("#button2");

        let combined = plan1.include(plan2);
        assert_eq!(combined.steps.len(), 4);
    }

    #[test_log::test]
    fn test_test_plan_interaction_methods() {
        let plan = TestPlan::new()
            .double_click("#element")
            .right_click("#menu")
            .hover("#tooltip")
            .focus("#input")
            .blur("#input")
            .key_press(Key::Enter)
            .key_sequence(vec![Key::Control, Key::C])
            .scroll(ScrollDirection::Down, 100);

        assert_eq!(plan.steps.len(), 8);
    }

    #[test_log::test]
    fn test_test_plan_form_methods() {
        let plan = TestPlan::new()
            .fill_field("#name", "John")
            .select_option("#country", "US")
            .upload_file("#avatar", "/path/to/file.png");

        assert_eq!(plan.steps.len(), 3);
    }

    #[test_log::test]
    fn test_test_plan_navigation_methods() {
        let plan = TestPlan::new()
            .navigate_to("/page")
            .go_back()
            .go_forward()
            .reload()
            .set_hash("section1");

        assert_eq!(plan.steps.len(), 5);
    }

    #[test_log::test]
    fn test_test_plan_sleep() {
        let plan = TestPlan::new().sleep(Duration::from_millis(500));

        assert_eq!(plan.steps.len(), 1);
        if let TestStep::Wait(WaitStep::Duration { duration }) = &plan.steps[0] {
            assert_eq!(*duration, Duration::from_millis(500));
        } else {
            panic!("Expected Duration wait step");
        }
    }

    #[test_log::test]
    fn test_test_plan_send_request() {
        let request = HttpRequestStep::get("/api/data").expect_status(200);
        let plan = TestPlan::new().send_request(request);

        assert_eq!(plan.steps.len(), 1);
        if let TestStep::Http(req) = &plan.steps[0] {
            assert_eq!(req.url, "/api/data");
            assert_eq!(req.expected_status, Some(200));
        } else {
            panic!("Expected HTTP step");
        }
    }
}
