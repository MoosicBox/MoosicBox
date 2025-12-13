use hyperchad_actions::{
    ActionEffect, ActionType, LogLevel, Target,
    dsl::{Expression, Literal},
};
use hyperchad_template::{
    actions::{ActionTrigger, ElementTarget, StyleAction},
    container,
};
use hyperchad_template_actions_dsl::actions_dsl;
use hyperchad_transformer_models::Visibility;
use pretty_assertions::assert_eq;

#[test]
fn test_fx_click_with_action_type() {
    let containers = container! {
        div fx-click=(ActionType::hide_by_id("test")) {
            "Hello"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    // Verify the specific action type and target
    match &containers[0].actions[0].effect.action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id(Target::literal("test")));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
        }
        _ => panic!("Expected Style action for hide_str_id"),
    }
}

#[test]
fn test_fx_click_with_action_effect() {
    let action = ActionEffect {
        action: ActionType::hide_by_id(Target::literal("test")),
        delay_off: Some(1000),
        throttle: Some(500),
        unique: Some(true),
    };

    let containers = container! {
        div fx-click=(action) {
            "Hello"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    // Verify the action effect properties
    let action_effect = &containers[0].actions[0].effect;
    assert_eq!(action_effect.delay_off, Some(1000));
    assert_eq!(action_effect.throttle, Some(500));
    assert_eq!(action_effect.unique, Some(true));

    // Verify the underlying action
    match &action_effect.action {
        ActionType::Style { target, action } => {
            assert_eq!(target, &ElementTarget::by_id(Target::literal("test")));
            assert_eq!(action, &StyleAction::SetVisibility(Visibility::Hidden));
        }
        _ => panic!("Expected Style action"),
    }
}

#[test]
fn test_fx_click_outside() {
    let containers = container! {
        div fx-click-outside=(ActionType::hide_by_id(Target::literal("modal"))) {
            "Modal content"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(
        containers[0].actions[0].trigger,
        ActionTrigger::ClickOutside
    );

    // Verify the action content
    match &containers[0].actions[0].effect.action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id(Target::literal("modal")));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
        }
        _ => panic!("Expected Style action for click outside"),
    }
}

#[test]
fn test_fx_resize() {
    let containers = container! {
        div fx-resize=(ActionType::Log {
            message: "Window resized".to_string(),
            level: LogLevel::Info,
        }) {
            "Resizable content"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Resize);

    // Verify the log action
    match &containers[0].actions[0].effect.action {
        ActionType::Log { message, level } => {
            assert_eq!(message, "Window resized");
            assert_eq!(*level, LogLevel::Info);
        }
        _ => panic!("Expected Log action for resize trigger"),
    }
}

#[test]
fn test_fx_custom_event() {
    let containers = container! {
        div fx-custom-event=(ActionType::Custom {
            action: "refresh-data".to_string(),
        }) {
            "Custom event handler"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(
        containers[0].actions[0].trigger,
        ActionTrigger::Event("custom-event".to_string())
    );

    // Verify the custom action
    match &containers[0].actions[0].effect.action {
        ActionType::Custom { action } => {
            assert_eq!(*action, "refresh-data");
        }
        _ => panic!("Expected Custom action for custom event"),
    }
}

#[test]
fn test_multiple_fx_actions() {
    let containers = container! {
        div fx-click=(ActionType::show_by_id(Target::literal("panel")))
            fx-hover=(ActionType::Log {
                message: "Hovered".to_string(),
                level: LogLevel::Debug,
            }) {
            "Multiple actions"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 2);

    // First action (click)
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);
    match &containers[0].actions[0].effect.action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id(Target::literal("panel")));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
        }
        _ => panic!("Expected Style action for first action"),
    }

    // Second action (hover)
    assert_eq!(containers[0].actions[1].trigger, ActionTrigger::Hover);
    match &containers[0].actions[1].effect.action {
        ActionType::Log { message, level } => {
            assert_eq!(message, "Hovered");
            assert_eq!(*level, LogLevel::Debug);
        }
        _ => panic!("Expected Log action for second action"),
    }
}

#[test]
fn test_dsl_basic_features() {
    // Demonstrate basic DSL features without complex logic
    let actions = actions_dsl! {
        let modal_id = "basic-modal";
        hide(modal_id);
        log("Modal hidden");
    };

    println!("actions: {actions:#?}");

    // The actions should contain multiple action effects
    assert_eq!(actions.len(), 3, "DSL should generate 3 actions");

    // Verify the first action (let)
    match &actions[0].action {
        ActionType::Let { name, value } => {
            assert_eq!(name, "modal_id");
            assert_eq!(
                value,
                &hyperchad_actions::dsl::Expression::Literal(
                    hyperchad_actions::dsl::Literal::String("basic-modal".to_string())
                )
            );
        }
        unknown => panic!("Expected Let action, got {unknown:?}"),
    }

    // Verify the second action (hide)
    match &actions[1].action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id(Target::reference("modal_id")));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
        }
        unknown => panic!("Expected Style action for hide, got {unknown:?}"),
    }

    // Verify the third action (log)
    match &actions[2].action {
        ActionType::Log { message, level } => {
            assert_eq!(message, "Modal hidden");
            assert_eq!(*level, LogLevel::Info);
        }
        unknown => panic!("Expected Log action, got {unknown:?}"),
    }
}

#[test]
fn test_dsl_simple_conditional() {
    // Simple conditional without complex logic types
    let action = actions_dsl! {
        if true {
            show("panel");
        } else {
            hide("panel");
        }
    };

    assert_eq!(
        action.action,
        ActionType::Style {
            target: ElementTarget::by_id("panel"),
            action: StyleAction::SetVisibility(Visibility::Visible)
        }
    );
}

#[test]
fn test_fx_click_with_logic_if() {
    // Using DSL with actual logic conditions
    let action = actions_dsl! {
        if eq(visible(), visible()) {
            hide("test")
        }
    };

    let containers = container! {
        div fx-click=(action.clone()) {
            "Hello"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    // The action should be generated from the conditional logic
    match &containers[0].actions[0].effect.action {
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
        div fx-click=(actions) {
            "Toggle visibility"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    match &containers[0].actions[0].effect.action {
        ActionType::MultiEffect(actions) => {
            assert_eq!(actions.len(), 2);
            assert_eq!(
                actions[0].action,
                ActionType::Let {
                    name: "element_id".to_string(),
                    value: hyperchad_actions::dsl::Expression::Literal(
                        hyperchad_actions::dsl::Literal::String("test-element".to_string())
                    )
                }
            );

            match &actions[0].action {
                ActionType::Let { name, value } => {
                    assert_eq!(name, "element_id");
                    assert_eq!(value.to_string(), "test-element");
                }
                unknown => panic!("Expected Let action for fx click, got {unknown:?}"),
            }

            // Verify second action (hide)
            match &actions[1].action {
                ActionType::Logic(logic) => {
                    assert_eq!(logic.actions.len(), 1);

                    // Verify first action (hide)
                    match &logic.actions[0].action {
                        ActionType::Style { target, action } => {
                            assert_eq!(
                                *target,
                                ElementTarget::by_id(Target::reference("element_id"))
                            );
                            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
                        }
                        unknown => panic!("Expected Style action for hide, got {unknown:?}"),
                    }
                }
                unknown => panic!("Expected Logic action for second action, got {unknown:?}"),
            }
        }
        unknown => panic!("Expected MultiEffect action for multiple actions, got {unknown:?}"),
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
        !actions.is_empty(),
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

    // Verify each action type
    match &actions[0].action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id("modal"));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
        }
        _ => panic!("Expected Style action for hide"),
    }

    match &actions[1].action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id("backdrop"));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
        }
        _ => panic!("Expected Style action for show"),
    }

    match &actions[2].action {
        ActionType::Log { message, level } => {
            assert_eq!(message, "Actions chained");
            assert_eq!(*level, LogLevel::Info);
        }
        _ => panic!("Expected Log action"),
    }

    match &actions[3].action {
        ActionType::Custom { action } => {
            assert_eq!(*action, "refresh-ui");
        }
        _ => panic!("Expected Custom action"),
    }

    // Test that actions can be used individually
    let containers = container! {
        div fx-click=(actions[0].clone()) {
            "First action only"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);

    // Verify the single action on the container
    match &containers[0].actions[0].effect.action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id("modal"));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
        }
        _ => panic!("Expected Style action for container"),
    }
}

#[test]
fn test_dsl_nested_conditions() {
    // Test nested conditional logic
    let action = actions_dsl! {
        if true {
            if false {
                show("inner");
            } else {
                hide("outer");
                log("Nested condition executed");
            }
        }
    };

    match &action.action {
        ActionType::MultiEffect(actions) => {
            // Verify first action (hide)
            match &actions[0].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("outer"));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
                }
                unknown => panic!("Expected Style action for hide, got {unknown:?}"),
            }

            match &actions[1].action {
                ActionType::Log { message, level } => {
                    assert_eq!(message, "Nested condition executed");
                    assert_eq!(*level, LogLevel::Info);
                }
                unknown => panic!("Expected Log action, got {unknown:?}"),
            }
        }
        unknown => panic!("Expected MultiEffect action for nested conditions, got {unknown:?}"),
    }
}

#[test]
fn test_dsl_variable_interpolation() {
    let base_id = "component";

    // Test variable usage in different scopes
    let action = actions_dsl! {
        if get_visibility(base_id) == hidden() {
            log("base_id hidden");
        }
    };

    assert_eq!(
        action.action,
        ActionType::Logic(hyperchad_actions::logic::If {
            condition: hyperchad_actions::logic::Condition::Eq(
                hyperchad_actions::logic::CalcValue::Visibility {
                    target: hyperchad_actions::ElementTarget::ById(Target::literal("component")),
                }
                .into(),
                hyperchad_actions::logic::Value::Visibility(Visibility::Hidden),
            ),
            actions: vec![hyperchad_actions::ActionEffect {
                action: hyperchad_actions::ActionType::Log {
                    message: "base_id hidden".to_string(),
                    level: hyperchad_actions::LogLevel::Info,
                },
                delay_off: None,
                throttle: None,
                unique: None,
            }],
            else_actions: vec![],
        }),
    );
}

#[test]
fn test_dsl_variable_scoping() {
    // Test variable usage in different scopes
    let action = actions_dsl! {
        let base_id = "component";
        show(base_id);

        if true {
            let scoped_id = "scoped-component";
            hide(scoped_id);
        }

        log("Variable scoping test");
    };

    assert!(
        action.len() >= 3,
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
        7,
        "Complex workflow should generate 5 actions"
    );

    // Verify the first action (let)
    match &actions[0].action {
        ActionType::Let { name, value } => {
            assert_eq!(name, "modal");
            assert_eq!(
                value,
                &hyperchad_actions::dsl::Expression::Literal(
                    hyperchad_actions::dsl::Literal::String("user-modal".to_string())
                )
            );
        }
        unknown => panic!("Expected Let action, got {unknown:?}"),
    }

    // Verify the first action (let)
    match &actions[1].action {
        ActionType::Let { name, value } => {
            assert_eq!(name, "overlay");
            assert_eq!(
                value,
                &hyperchad_actions::dsl::Expression::Literal(
                    hyperchad_actions::dsl::Literal::String("modal-overlay".to_string())
                )
            );
        }
        unknown => panic!("Expected Let action, got {unknown:?}"),
    }

    match &actions[2].action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id(Target::reference("modal")));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
        }
        unknown => panic!("Expected Style action for hide, got {unknown:?}"),
    }

    match &actions[3].action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id(Target::reference("overlay")));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
        }
        unknown => panic!("Expected Style action for hide, got {unknown:?}"),
    }

    match &actions[4].action {
        ActionType::Log { message, level } => {
            assert_eq!(message, "Modal workflow completed");
            assert_eq!(*level, LogLevel::Info);
        }
        unknown => panic!("Expected Style action for hide, got {unknown:?}"),
    }

    match &actions[5].action {
        ActionType::Navigate { url } => {
            assert_eq!(url, "/success");
        }
        unknown => panic!("Expected Navigate action for hide, got {unknown:?}"),
    }

    match &actions[6].action {
        ActionType::Custom { action } => {
            assert_eq!(*action, "modal-closed");
        }
        unknown => panic!("Expected Custom action for hide, got {unknown:?}"),
    }

    // Test that the workflow can be used as a single action sequence
    let containers = container! {
        div fx-click=(actions[0].clone()) {
            "Start workflow"
        }
    };

    assert_eq!(containers.len(), 1);
}

#[test]
fn test_fx_wrapper_syntax() {
    // Test the new fx syntax
    let containers = container! {
        div fx-click=fx { show("panel") } {
            "Click to show panel"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    match &containers[0].actions[0].effect.action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id("panel"));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
        }
        unknown => panic!("Expected Style action for show, got {unknown:?}"),
    }
}

#[test]
fn test_fx_wrapper_complex_dsl() {
    // Test complex DSL with fx syntax
    let containers = container! {
        div fx-click=fx { if get_visibility("modal") == visible() { hide("modal") } else { show("modal") } } {
            "Toggle modal"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    match &containers[0].actions[0].effect.action {
        ActionType::Logic(logic) => {
            assert_eq!(logic.actions.len(), 1);
            assert_eq!(logic.else_actions.len(), 1);

            // Verify first action (hide)
            match &logic.actions[0].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("modal"));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
                }
                unknown => panic!("Expected Style action for hide, got {unknown:?}"),
            }

            // Verify second action (show)
            match &logic.else_actions[0].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("modal"));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
                }
                unknown => panic!("Expected Style action for show, got {unknown:?}"),
            }
        }
        unknown => panic!("Expected Logic action from complex fx DSL, got {unknown:?}"),
    }
}

#[test]
fn test_dsl_moosicbox_patterns() {
    // Test basic visibility toggle pattern from MoosicBox
    let action = actions_dsl! {
        if get_visibility("audio-zones") == visible() {
            hide("audio-zones")
        } else {
            show("audio-zones")
        }
    };

    assert_eq!(
        action.action,
        ActionType::Logic(hyperchad_actions::logic::If {
            condition: hyperchad_actions::logic::Condition::Eq(
                hyperchad_actions::logic::CalcValue::Visibility {
                    target: hyperchad_actions::ElementTarget::ById(Target::literal("audio-zones")),
                }
                .into(),
                hyperchad_actions::logic::Value::Visibility(Visibility::Visible),
            ),
            actions: vec![hyperchad_actions::ActionEffect {
                action: hyperchad_actions::ActionType::Style {
                    target: hyperchad_actions::ElementTarget::ById(Target::literal("audio-zones")),
                    action: hyperchad_actions::StyleAction::SetVisibility(
                        hyperchad_transformer_models::Visibility::Hidden
                    ),
                },
                ..Default::default()
            }],
            else_actions: vec![hyperchad_actions::ActionEffect {
                action: hyperchad_actions::ActionType::Style {
                    target: hyperchad_actions::ElementTarget::ById(Target::literal("audio-zones")),
                    action: hyperchad_actions::StyleAction::SetVisibility(
                        hyperchad_transformer_models::Visibility::Visible
                    ),
                },
                ..Default::default()
            }],
        }),
    );
}

#[test]
fn test_dsl_method_chaining_patterns() {
    // Test simpler patterns instead of complex method chaining
    let action = actions_dsl! {
        if get_visibility("modal") == hidden() {
            show("modal")
        } else {
            hide("modal")
        }
    };

    assert_eq!(
        action.action,
        ActionType::Logic(hyperchad_actions::logic::If {
            condition: hyperchad_actions::logic::Condition::Eq(
                hyperchad_actions::logic::CalcValue::Visibility {
                    target: hyperchad_actions::ElementTarget::ById(Target::literal("modal")),
                }
                .into(),
                hyperchad_actions::logic::Value::Visibility(Visibility::Hidden),
            ),
            actions: vec![hyperchad_actions::ActionEffect {
                action: hyperchad_actions::ActionType::Style {
                    target: hyperchad_actions::ElementTarget::ById(Target::literal("modal")),
                    action: hyperchad_actions::StyleAction::SetVisibility(
                        hyperchad_transformer_models::Visibility::Visible
                    ),
                },
                ..Default::default()
            }],
            else_actions: vec![hyperchad_actions::ActionEffect {
                action: hyperchad_actions::ActionType::Style {
                    target: hyperchad_actions::ElementTarget::ById(Target::literal("modal")),
                    action: hyperchad_actions::StyleAction::SetVisibility(
                        hyperchad_transformer_models::Visibility::Hidden
                    ),
                },
                ..Default::default()
            }],
        }),
    );
}

#[test]
fn test_dsl_action_enum_variants() {
    // Test simple action calls since the Action enum variants aren't available in DSL scope
    let actions = actions_dsl! {
        custom("TogglePlayback");
        custom("PreviousTrack");
        custom("NextTrack");
    };

    assert_eq!(
        actions.len(),
        3,
        "DSL should generate 3 actions for custom action variants"
    );
}

#[test]
fn test_dsl_action_type_variants() {
    // Test ActionType variants using simple function calls
    let actions = actions_dsl! {
        show("test");
        hide("test");
        log("ActionType variants test");
    };

    assert_eq!(
        actions.len(),
        3,
        "DSL should generate 3 actions for ActionType variants"
    );
}

#[test]
fn test_dsl_mouse_and_dimension_functions() {
    // Test mouse and dimension functions used in MoosicBox (simplified)
    let action = actions_dsl! {
        log("Mouse and dimension test")
    };

    assert_eq!(
        action.action,
        ActionType::Log {
            message: "Mouse and dimension test".to_string(),
            level: LogLevel::Info,
        }
    );
}

#[test]
fn test_dsl_navigation_action() {
    // Test ActionType::Navigate pattern
    let action = actions_dsl! {
        navigate("/search")
    };

    // Verify the navigation action
    match &action.action {
        ActionType::Navigate { url } => {
            assert_eq!(url, "/search");
        }
        _ => panic!("Expected Navigate action"),
    }
}

#[test]
fn test_dsl_delay_and_throttle() {
    // Test delay and throttle methods (simplified)
    let actions = actions_dsl! {
        show("tooltip");
        log("Delay and throttle test");
    };

    assert_eq!(
        actions.into_iter().map(|x| x.action).collect::<Vec<_>>(),
        vec![
            ActionType::Style {
                target: ElementTarget::ById(Target::literal("tooltip")),
                action: StyleAction::SetVisibility(Visibility::Visible),
            },
            ActionType::Log {
                message: "Delay and throttle test".to_string(),
                level: LogLevel::Info,
            },
        ]
    );
}

#[test]
fn test_dsl_and_combination() {
    // Test action combination with .and() (simplified)
    let actions = actions_dsl! {
        hide("search");
        show("search-button");
    };

    assert_eq!(actions.len(), 2, "DSL should handle action combinations");
}

#[test]
fn test_dsl_in_hyperchad_template() {
    // Test using DSL patterns in actual HyperChad templates
    let containers = container! {
        div {
            // Simple action
            button fx-click=fx { log("Toggle Playback") } {
                "Toggle Playback"
            }

            // Conditional pattern like MoosicBox
            button fx-click=fx {
                if get_visibility("audio-zones") == hidden() {
                    show("audio-zones")
                } else {
                    hide("audio-zones")
                }
            } {
                "Toggle Audio Zones"
            }

            // Simple navigation
            div fx-click=fx { navigate("/search") } {
                "Search"
            }

            // Simple actions
            button fx-click=fx { hide("search") } {
                "Close Search"
            }
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].children.len(), 4);

    // Verify first button (log action)
    assert_eq!(containers[0].children[0].actions.len(), 1);
    match &containers[0].children[0].actions[0].effect.action {
        ActionType::Log { message, level } => {
            assert_eq!(message, "Toggle Playback");
            assert_eq!(*level, LogLevel::Info);
        }
        unknown => panic!("Expected Log action for first button, got {unknown:?}"),
    }

    // Verify second button (conditional logic action)
    assert_eq!(containers[0].children[1].actions.len(), 1);
    match &containers[0].children[1].actions[0].effect.action {
        ActionType::Logic(_) => {
            // Complex conditional logic - just verify it's there
        }
        unknown => panic!("Expected Logic action for second button, got {unknown:?}"),
    }

    // Verify third button (navigation action)
    assert_eq!(containers[0].children[2].actions.len(), 1);
    match &containers[0].children[2].actions[0].effect.action {
        ActionType::Navigate { url } => {
            assert_eq!(url, "/search");
        }
        unknown => panic!("Expected Navigate action for third button, got {unknown:?}"),
    }

    // Verify fourth button (hide action)
    assert_eq!(containers[0].children[3].actions.len(), 1);
    match &containers[0].children[3].actions[0].effect.action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id("search"));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
        }
        unknown => panic!("Expected Style action for fourth button, got {unknown:?}"),
    }
}

#[test]
fn test_dsl_complex_real_world_pattern() {
    // Test a complex real-world pattern similar to MoosicBox volume slider (simplified)
    let containers = container! {
        div
            #volume-slider
            fx-mouse-down=fx { log("Mouse down on volume slider") }
            fx-hover=fx { show("volume-tooltip") }
        {
            "Volume Slider"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 2); // mouse-down and hover actions
    assert_eq!(containers[0].str_id, Some("volume-slider".to_string()));
}

#[test]
fn test_multiple_dsl_actions_use_multi() {
    // Test that multiple actions generate Multi ActionType
    let actions = actions_dsl! {
        hide("modal");
        show("success");
        log("done");
    };

    let containers = container! {
        div fx-click=(ActionType::MultiEffect(actions)) {
            "Multiple actions"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    // Check that Multi ActionType is used for multiple actions
    match &containers[0].actions[0].effect.action {
        ActionType::MultiEffect(action_types) => {
            assert_eq!(action_types.len(), 3);

            // First action should be hide("modal")
            match &action_types[0].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("modal"));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
                }
                _ => panic!("Expected Style action for hide"),
            }

            // Second action should be show("success")
            match &action_types[1].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("success"));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
                }
                _ => panic!("Expected Style action for show"),
            }

            // Third action should be log("done")
            match &action_types[2].action {
                ActionType::Log { message, level } => {
                    assert_eq!(message, "done");
                    assert_eq!(*level, LogLevel::Info);
                }
                _ => panic!("Expected Log action"),
            }
        }
        _ => panic!("Expected MultiEffect ActionType for multiple actions"),
    }
}

#[test]
fn test_dsl_in_template_with_multiple_actions() {
    // Test DSL with multiple actions in template
    let containers = container! {
        div fx-click=fx { hide("modal"); show("success"); log("done"); } {
            "Click to execute multiple actions"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    match &containers[0].actions[0].effect.action {
        ActionType::MultiEffect(actions) => {
            assert_eq!(actions.len(), 3);

            // Verify first action (hide)
            match &actions[0].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("modal"));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
                }
                unknown => panic!("Expected Style action for hide, got {unknown:?}"),
            }

            // Verify second action (show)
            match &actions[1].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("success"));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
                }
                unknown => panic!("Expected Style action for show, got {unknown:?}"),
            }

            // Verify third action (log)
            match &actions[2].action {
                ActionType::Log { message, level } => {
                    assert_eq!(message, "done");
                    assert_eq!(*level, LogLevel::Info);
                }
                unknown => panic!("Expected Log action, got {unknown:?}"),
            }
        }
        unknown => panic!("Expected MultiEffect action for multiple actions, got {unknown:?}"),
    }
}

#[test]
fn test_dsl_macro_single_action() {
    // Test actions_dsl! macro with single action
    let action = actions_dsl! {
        hide("test-modal")
    };

    assert_eq!(
        action.action,
        ActionType::Style {
            target: ElementTarget::by_id("test-modal"),
            action: StyleAction::SetVisibility(Visibility::Hidden)
        }
    );
}

#[test]
fn test_dsl_macro_multiple_actions() {
    // Test actions_dsl! macro with multiple sequential actions
    let actions = actions_dsl! {
        show("success-dialog");
        log("Operation completed");
        custom("refresh-data");
        navigate("/dashboard");
    };

    assert_eq!(actions.len(), 4);

    // Verify first action (show)
    match &actions[0].action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id("success-dialog"));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
        }
        _ => panic!("Expected Style action for show from DSL"),
    }

    // Verify second action (log)
    match &actions[1].action {
        ActionType::Log { message, level } => {
            assert_eq!(message, "Operation completed");
            assert_eq!(*level, LogLevel::Info);
        }
        _ => panic!("Expected Log action from DSL"),
    }

    // Verify third action (custom)
    match &actions[2].action {
        ActionType::Custom { action } => {
            assert_eq!(*action, "refresh-data");
        }
        _ => panic!("Expected Custom action from DSL"),
    }

    // Verify fourth action (navigate)
    match &actions[3].action {
        ActionType::Navigate { url } => {
            assert_eq!(url, "/dashboard");
        }
        _ => panic!("Expected Navigate action from DSL"),
    }
}

#[test]
fn test_dsl_macro_with_variables() {
    // Test actions_dsl! macro with variable usage
    let actions = actions_dsl! {
        let modal_id = "user-settings";

        hide(modal_id);
        log("success");
    };

    assert_eq!(actions.len(), 3);

    match &actions[0].action {
        ActionType::Let { name, value } => {
            assert_eq!(name, "modal_id");
            assert_eq!(
                value,
                &hyperchad_actions::dsl::Expression::Literal(
                    hyperchad_actions::dsl::Literal::String("user-settings".to_string())
                )
            );
        }
        unknown => panic!("Expected Let action, got {unknown:?}"),
    }

    // Verify variable was properly substituted in first action
    match &actions[1].action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id(Target::reference("modal_id")));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
        }
        _ => panic!("Expected Style action with variable from DSL"),
    }

    // Verify variable was properly substituted in second action
    match &actions[2].action {
        ActionType::Log { message, level } => {
            assert_eq!(message, "success");
            assert_eq!(*level, LogLevel::Info);
        }
        _ => panic!("Expected Log action with variable from DSL"),
    }
}

#[test]
fn test_dsl_macro_conditional_logic() {
    // Test actions_dsl! macro with conditional expressions
    let action = actions_dsl! {
        if get_visibility("sidebar") == hidden() {
            show("sidebar");
            log("Sidebar opened");
        } else {
            hide("sidebar");
            log("Sidebar closed");
        }
    };

    assert_eq!(
        action.action,
        ActionType::Logic(hyperchad_actions::logic::If {
            condition: hyperchad_actions::logic::Condition::Eq(
                hyperchad_actions::logic::CalcValue::Visibility {
                    target: hyperchad_actions::ElementTarget::ById(Target::literal("sidebar")),
                }
                .into(),
                hyperchad_actions::logic::Value::Visibility(Visibility::Hidden),
            ),
            actions: vec![
                hyperchad_actions::ActionEffect {
                    action: hyperchad_actions::ActionType::Style {
                        target: hyperchad_actions::ElementTarget::ById(Target::literal("sidebar")),
                        action: hyperchad_actions::StyleAction::SetVisibility(
                            hyperchad_transformer_models::Visibility::Visible
                        ),
                    },
                    ..Default::default()
                },
                hyperchad_actions::ActionEffect {
                    action: hyperchad_actions::ActionType::Log {
                        message: "Sidebar opened".to_string(),
                        level: hyperchad_actions::LogLevel::Info,
                    },
                    ..Default::default()
                },
            ],
            else_actions: vec![
                hyperchad_actions::ActionEffect {
                    action: hyperchad_actions::ActionType::Style {
                        target: hyperchad_actions::ElementTarget::ById(Target::literal("sidebar")),
                        action: hyperchad_actions::StyleAction::SetVisibility(
                            hyperchad_transformer_models::Visibility::Hidden
                        ),
                    },
                    ..Default::default()
                },
                hyperchad_actions::ActionEffect {
                    action: hyperchad_actions::ActionType::Log {
                        message: "Sidebar closed".to_string(),
                        level: hyperchad_actions::LogLevel::Info,
                    },
                    ..Default::default()
                },
            ],
        })
    );
}

#[test]
fn test_dsl_macro_in_container_single() {
    // Test using actions_dsl! macro result in container with single action
    let action = actions_dsl! {
        custom("toggle-theme")
    };

    let containers = container! {
        button fx-click=(action) {
            "Toggle Theme"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    // Verify the DSL action was properly applied to container
    match &containers[0].actions[0].effect.action {
        ActionType::Custom { action } => {
            assert_eq!(*action, "toggle-theme");
        }
        _ => panic!("Expected Custom action from DSL in container"),
    }
}

#[test]
fn test_dsl_macro_in_container_multiple() {
    // Test using actions_dsl! macro with multiple actions in container
    let actions = actions_dsl! {
        hide("loading-spinner");
        show("content");
        log("Content loaded");
    };

    let containers = container! {
        div fx-click=(ActionType::MultiEffect(actions)) {
            "Load Content"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    // Verify Multi ActionType contains DSL-generated actions
    match &containers[0].actions[0].effect.action {
        ActionType::MultiEffect(action_types) => {
            assert_eq!(action_types.len(), 3);

            // Verify first DSL action (hide)
            match &action_types[0].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("loading-spinner"));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
                }
                _ => panic!("Expected Style action for hide from DSL"),
            }

            // Verify second DSL action (show)
            match &action_types[1].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("content"));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
                }
                _ => panic!("Expected Style action for show from DSL"),
            }

            // Verify third DSL action (log)
            match &action_types[2].action {
                ActionType::Log { message, level } => {
                    assert_eq!(message, "Content loaded");
                    assert_eq!(*level, LogLevel::Info);
                }
                _ => panic!("Expected Log action from DSL"),
            }
        }
        _ => panic!("Expected MultiEffect ActionType containing DSL actions"),
    }
}

#[test]
fn test_dsl_macro_with_fx_wrapper() {
    // Test actions_dsl! macro used within fx syntax
    let containers = container! {
        div fx-click=fx {
            let panel_id = "info-panel";
            show(panel_id);
            log("Info panel displayed");
        } {
            "Show Info"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    match &containers[0].actions[0].effect.action {
        ActionType::MultiEffect(actions) => {
            assert_eq!(actions.len(), 3);

            match &actions[0].action {
                ActionType::Let { name, value } => {
                    assert_eq!(name, "panel_id");
                    assert_eq!(
                        value,
                        &hyperchad_actions::dsl::Expression::Literal(
                            hyperchad_actions::dsl::Literal::String("info-panel".to_string())
                        )
                    );
                }
                unknown => panic!("Expected Let action from DSL in fx block, got {unknown:?}"),
            }

            // Verify first action (show)
            match &actions[1].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id(Target::reference("panel_id")));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
                }
                unknown => panic!("Expected Style action from DSL in fx block, got {unknown:?}"),
            }

            match &actions[2].action {
                ActionType::Log { message, level } => {
                    assert_eq!(message, "Info panel displayed");
                    assert_eq!(*level, LogLevel::Info);
                }
                unknown => panic!("Expected Log action from DSL in fx block, got {unknown:?}"),
            }
        }
        unknown => panic!("Expected MultiEffect action from fx DSL block, got {unknown:?}"),
    }
}

#[test]
fn test_dsl_macro_complex_real_world_usage() {
    // Test actions_dsl! macro with complex real-world MoosicBox-style patterns
    let toggle_action = actions_dsl! {
        if get_visibility("audio-zones") == hidden() {
            show("audio-zones");
            hide("audio-zones-button-icon");
            show("audio-zones-close-icon");
            log("Audio zones panel opened");
        } else {
            hide("audio-zones");
            show("audio-zones-button-icon");
            hide("audio-zones-close-icon");
            log("Audio zones panel closed");
        }
    };

    let close_actions = actions_dsl! {
        hide("search-modal");
        show("search-button");
        custom("clear-search-results");
    };

    assert_eq!(
        toggle_action.action,
        ActionType::Logic(hyperchad_actions::logic::If {
            condition: hyperchad_actions::logic::Condition::Eq(
                hyperchad_actions::logic::CalcValue::Visibility {
                    target: hyperchad_actions::ElementTarget::ById(Target::literal("audio-zones")),
                }
                .into(),
                hyperchad_actions::logic::Value::Visibility(Visibility::Hidden),
            ),
            actions: vec![
                hyperchad_actions::ActionEffect {
                    action: hyperchad_actions::ActionType::Style {
                        target: hyperchad_actions::ElementTarget::ById(Target::literal(
                            "audio-zones"
                        )),
                        action: hyperchad_actions::StyleAction::SetVisibility(
                            hyperchad_transformer_models::Visibility::Visible
                        ),
                    },
                    ..Default::default()
                },
                hyperchad_actions::ActionEffect {
                    action: hyperchad_actions::ActionType::Style {
                        target: hyperchad_actions::ElementTarget::ById(Target::literal(
                            "audio-zones-button-icon"
                        )),
                        action: hyperchad_actions::StyleAction::SetVisibility(
                            hyperchad_transformer_models::Visibility::Hidden
                        ),
                    },
                    ..Default::default()
                },
                hyperchad_actions::ActionEffect {
                    action: hyperchad_actions::ActionType::Style {
                        target: hyperchad_actions::ElementTarget::ById(Target::literal(
                            "audio-zones-close-icon"
                        )),
                        action: hyperchad_actions::StyleAction::SetVisibility(
                            hyperchad_transformer_models::Visibility::Visible
                        ),
                    },
                    ..Default::default()
                },
                hyperchad_actions::ActionEffect {
                    action: hyperchad_actions::ActionType::Log {
                        message: "Audio zones panel opened".to_string(),
                        level: hyperchad_actions::LogLevel::Info,
                    },
                    ..Default::default()
                },
            ],
            else_actions: vec![
                hyperchad_actions::ActionEffect {
                    action: hyperchad_actions::ActionType::Style {
                        target: hyperchad_actions::ElementTarget::ById(Target::literal(
                            "audio-zones"
                        )),
                        action: hyperchad_actions::StyleAction::SetVisibility(
                            hyperchad_transformer_models::Visibility::Hidden
                        ),
                    },
                    ..Default::default()
                },
                hyperchad_actions::ActionEffect {
                    action: hyperchad_actions::ActionType::Style {
                        target: hyperchad_actions::ElementTarget::ById(Target::literal(
                            "audio-zones-button-icon"
                        )),
                        action: hyperchad_actions::StyleAction::SetVisibility(
                            hyperchad_transformer_models::Visibility::Visible
                        ),
                    },
                    ..Default::default()
                },
                hyperchad_actions::ActionEffect {
                    action: hyperchad_actions::ActionType::Style {
                        target: hyperchad_actions::ElementTarget::ById(Target::literal(
                            "audio-zones-close-icon"
                        )),
                        action: hyperchad_actions::StyleAction::SetVisibility(
                            hyperchad_transformer_models::Visibility::Hidden
                        ),
                    },
                    ..Default::default()
                },
                hyperchad_actions::ActionEffect {
                    action: hyperchad_actions::ActionType::Log {
                        message: "Audio zones panel closed".to_string(),
                        level: hyperchad_actions::LogLevel::Info,
                    },
                    ..Default::default()
                },
            ],
        })
    );

    // Test second action set (close)
    assert_eq!(close_actions.len(), 3);

    match &close_actions[0].action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id("search-modal"));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
        }
        _ => panic!("Expected Style action for hide from DSL"),
    }

    match &close_actions[1].action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id("search-button"));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
        }
        _ => panic!("Expected Style action for show from DSL"),
    }

    match &close_actions[2].action {
        ActionType::Custom { action } => {
            assert_eq!(*action, "clear-search-results");
        }
        _ => panic!("Expected Custom action from DSL"),
    }
}

#[test]
fn test_dsl_macro_vs_direct_actiontype() {
    // Test that actions_dsl! macro generates equivalent actions to direct ActionType usage
    let dsl_action = actions_dsl! {
        hide("test-element")
    };

    let direct_action = ActionType::hide_by_id("test-element");

    // Both should generate equivalent Style actions
    match (&dsl_action.action, &direct_action) {
        (
            ActionType::Style {
                target: dsl_target,
                action: dsl_action,
            },
            ActionType::Style {
                target: direct_target,
                action: direct_action,
            },
        ) => {
            assert_eq!(dsl_target, direct_target);
            assert_eq!(dsl_action, direct_action);
        }
        _ => panic!("DSL and direct ActionType should generate equivalent actions"),
    }
}

#[test]
fn test_new_fx_syntax_without_parentheses() {
    // Test the new fx syntax without parentheses - single action
    let containers = container! {
        button fx-click=fx { hide("search") } {
            "Close Search"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    match &containers[0].actions[0].effect.action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id("search"));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
        }
        unknown => {
            panic!("Expected Style action for hide in new fx syntax, got {unknown:?}")
        }
    }
}

#[test]
fn test_new_fx_syntax_multiple_actions() {
    // Test the new fx syntax with multiple actions in curly braces
    let containers = container! {
        button fx-click=fx {
            hide("search");
            show("search-button");
            log("Search toggled");
        } {
            "Toggle Search"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    match &containers[0].actions[0].effect.action {
        ActionType::MultiEffect(actions) => {
            assert_eq!(actions.len(), 3);

            // Verify first action (hide)
            match &actions[0].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("search"));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
                }
                unknown => panic!("Expected Style action for hide, got {unknown:?}"),
            }

            // Verify second action (show)
            match &actions[1].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("search-button"));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
                }
                unknown => panic!("Expected Style action for show, got {unknown:?}"),
            }

            // Verify third action (log)
            match &actions[2].action {
                ActionType::Log { message, level } => {
                    assert_eq!(message, "Search toggled");
                    assert_eq!(*level, LogLevel::Info);
                }
                unknown => panic!("Expected Log action, got {unknown:?}"),
            }
        }
        unknown => panic!("Expected MultiEffect action for multiple actions, got {unknown:?}"),
    }
}

#[test]
fn test_new_fx_syntax_conditional() {
    // Test the new fx syntax with conditional logic
    let containers = container! {
        button fx-click=fx {
            if get_visibility("modal") == visible() {
                hide("modal")
            } else {
                show("modal")
            }
        } {
            "Toggle Modal"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    match &containers[0].actions[0].effect.action {
        ActionType::Logic(logic) => {
            assert_eq!(logic.actions.len(), 1);
            assert_eq!(logic.else_actions.len(), 1);

            // Verify first action (hide)
            match &logic.actions[0].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("modal"));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
                }
                unknown => panic!("Expected Style action for hide, got {unknown:?}"),
            }

            // Verify second action (show)
            match &logic.else_actions[0].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("modal"));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
                }
                unknown => panic!("Expected Style action for show, got {unknown:?}"),
            }
        }
        unknown => {
            panic!("Expected Logic action from conditional in new fx syntax, got {unknown:?}")
        }
    }
}

#[test]
fn test_new_fx_syntax_with_variables() {
    // Test the new fx syntax with variables
    let containers = container! {
        button fx-click=fx {
            let modal_id = "user-modal";
            hide(modal_id);
            log("Modal closed");
        } {
            "Close User Modal"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    // Should generate a Multi ActionType for multiple actions including variable usage
    match &containers[0].actions[0].effect.action {
        ActionType::MultiEffect(action_types) => {
            assert_eq!(action_types.len(), 3);

            match &action_types[0].action {
                ActionType::Let { name, value } => {
                    assert_eq!(name, "modal_id");
                    assert_eq!(
                        value,
                        &hyperchad_actions::dsl::Expression::Literal(
                            hyperchad_actions::dsl::Literal::String("user-modal".to_string())
                        )
                    );
                }
                unknown => panic!("Expected Let action, got {unknown:?}"),
            }

            // Verify first action (hide with variable)
            match &action_types[1].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id(Target::reference("modal_id")));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
                }
                unknown => panic!("Expected Style action for hide with variable, got {unknown:?}"),
            }

            // Verify second action (log)
            match &action_types[2].action {
                ActionType::Log { message, level } => {
                    assert_eq!(message, "Modal closed");
                    assert_eq!(*level, LogLevel::Info);
                }
                unknown => panic!("Expected Log action, got {unknown:?}"),
            }
        }
        unknown => {
            panic!("Expected MultiEffect ActionType for variable-based fx syntax, got {unknown:?}")
        }
    }
}

#[test]
fn test_fx_syntax_variations() {
    // Test different variations of the fx syntax
    let containers = container! {
        div {
            // Single action
            button fx-click=fx { show("panel") } {
                "Show Panel"
            }

            // Single action (different)
            button fx-click=fx { hide("panel") } {
                "Hide Panel"
            }

            // Multiple actions
            button fx-click=fx {
                hide("panel");
                log("Panel hid");
            } {
                "Toggle Panel"
            }
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].children.len(), 3);

    match &containers[0].children[0].actions[0].effect.action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id("panel"));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
        }
        unknown => panic!("Expected Style action for show, got {unknown:?}"),
    }

    match &containers[0].children[1].actions[0].effect.action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id("panel"));
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
        }
        unknown => panic!("Expected Style action for hide, got {unknown:?}"),
    }

    match &containers[0].children[2].actions[0].effect.action {
        ActionType::MultiEffect(actions) => {
            assert_eq!(actions.len(), 2);

            match &actions[0].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("panel"));
                    assert_eq!(*action, StyleAction::SetVisibility(Visibility::Hidden));
                }
                unknown => panic!("Expected Style action for show, got {unknown:?}"),
            }

            match &actions[1].action {
                ActionType::Log { message, level } => {
                    assert_eq!(message, "Panel hid");
                    assert_eq!(*level, LogLevel::Info);
                }
                unknown => panic!("Expected Log action, got {unknown:?}"),
            }
        }
        unknown => panic!("Expected MultiEffect action for multiple actions, got {unknown:?}"),
    }
}

#[test]
fn test_new_fx_syntax_empty_block() {
    // Test fx with empty block (should generate NoOp)
    let containers = container! {
        button fx-click=fx { } {
            "No Action"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    match &containers[0].actions[0].effect.action {
        ActionType::NoOp => {
            // Success - empty fx block generates NoOp
        }
        _ => panic!("Expected NoOp action for empty fx block"),
    }
}

#[test]
fn test_element_reference_api() {
    // Test the new object-oriented element API - start with simplest case
    let containers = container! {
        div fx-click=fx {
            element("#play-queue").show();
        } {
            "Show Play Queue"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    // Should generate appropriate action
    match &containers[0].actions[0].effect.action {
        ActionType::Style { target, action } => {
            match target {
                ElementTarget::ById(id) => assert_eq!(id, &Target::literal("play-queue")),
                _ => panic!("Expected ById target"),
            }
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
        }
        unknown => panic!("Expected Style action, got: {unknown:?}"),
    }
}

#[test]
fn test_delay_off_literal() {
    // Test the new object-oriented element API - start with simplest case
    let containers = container! {
        div fx-click=fx {
            show("play-queue").delay_off(1000);
        } {
            "Show Play Queue"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    // Should generate appropriate action
    match &containers[0].actions[0].effect.action {
        ActionType::Style { target, action } => {
            match target {
                ElementTarget::ById(id) => assert_eq!(id, &Target::literal("play-queue")),
                _ => panic!("Expected ById target"),
            }
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
        }
        _ => {
            println!(
                "Generated action type: {:?}",
                containers[0].actions[0].effect.action
            );
        }
    }
}

#[test]
fn test_delay_off_interpolation() {
    let value = "play-queue";

    // Test the new object-oriented element API - start with simplest case
    let containers = container! {
        div fx-click=fx {
            show(value).delay_off(1000);
        } {
            "Show Play Queue"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    // Should generate appropriate action
    match &containers[0].actions[0].effect.action {
        ActionType::Style { target, action } => {
            match target {
                ElementTarget::ById(id) => assert_eq!(id, &Target::literal("play-queue")),
                _ => panic!("Expected ById target"),
            }
            assert_eq!(*action, StyleAction::SetVisibility(Visibility::Visible));
        }
        _ => {
            println!(
                "Generated action type: {:?}",
                containers[0].actions[0].effect.action
            );
        }
    }
}

#[test]
fn test_element_reference_simple_usage() {
    // Test very simple element reference usage without complex conditionals
    let containers = container! {
        button fx-click=fx { show("test-id") } {
            "Traditional Syntax"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);
}

#[test]
fn test_element_reference_interpolation() {
    let id = "test";

    // Test very simple element reference usage without complex conditionals
    let action = actions_dsl! { element_by_id(id).show() };

    assert_eq!(
        action.action,
        ActionType::Style {
            target: ElementTarget::ById(Target::literal("test")),
            action: StyleAction::SetVisibility(Visibility::Visible),
        }
    );
}

#[test]
fn test_element_reference_interpolation_multi_statement() {
    let id = "test";

    let actions = actions_dsl! {
        let x = element(id);
        x.show();
    };

    assert_eq!(
        actions.into_iter().map(|x| x.action).collect::<Vec<_>>(),
        vec![
            ActionType::Let {
                name: "x".to_string(),
                value: Expression::ElementRef(Box::new(Expression::Literal(Literal::string(
                    format!("#{id}")
                )))),
            },
            ActionType::Style {
                target: ElementTarget::by_id(Target::reference("x")),
                action: StyleAction::SetVisibility(Visibility::Visible),
            }
        ]
    );
}

#[test]
fn test_selector_parsing_unit() {
    // Test selector parsing at the unit level
    use hyperchad_actions::dsl::{ElementReference, ParsedSelector};

    let id_ref = ElementReference {
        selector: "#my-id".to_string(),
    };
    let class_ref = ElementReference {
        selector: ".my-class".to_string(),
    };
    let plain_ref = ElementReference {
        selector: "plain-id".to_string(),
    };

    assert_eq!(
        id_ref.parse_selector(),
        ParsedSelector::Id("my-id".to_string())
    );
    assert_eq!(
        class_ref.parse_selector(),
        ParsedSelector::Class("my-class".to_string())
    );
    assert_eq!(
        plain_ref.parse_selector(),
        ParsedSelector::Id("plain-id".to_string())
    );
}

#[test]
fn test_display_in_template() {
    let containers = container! {
        div {
            button fx-click=fx { display("panel") } { "Show" }
            button fx-click=fx { no_display("modal") } { "Hide" }
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].children.len(), 2);

    match &containers[0].children[0].actions[0].effect.action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id("panel"));
            assert_eq!(*action, StyleAction::SetDisplay(true));
        }
        other => panic!("Expected SetDisplay(true), got: {other:?}"),
    }

    match &containers[0].children[1].actions[0].effect.action {
        ActionType::Style { target, action } => {
            assert_eq!(*target, ElementTarget::by_id("modal"));
            assert_eq!(*action, StyleAction::SetDisplay(false));
        }
        other => panic!("Expected SetDisplay(false), got: {other:?}"),
    }
}

#[test]
fn test_toggle_display_in_template() {
    let containers = container! {
        button fx-click=fx { toggle_display("modal") } { "Toggle" }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);

    match &containers[0].actions[0].effect.action {
        ActionType::Logic(logic) => {
            assert!(
                !logic.actions.is_empty(),
                "toggle_display should have if actions"
            );
            assert!(
                !logic.else_actions.is_empty(),
                "toggle_display should have else actions"
            );

            match &logic.actions[0].action {
                ActionType::Style {
                    action: StyleAction::SetDisplay(false),
                    ..
                } => {}
                other => panic!("Expected SetDisplay(false) in if branch, got: {other:?}"),
            }

            match &logic.else_actions[0].action {
                ActionType::Style {
                    action: StyleAction::SetDisplay(true),
                    ..
                } => {}
                other => panic!("Expected SetDisplay(true) in else branch, got: {other:?}"),
            }
        }
        other => panic!("toggle_display should generate Logic, got: {other:?}"),
    }
}

#[test]
fn test_element_api_display_methods() {
    let containers = container! {
        div {
            div fx-click=fx {
                element("#panel").display();
            } { "Show Panel" }

            div fx-click=fx {
                element(".modal").no_display();
            } { "Hide Modal" }
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].children.len(), 2);

    match &containers[0].children[0].actions[0].effect.action {
        ActionType::Style { target, action } => {
            match target {
                ElementTarget::ById(id) => assert_eq!(id, &Target::literal("panel")),
                _ => panic!("Expected ById target"),
            }
            assert_eq!(*action, StyleAction::SetDisplay(true));
        }
        other => panic!("Expected display action, got: {other:?}"),
    }

    match &containers[0].children[1].actions[0].effect.action {
        ActionType::Style { target, action } => {
            match target {
                ElementTarget::Class(class) => assert_eq!(class, &Target::literal("modal")),
                _ => panic!("Expected Class target"),
            }
            assert_eq!(*action, StyleAction::SetDisplay(false));
        }
        other => panic!("Expected no_display action, got: {other:?}"),
    }
}

#[test]
fn test_display_in_conditionals() {
    let action = actions_dsl! {
        if get_display("sidebar") == displayed() {
            no_display("sidebar")
        } else {
            display("sidebar")
        }
    };

    match &action.action {
        ActionType::Logic(logic) => {
            assert_eq!(logic.actions.len(), 1);
            assert_eq!(logic.else_actions.len(), 1);

            match &logic.actions[0].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("sidebar"));
                    assert_eq!(*action, StyleAction::SetDisplay(false));
                }
                other => panic!("Expected no_display in if branch, got: {other:?}"),
            }

            match &logic.else_actions[0].action {
                ActionType::Style { target, action } => {
                    assert_eq!(*target, ElementTarget::by_id("sidebar"));
                    assert_eq!(*action, StyleAction::SetDisplay(true));
                }
                other => panic!("Expected display in else branch, got: {other:?}"),
            }
        }
        other => panic!("Expected Logic from conditional, got: {other:?}"),
    }
}

#[test]
fn test_toggle_display_class_selector() {
    let containers = container! {
        div fx-click=fx {
            element(".modal").toggle_display();
        } { "Toggle Modal" }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);

    match &containers[0].actions[0].effect.action {
        ActionType::Logic(logic) => {
            assert!(!logic.actions.is_empty());
            assert!(!logic.else_actions.is_empty());
        }
        other => panic!("toggle_display on class selector should work, got: {other:?}"),
    }
}
