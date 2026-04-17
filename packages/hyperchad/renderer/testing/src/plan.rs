use std::time::Instant;

use hyperchad_renderer::View;
use hyperchad_renderer::transformer::{Container, Element};
use hyperchad_test_utils::{
    ControlStep, FormStep, InteractionStep, NavigationStep, TestPlan, TestResult, TestStep,
    WaitStep,
};

use crate::{Harness, client::event::CustomEvent};

/// Executes a `hyperchad_test_utils::TestPlan` with deterministic in-process semantics.
#[must_use]
pub fn execute_test_plan(plan: &TestPlan, routes: &[String]) -> TestResult {
    let started = Instant::now();
    let mut harness = Harness::with_default_renderer();

    for (index, route) in routes.iter().enumerate() {
        harness.route_full(route, dummy_route_view(route, index.saturating_add(1)));
    }

    if routes.is_empty() {
        harness.route_full("/", dummy_route_view("/", 1));
    }

    let mut result = TestResult::success();

    if let Some(setup) = &plan.setup {
        run_steps(&mut harness, &setup.steps, &mut result);
    }

    run_steps(&mut harness, &plan.steps, &mut result);

    if let Some(teardown) = &plan.teardown {
        run_steps(&mut harness, &teardown.steps, &mut result);
    }

    result.execution_time = started.elapsed();
    result
}

fn run_steps(harness: &mut Harness, steps: &[TestStep], result: &mut TestResult) {
    for step in steps {
        result.steps_executed = result.steps_executed.saturating_add(1);
        if !run_step(harness, step, result) {
            result.success = false;
            break;
        }
    }
}

fn run_step(harness: &mut Harness, step: &TestStep, result: &mut TestResult) -> bool {
    match step {
        TestStep::Navigation(step) => run_navigation_step(harness, step, result),
        TestStep::Interaction(step) => run_interaction_step(harness, step, result),
        TestStep::Form(step) => run_form_step(harness, step, result),
        TestStep::Http(step) => {
            result.add_warning(format!(
                "HTTP step not executed in-process: {}",
                step.description()
            ));
            true
        }
        TestStep::Wait(step) => run_wait_step(harness, step, result),
        TestStep::Control(step) => run_control_step(harness, step, result),
    }
}

fn run_navigation_step(
    harness: &mut Harness,
    step: &NavigationStep,
    result: &mut TestResult,
) -> bool {
    let status = match step {
        NavigationStep::GoTo { url } => harness.navigate_to(url),
        NavigationStep::SetHash { hash } => harness.navigate_to(hash),
        NavigationStep::GoBack | NavigationStep::GoForward | NavigationStep::Reload => {
            result.add_warning(format!(
                "Navigation operation not simulated: {}",
                step.description()
            ));
            Ok(())
        }
    };

    if let Err(err) = status {
        result.add_error(err.to_string());
        return false;
    }

    true
}

fn run_interaction_step(
    harness: &mut Harness,
    step: &InteractionStep,
    result: &mut TestResult,
) -> bool {
    let status = match step {
        InteractionStep::Click { selector } => harness.click(selector).map(|_| ()),
        InteractionStep::Hover { selector } => harness
            .dispatch(
                selector,
                crate::client::core::event_types::HOVER,
                None,
                None,
                None,
            )
            .map(|_| ()),
        InteractionStep::Focus { selector } => harness
            .dispatch_custom_event(selector, CustomEvent::new("focus", None))
            .map(|_| ()),
        InteractionStep::Blur { selector } => harness
            .dispatch_custom_event(selector, CustomEvent::new("blur", None))
            .map(|_| ()),
        InteractionStep::MouseDown { .. }
        | InteractionStep::MouseUp { .. }
        | InteractionStep::MouseMove { .. }
        | InteractionStep::KeyPress { .. }
        | InteractionStep::KeySequence { .. }
        | InteractionStep::Scroll { .. }
        | InteractionStep::RightClick { .. }
        | InteractionStep::DoubleClick { .. }
        | InteractionStep::DragAndDrop { .. } => {
            result.add_warning(format!(
                "Interaction operation not simulated: {}",
                step.description()
            ));
            Ok(())
        }
    };

    if let Err(err) = status {
        result.add_error(err.to_string());
        return false;
    }

    true
}

fn run_form_step(harness: &mut Harness, step: &FormStep, result: &mut TestResult) -> bool {
    let status = match step {
        FormStep::SubmitForm { selector } => harness.click(selector).map(|_| ()),
        FormStep::ResetForm { .. }
        | FormStep::FillForm { .. }
        | FormStep::FillField { .. }
        | FormStep::SelectOption { .. }
        | FormStep::UploadFile { .. } => {
            result.add_warning(format!(
                "Form operation not simulated: {}",
                step.description()
            ));
            Ok(())
        }
    };

    if let Err(err) = status {
        result.add_error(err.to_string());
        return false;
    }
    true
}

fn run_wait_step(harness: &mut Harness, step: &WaitStep, result: &mut TestResult) -> bool {
    match step {
        WaitStep::ElementExists { selector } => {
            if let Err(err) = harness.assert_selector_exists(selector) {
                result.add_error(err.to_string());
                return false;
            }
        }
        WaitStep::UrlContains { fragment } => {
            let has_match = harness
                .navigation_history()
                .last()
                .is_some_and(|url| url.contains(fragment));
            if !has_match {
                result.add_error(format!(
                    "Wait condition failed: url does not contain '{fragment}'"
                ));
                return false;
            }
        }
        WaitStep::Duration { .. } => {
            // Deterministic no-op: waiting does not advance any real clock.
        }
    }
    true
}

fn run_control_step(harness: &mut Harness, step: &ControlStep, result: &mut TestResult) -> bool {
    match step {
        ControlStep::Loop { count, steps } => {
            for _ in 0..*count {
                run_steps(harness, steps, result);
                if !result.success {
                    return false;
                }
            }
            true
        }
        ControlStep::Parallel { branches } => {
            for branch in branches.values() {
                run_steps(harness, branch, result);
                if !result.success {
                    return false;
                }
            }
            true
        }
        ControlStep::ForEach { data, steps } => {
            for _ in data {
                run_steps(harness, steps, result);
                if !result.success {
                    return false;
                }
            }
            true
        }
        ControlStep::Try {
            steps,
            catch_steps,
            finally_steps,
        } => {
            let success_before = result.success;
            run_steps(harness, steps, result);
            if !result.success {
                if let Some(catch_steps) = catch_steps {
                    run_steps(harness, catch_steps, result);
                }
            } else {
                result.success = success_before;
            }

            if let Some(finally_steps) = finally_steps {
                run_steps(harness, finally_steps, result);
            }

            result.success
        }
        ControlStep::Retry {
            steps,
            max_attempts,
            delay,
        } => {
            let mut retries = 0_u32;
            loop {
                let mut local = TestResult::success();
                run_steps(harness, steps, &mut local);
                if local.success {
                    break;
                }

                retries = retries.saturating_add(1);
                if retries > *max_attempts {
                    result.add_error("Retry exhausted".to_string());
                    return false;
                }

                if delay.is_some_and(|delay| delay > std::time::Duration::ZERO) {
                    result.add_warning("Retry delay ignored in deterministic mode".to_string());
                }
            }
            true
        }
    }
}

fn dummy_route_view(path: &str, id: usize) -> View {
    let path_id = sanitize_path(path);
    View::builder()
        .with_primary(Container {
            id,
            str_id: Some(path_id),
            element: Element::Div,
            children: vec![Container {
                id: id.saturating_add(10_000),
                str_id: Some("route-content".to_string()),
                element: Element::Raw {
                    value: path.to_string(),
                },
                ..Default::default()
            }],
            ..Default::default()
        })
        .build()
}

fn sanitize_path(path: &str) -> String {
    let normalized = path.trim_matches('/').replace('/', "-");
    if normalized.is_empty() {
        "route-root".to_string()
    } else {
        format!("route-{normalized}")
    }
}
