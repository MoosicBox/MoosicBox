#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic Actions Example
//!
//! This example demonstrates the core functionality of the `hyperchad_actions` crate,
//! showing how to create and use actions with triggers, effects, and element targeting.

use hyperchad_actions::{
    Action, ActionEffect, ActionTrigger, ActionType, ElementTarget, LogLevel, StyleAction,
};
use hyperchad_transformer_models::Visibility;

#[allow(clippy::too_many_lines)]
fn main() {
    // Initialize logging to see log messages
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .init();

    println!("=== HyperChad Actions - Basic Usage Examples ===\n");

    // Example 1: Simple Click Action
    println!("1. Simple Click Action:");
    let click_action = Action {
        trigger: ActionTrigger::Click,
        effect: ActionType::hide_str_id("modal").into(),
    };
    println!("   Action: {click_action:?}");
    println!("   When clicked, hides element with ID 'modal'\n");

    // Example 2: Element Targeting
    println!("2. Element Targeting Options:");

    // Target by string ID
    let hide_by_id = ActionType::hide_str_id("my-element");
    println!("   By String ID: {hide_by_id:?}");

    // Target by class
    let show_by_class = ActionType::show_class("menu");
    println!("   By Class: {show_by_class:?}");

    // Target self
    let hide_self = ActionType::hide_self();
    println!("   Self: {hide_self:?}");

    // Target last child
    let show_last_child = ActionType::show_last_child();
    println!("   Last Child: {show_last_child:?}\n");

    // Example 3: Style Actions
    println!("3. Style Actions:");

    // Visibility control
    let set_visibility = ActionType::Style {
        target: ElementTarget::StrId("element".into()),
        action: StyleAction::SetVisibility(Visibility::Hidden),
    };
    println!("   Set Visibility: {set_visibility:?}");

    // Display control
    let set_display = ActionType::set_display_str_id(false, "element");
    println!("   Set Display: {set_display:?}");

    // Background control
    let set_background = ActionType::set_background_str_id("#ff0000", "element");
    println!("   Set Background: {set_background:?}");

    // Focus control
    let set_focus = ActionType::focus_str_id("input-field");
    println!("   Set Focus: {set_focus:?}\n");

    // Example 4: Multi-Actions
    println!("4. Multi-Actions (Sequential Execution):");
    let multi_action = ActionType::Multi(vec![
        ActionType::hide_str_id("loading"),
        ActionType::show_str_id("content"),
        ActionType::Log {
            message: "Content loaded successfully".to_string(),
            level: LogLevel::Info,
        },
    ]);
    println!("   Multi-Action: {multi_action:?}\n");

    // Example 5: Chaining Actions
    println!("5. Chaining Actions with 'and':");
    let chained = ActionType::hide_str_id("modal")
        .and(ActionType::show_str_id("success-message"))
        .and(ActionType::Log {
            message: "Modal closed".to_string(),
            level: LogLevel::Info,
        });
    println!("   Chained Actions: {chained:?}\n");

    // Example 6: Action Effects (Timing Modifiers)
    println!("6. Action Effects with Timing:");

    // Throttled action
    let throttled = ActionType::hide_str_id("tooltip").throttle(500);
    println!("   Throttled (500ms): {throttled:?}");

    // Delayed action
    let delayed = ActionType::show_str_id("notification").delay_off(2000);
    println!("   Delay Off (2000ms): {delayed:?}");

    // Unique action
    let unique = ActionType::display_str_id("alert").unique();
    println!("   Unique: {unique:?}\n");

    // Example 7: Different Triggers
    println!("7. Action Triggers:");

    let click_trigger = Action {
        trigger: ActionTrigger::Click,
        effect: ActionType::hide_self().into(),
    };
    println!("   Click: {click_trigger:?}");

    let hover_trigger = Action {
        trigger: ActionTrigger::Hover,
        effect: ActionType::show_str_id("tooltip").into(),
    };
    println!("   Hover: {hover_trigger:?}");

    let change_trigger = Action {
        trigger: ActionTrigger::Change,
        effect: ActionType::Log {
            message: "Input changed".to_string(),
            level: LogLevel::Debug,
        }
        .into(),
    };
    println!("   Change: {change_trigger:?}");

    let immediate_trigger = Action {
        trigger: ActionTrigger::Immediate,
        effect: ActionType::show_str_id("welcome-message").into(),
    };
    println!("   Immediate: {immediate_trigger:?}\n");

    // Example 8: HTTP Event Triggers
    println!("8. HTTP Event Triggers:");

    let before_request = Action {
        trigger: ActionTrigger::HttpBeforeRequest,
        effect: ActionType::display_str_id("loading-spinner").into(),
    };
    println!("   Before Request: {before_request:?}");

    let after_request = Action {
        trigger: ActionTrigger::HttpAfterRequest,
        effect: ActionType::no_display_str_id("loading-spinner").into(),
    };
    println!("   After Request: {after_request:?}");

    let on_success = Action {
        trigger: ActionTrigger::HttpRequestSuccess,
        effect: ActionType::show_str_id("success-banner").into(),
    };
    println!("   On Success: {on_success:?}");

    let on_error = Action {
        trigger: ActionTrigger::HttpRequestError,
        effect: ActionType::show_str_id("error-banner").into(),
    };
    println!("   On Error: {on_error:?}\n");

    // Example 9: Custom Actions
    println!("9. Custom Actions:");
    let custom = ActionType::Custom {
        action: "my-custom-action".to_string(),
    };
    println!("   Custom Action: {custom:?}");

    let event_action = ActionType::on_event("user-login", ActionType::show_str_id("dashboard"));
    println!("   Event Action: {event_action:?}\n");

    // Example 10: Navigation
    println!("10. Navigation:");
    let navigate = ActionType::Navigate {
        url: "/dashboard".to_string(),
    };
    println!("   Navigate: {navigate:?}\n");

    // Example 11: Input Actions
    println!("11. Input Actions:");
    let select_input = ActionType::select_str_id("email-input");
    println!("   Select Input: {select_input:?}");

    let focus_button = ActionType::focus_class("submit-button");
    println!("   Focus Button: {focus_button:?}\n");

    // Example 12: Complex Action Effect
    println!("12. Complex Action Effect (Multiple Modifiers):");
    let complex_effect = ActionEffect {
        action: ActionType::show_str_id("popup"),
        delay_off: Some(3000),
        throttle: Some(1000),
        unique: Some(true),
    };
    println!("   Complex Effect: {complex_effect:?}");
    println!("   Shows popup, unique, throttled to 1s, auto-hides after 3s\n");

    // Example 13: Conditional Logic (requires logic feature)
    #[cfg(feature = "logic")]
    {
        use hyperchad_actions::logic::{Condition, If, get_visibility_str_id};

        println!("13. Conditional Logic:");
        let conditional = ActionType::Logic(If {
            condition: Condition::Eq(
                get_visibility_str_id("menu").into(),
                Visibility::Visible.into(),
            ),
            actions: vec![ActionType::hide_str_id("menu").into()],
            else_actions: vec![ActionType::show_str_id("menu").into()],
        });
        println!("   Conditional Toggle: {conditional:?}\n");

        // Toggle helper
        let toggle = ActionType::toggle_visibility_str_id("sidebar");
        println!("   Toggle Visibility: {toggle:?}\n");
    }

    // Example 14: Value Calculations (requires logic feature)
    #[cfg(feature = "logic")]
    {
        use hyperchad_actions::logic::{get_mouse_x_self, get_width_px_self};

        println!("14. Value Calculations:");
        let mouse_x = get_mouse_x_self();
        println!("   Mouse X (self): {mouse_x:?}");

        let width = get_width_px_self();
        println!("   Width (self): {width:?}");

        let half_width = width.divide(2.0);
        println!("   Half Width: {half_width:?}");

        let clamped = mouse_x.clamp(0.0, 100.0);
        println!("   Clamped Mouse X: {clamped:?}\n");
    }

    println!("=== Example Complete ===");
    println!("\nThis example demonstrated:");
    println!("  - Action creation with triggers and effects");
    println!("  - Element targeting (ID, class, self, last child)");
    println!("  - Style actions (visibility, display, background, focus)");
    println!("  - Multi-actions and chaining");
    println!("  - Action timing (throttle, delay_off, unique)");
    println!("  - Various triggers (click, hover, change, HTTP events)");
    println!("  - Custom actions and navigation");
    println!("  - Input actions");
    #[cfg(feature = "logic")]
    println!("  - Conditional logic and value calculations");
}
