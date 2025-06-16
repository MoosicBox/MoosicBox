use hyperchad_template2::{
    actions::{ActionTrigger, ActionType, ElementTarget, StyleAction},
    container,
};
use hyperchad_transformer_models::Visibility;

#[test]
fn test_fx_click_with_action_type() {
    let containers = container! {
        Div fx-click=(ActionType::hide_str_id("test")) {
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
    let action_effect = ActionType::show_str_id("test").throttle(100);
    let containers = container! {
        Div fx-click=(action_effect) {
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
fn test_fx_click_with_logic_if() {
    use hyperchad_template2::actions::logic::{eq, if_stmt, visible};

    let if_action = if_stmt(eq(visible(), visible()), ActionType::hide_str_id("test"));

    let containers = container! {
        Div fx-click=(if_action) {
            "Hello"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    match &containers[0].actions[0].action.action {
        ActionType::Logic(if_stmt) => {
            assert_eq!(if_stmt.actions.len(), 1);
        }
        _ => panic!("Expected Logic action"),
    }
}

#[test]
fn test_fx_click_outside() {
    let containers = container! {
        Div fx-click-outside=(ActionType::hide_str_id("modal")) {
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
    let containers = container! {
        Div fx-resize=(ActionType::Custom { action: "refresh".to_string() }) {
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
    let containers = container! {
        Div fx-scroll=(ActionType::NoOp) {
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
    let containers = container! {
        Div
            fx-click=(ActionType::show_str_id("panel"))
            fx-hover=(ActionType::hide_str_id("tooltip"))
            fx-resize=(ActionType::NoOp)
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
fn test_fx_action_with_complex_expression() {
    use hyperchad_template2::actions::logic::{get_visibility_str_id, visible};

    let id = "test-element";
    let containers = container! {
        Div fx-click=(
            get_visibility_str_id(id)
                .eq(visible())
                .then(ActionType::hide_str_id(id))
        ) {
            "Toggle visibility"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(containers[0].actions[0].trigger, ActionTrigger::Click);

    match &containers[0].actions[0].action.action {
        ActionType::Logic(_) => {
            // Complex logic expressions become Logic actions
        }
        _ => panic!("Expected Logic action for complex expression"),
    }
}
