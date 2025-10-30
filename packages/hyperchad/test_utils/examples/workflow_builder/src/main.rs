#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Workflow Builder Example
//!
//! This example demonstrates how to build comprehensive test workflows using the
//! `hyperchad_test_utils` fluent API. It showcases various test scenarios including
//! navigation, form filling, HTTP requests, and control flow patterns.

use std::time::Duration;

use hyperchad_test_utils::{FormData, HttpRequestStep, Key, ScrollDirection, TestPlan, fragments};

fn main() {
    println!("=== HyperChad Test Utils - Workflow Builder Example ===\n");

    // Example 1: Simple login workflow
    println!("1. Building a simple login workflow...");
    let login_test = build_login_workflow();
    println!("   Created test plan with {} steps", login_test.steps.len());
    println!("   Steps: navigate → wait → fill form → click → wait for redirect\n");

    // Example 2: Complex form interaction
    println!("2. Building a complex form interaction workflow...");
    let form_test = build_form_workflow();
    println!("   Created test plan with {} steps", form_test.steps.len());
    println!("   Steps: navigate → fill multi-field form → upload file → submit\n");

    // Example 3: HTTP API testing
    println!("3. Building an HTTP API test workflow...");
    let api_test = build_api_workflow();
    println!("   Created test plan with {} steps", api_test.steps.len());
    println!("   Steps: GET request → POST with JSON → PUT update → DELETE\n");

    // Example 4: Control flow with loops and parallel execution
    println!("4. Building a workflow with control flow...");
    let control_flow_test = build_control_flow_workflow();
    println!(
        "   Created test plan with {} steps",
        control_flow_test.steps.len()
    );
    println!("   Steps: loop 3x (click → wait) → parallel branches\n");

    // Example 5: Keyboard and mouse interactions
    println!("5. Building keyboard and mouse interaction workflow...");
    let interaction_test = build_interaction_workflow();
    println!(
        "   Created test plan with {} steps",
        interaction_test.steps.len()
    );
    println!("   Steps: navigation → keyboard shortcuts → scrolling → hover\n");

    // Example 6: Using reusable test fragments
    println!("6. Building workflow using reusable fragments...");
    let fragment_test = build_fragment_workflow();
    println!(
        "   Created test plan with {} steps",
        fragment_test.steps.len()
    );
    println!("   Steps: login fragment → navigation fragment → logout fragment\n");

    // Example 7: Complete workflow with setup and teardown
    println!("7. Building a complete workflow with setup/teardown...");
    let complete_test = build_complete_workflow();
    println!("   Setup: {}", complete_test.setup.is_some());
    println!("   Steps: {}", complete_test.steps.len());
    println!("   Teardown: {}", complete_test.teardown.is_some());
    println!("   Timeout: {:?}", complete_test.timeout);
    println!("   Retry count: {}\n", complete_test.retry_count);

    println!("✓ All workflows built successfully!");
    println!("\nThese test plans can be serialized to JSON for external test runners,");
    println!("or executed by a test framework that implements the test step execution.");
}

/// Build a simple login workflow demonstrating basic navigation and form filling
fn build_login_workflow() -> TestPlan {
    TestPlan::new()
        .navigate_to("/login")
        .wait_for_element("#login-form")
        .fill_form(
            FormData::new()
                .text("username", "testuser")
                .text("password", "secure123"),
        )
        .click("#login-button")
        .wait_for_url("/dashboard")
}

/// Build a complex form workflow with multiple field types
fn build_form_workflow() -> TestPlan {
    TestPlan::new()
        .navigate_to("/profile/edit")
        .wait_for_element("form#profile-form")
        // Fill various form field types
        .fill_form(
            FormData::new()
                .text("name", "John Doe")
                .text("email", "john@example.com")
                .number("age", 30.0)
                .boolean("newsletter", true)
                .select("country", "US")
                .multi_select("interests", vec!["music".to_string(), "coding".to_string()]),
        )
        // Upload a profile picture
        .upload_file("#avatar", "/tmp/profile.jpg")
        // Submit the form
        .click("button[type='submit']")
        .wait_for_element(".success-message")
}

/// Build an HTTP API testing workflow
fn build_api_workflow() -> TestPlan {
    TestPlan::new()
        // GET request to fetch data
        .send_request(
            HttpRequestStep::get("https://api.example.com/users/1")
                .with_header("Authorization", "Bearer token123")
                .expect_status(200),
        )
        // POST request with JSON body
        .send_request(
            HttpRequestStep::post("https://api.example.com/users")
                .json(serde_json::json!({
                    "name": "Jane Smith",
                    "email": "jane@example.com"
                }))
                .expect_status(201),
        )
        // PUT request to update
        .send_request(
            HttpRequestStep::put("https://api.example.com/users/1")
                .json(serde_json::json!({
                    "name": "Jane Doe"
                }))
                .expect_status(200),
        )
        // DELETE request
        .send_request(
            HttpRequestStep::delete("https://api.example.com/users/1")
                .with_header("Authorization", "Bearer token123")
                .expect_status(204),
        )
}

/// Build a workflow demonstrating control flow patterns
fn build_control_flow_workflow() -> TestPlan {
    TestPlan::new()
        .navigate_to("/items")
        .wait_for_element("#item-list")
        // Loop: Click the "load more" button 3 times
        .repeat(3)
        .step(hyperchad_test_utils::TestStep::Interaction(
            hyperchad_test_utils::InteractionStep::Click {
                selector: "#load-more".to_string(),
            },
        ))
        .step(hyperchad_test_utils::TestStep::Wait(
            hyperchad_test_utils::WaitStep::Duration {
                duration: Duration::from_millis(500),
            },
        ))
        .end_repeat()
        // Parallel execution: Check multiple elements simultaneously
        .parallel()
        .branch("check-header")
        .step(hyperchad_test_utils::TestStep::Wait(
            hyperchad_test_utils::WaitStep::ElementExists {
                selector: "header".to_string(),
            },
        ))
        .branch("check-footer")
        .step(hyperchad_test_utils::TestStep::Wait(
            hyperchad_test_utils::WaitStep::ElementExists {
                selector: "footer".to_string(),
            },
        ))
        .branch("check-sidebar")
        .step(hyperchad_test_utils::TestStep::Wait(
            hyperchad_test_utils::WaitStep::ElementExists {
                selector: ".sidebar".to_string(),
            },
        ))
        .join_all()
}

/// Build a workflow demonstrating keyboard and mouse interactions
fn build_interaction_workflow() -> TestPlan {
    TestPlan::new()
        .navigate_to("/editor")
        .wait_for_element("#text-editor")
        // Focus the editor
        .focus("#text-editor")
        // Type using keyboard shortcuts (Ctrl+A to select all)
        .key_sequence(vec![Key::Control, Key::A])
        .sleep(Duration::from_millis(100))
        // Type some text
        .key_sequence(vec![Key::H, Key::E, Key::L, Key::L, Key::O])
        // Scroll down the page
        .scroll(ScrollDirection::Down, 500)
        .sleep(Duration::from_millis(300))
        // Hover over a tooltip element
        .hover("#help-icon")
        .wait_for_element(".tooltip")
        // Double-click to select a word
        .double_click(".word")
}

/// Build a workflow using reusable test fragments
fn build_fragment_workflow() -> TestPlan {
    TestPlan::new()
        // Use the pre-built login fragment
        .include(fragments::login_flow("testuser", "password123"))
        // Add some navigation testing
        .include(fragments::navigation_test())
        // Use the pre-built logout fragment
        .include(fragments::logout_flow())
}

/// Build a complete workflow with setup, teardown, timeout, and retry
fn build_complete_workflow() -> TestPlan {
    use hyperchad_test_utils::{SetupStep, TeardownStep, TestStep};

    TestPlan::new()
        // Setup: Clear cookies and set initial state
        .with_setup(SetupStep {
            description: "Clear browser state".to_string(),
            steps: vec![
                TestStep::Navigation(hyperchad_test_utils::NavigationStep::GoTo {
                    url: "/reset".to_string(),
                }),
                TestStep::Wait(hyperchad_test_utils::WaitStep::Duration {
                    duration: Duration::from_millis(500),
                }),
            ],
        })
        // Main test steps
        .navigate_to("/dashboard")
        .wait_for_element("#main-content")
        .click("#settings-button")
        .wait_for_url("/settings")
        // Teardown: Cleanup after test
        .with_teardown(TeardownStep {
            description: "Logout and clear state".to_string(),
            steps: vec![TestStep::Navigation(
                hyperchad_test_utils::NavigationStep::GoTo {
                    url: "/logout".to_string(),
                },
            )],
        })
        // Set a timeout for the entire test
        .with_timeout(Duration::from_secs(30))
        // Retry the test up to 2 times on failure
        .with_retry_count(2)
}
