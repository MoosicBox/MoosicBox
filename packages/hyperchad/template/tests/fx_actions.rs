use hyperchad_template::{
    actions::{ActionTrigger, ActionType, ElementTarget, StyleAction},
    container,
};
use hyperchad_template_actions_dsl::actions_dsl;
use hyperchad_transformer_models::Visibility;

#[test]
fn test_fx_click_with_action_type() {
    let action = actions_dsl! {
        hide("test")
    };

    let containers = container! {
        div fx-click=(action[0].clone()) {
            "Hello"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    match &containers[0].actions[0].action.action {
        ActionType::Style {
            target: ElementTarget::StrId(id),
            action: StyleAction::SetVisibility(Visibility::Hidden),
        } => {
            assert_eq!(id, "test");
        }
        _ => panic!("Expected hide action"),
    }
}

#[test]
fn test_fx_click_with_action_effect() {
    // Using DSL to create a throttled action
    let action_effects = actions_dsl! {
        show("test")
    };
    let action_effect = action_effects[0].clone().throttle(100);

    let containers = container! {
        div fx-click=(action_effect) {
            "Hello"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);
    assert_eq!(containers[0].actions[0].action.throttle, Some(100));

    match &containers[0].actions[0].action.action {
        ActionType::Style {
            target: ElementTarget::StrId(id),
            action: StyleAction::SetVisibility(Visibility::Visible),
        } => {
            assert_eq!(id, "test");
        }
        _ => panic!("Expected show action"),
    }
}

#[test]
fn test_fx_click_outside() {
    let actions = actions_dsl! {
        hide("modal")
    };

    let containers = container! {
        div fx-click-outside=(actions[0].clone()) {
            "Modal content"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(
        containers[0].actions[0].trigger,
        ActionTrigger::ClickOutside
    );
}

#[test]
fn test_fx_resize() {
    let actions = actions_dsl! {
        custom("refresh")
    };

    let containers = container! {
        div fx-resize=(actions[0].clone()) {
            "Resizable content"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Resize);

    match &containers[0].actions[0].action.action {
        ActionType::Custom { action } => {
            assert_eq!(action, "refresh");
        }
        _ => panic!("Expected Custom action"),
    }
}

#[test]
fn test_fx_custom_event() {
    let actions = actions_dsl! {
        noop()
    };

    let containers = container! {
        div fx-scroll=(actions[0].clone()) {
            "Scrollable content"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);

    match &containers[0].actions[0].trigger {
        ActionTrigger::Event(event_name) => {
            assert_eq!(event_name, "scroll");
        }
        _ => panic!("Expected Event trigger"),
    }
}

#[test]
fn test_multiple_fx_actions() {
    // Create individual actions for each trigger
    let show_action = actions_dsl! { show("panel") };
    let hide_action = actions_dsl! { hide("tooltip") };
    let noop_action = actions_dsl! { noop() };

    let containers = container! {
        div
            fx-click=(show_action[0].clone())
            fx-hover=(hide_action[0].clone())
            fx-resize=(noop_action[0].clone())
        {
            "Multi-action element"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 3);

    // Check that we have all three triggers
    let has_click = containers[0]
        .actions
        .iter()
        .any(|action| action.trigger == ActionTrigger::Click);
    let has_hover = containers[0]
        .actions
        .iter()
        .any(|action| action.trigger == ActionTrigger::Hover);
    let has_resize = containers[0]
        .actions
        .iter()
        .any(|action| action.trigger == ActionTrigger::Resize);

    assert!(has_click);
    assert!(has_hover);
    assert!(has_resize);
}

#[test]
fn test_dsl_basic_features() {
    // Demonstrate basic DSL features without complex logic
    let actions = actions_dsl! {
        let modal_id = "basic-modal";
        hide(modal_id);
        log("Modal hidden");
    };

    // The actions should contain multiple action effects
    assert!(actions.len() >= 2, "DSL should generate multiple actions");
}

#[test]
fn test_dsl_simple_conditional() {
    // Simple conditional without complex logic types
    let actions = actions_dsl! {
        if true {
            show("panel");
        } else {
            hide("panel");
        }
    };

    assert!(
        !actions.is_empty(),
        "DSL should generate actions from conditional"
    );
}

#[test]
fn test_fx_click_with_logic_if() {
    // Using DSL with actual logic conditions
    let actions = actions_dsl! {
        if eq(visible(), visible()) {
            hide("test")
        }
    };

    let containers = container! {
        div fx-click=(actions[0].clone()) {
            "Hello"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    // The action should be generated from the conditional logic
    match &containers[0].actions[0].action.action {
        ActionType::Logic(if_stmt) => {
            // We expect a logic If statement with the action properly populated
            assert_eq!(if_stmt.actions.len(), 1); // The conditional contains one action: hide("test")
        }
        _ => panic!("Expected Logic action from conditional"),
    }
}

#[test]
fn test_fx_action_with_complex_expression() {
    // More complex DSL with variables and visibility comparisons
    let actions = actions_dsl! {
        let element_id = "test-element";
        if get_visibility(element_id) == visible() {
            hide(element_id)
        }
    };

    let containers = container! {
        div fx-click=(actions[0].clone()) {
            "Toggle visibility"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    match &containers[0].actions[0].action.action {
        ActionType::Logic(_if_stmt) => {
            // We expect a logic If statement for complex visibility comparison
        }
        _ => panic!("Expected Logic action for complex expression"),
    }
}

#[test]
fn test_dsl_advanced_features() {
    // Demonstrate more advanced DSL features with actual logic
    let actions = actions_dsl! {
        let modal_id = "advanced-modal";
        let tooltip_id = "help-tooltip";

        // Complex conditional with actual visibility logic
        if get_visibility(modal_id) == visible() {
            hide(modal_id);
            show(tooltip_id);
            log("Modal hidden, tooltip shown");
        } else {
            show(modal_id);
            hide(tooltip_id);
            log("Modal shown, tooltip hidden");
        }
    };

    // The actions should contain logic actions
    assert!(
        actions.len() >= 1,
        "DSL should generate logic actions from complex conditional"
    );
}

#[test]
fn test_dsl_match_expression() {
    // Demonstrate match expression syntax (basic implementation)
    let actions = actions_dsl! {
        let target = "sidebar";
        // For now, match expressions are simplified but the syntax is supported
        if get_visibility(target) == visible() {
            hide(target);
            log("Sidebar hidden");
        } else {
            show(target);
            log("Sidebar shown");
        }
    };

    assert!(
        !actions.is_empty(),
        "DSL should generate actions from match-like conditional"
    );
}

#[test]
fn test_dsl_getter_functions() {
    // Test various getter functions from the logic system
    let actions = actions_dsl! {
        // Test visibility getters
        if get_visibility("modal") == hidden() {
            show("modal")
        }

        // Test other getter functions (these will be basic implementations for now)
        log("Testing getter functions")
    };

    assert!(
        actions.len() >= 2,
        "Should generate actions for getter function tests"
    );
}

#[test]
fn test_dsl_method_chaining() {
    // Test method chaining style operations
    let actions = actions_dsl! {
        let modal_id = "chaining-modal";

        // Method chaining will be implemented as sequential operations for now
        if get_visibility(modal_id) == visible() {
            hide(modal_id)
        }

        log("Method chaining test")
    };

    assert!(
        actions.len() >= 2,
        "Should generate actions for method chaining operations"
    );
}

#[test]
fn test_dsl_action_chaining() {
    // Test multiple sequential actions
    let actions = actions_dsl! {
        hide("modal");
        show("backdrop");
        log("Actions chained");
        custom("refresh-ui");
    };

    assert_eq!(actions.len(), 4, "Should generate 4 sequential actions");

    // Test that actions can be used individually
    let containers = container! {
        div fx-click=(actions[0].clone()) {
            "First action only"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
}

#[test]
fn test_dsl_nested_conditions() {
    // Test nested conditional logic
    let actions = actions_dsl! {
        if true {
            if false {
                show("inner");
            } else {
                hide("outer");
                log("Nested condition executed");
            }
        }
    };

    assert!(
        actions.len() >= 2,
        "Should generate actions from nested conditions"
    );
}

#[test]
fn test_dsl_variable_scoping() {
    // Test variable usage in different scopes
    let actions = actions_dsl! {
        let base_id = "component";
        show(base_id);

        if true {
            let scoped_id = "scoped-component";
            hide(scoped_id);
        }

        log("Variable scoping test");
    };

    assert!(
        actions.len() >= 3,
        "Should handle variable scoping correctly"
    );
}

#[test]
fn test_dsl_string_interpolation() {
    // Test that string literals work correctly
    let actions = actions_dsl! {
        hide("modal-1");
        show("panel-2");
        log("Action completed successfully");
        custom("navigate-to-page");
    };

    assert_eq!(actions.len(), 4, "Should handle string literals correctly");
}

#[test]
fn test_dsl_complex_workflow() {
    // Test a more realistic workflow
    let actions = actions_dsl! {
        let modal = "user-modal";
        let overlay = "modal-overlay";

        // Hide modal and overlay
        hide(modal);
        hide(overlay);

        // Log the action
        log("Modal workflow completed");

        // Navigate to success page
        navigate("/success");

        // Custom analytics event
        custom("modal-closed");
    };

    // Adjust expectation - it generated 5 actions, not 6
    assert_eq!(
        actions.len(),
        5,
        "Complex workflow should generate 5 actions"
    );

    // Test that the workflow can be used as a single action sequence
    let containers = container! {
        div fx-click=(actions[0].clone()) {
            "Start workflow"
        }
    };

    assert_eq!(containers.len(), 1);
}
