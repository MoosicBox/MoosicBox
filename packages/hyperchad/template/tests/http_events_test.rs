use hyperchad_actions::{ActionEffect, ActionType, LogLevel, Target};
use hyperchad_template::{actions::ActionTrigger, container};
use pretty_assertions::assert_eq;

#[test]
fn test_fx_http_before_request() {
    let containers = container! {
        button fx-http-before-request=(ActionType::Log {
            message: "Request starting".to_string(),
            level: LogLevel::Info,
        }) {
            "Submit"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(
        containers[0].actions[0].trigger,
        ActionTrigger::HttpBeforeRequest
    );

    match &containers[0].actions[0].effect.action {
        ActionType::Log { message, level } => {
            assert_eq!(message, "Request starting");
            assert_eq!(level, &LogLevel::Info);
        }
        _ => panic!("Expected Log action for http-before-request"),
    }
}

#[test]
fn test_fx_http_after_request() {
    let containers = container! {
        button fx-http-after-request=(ActionType::Log {
            message: "Request completed".to_string(),
            level: LogLevel::Info,
        }) {
            "Submit"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(
        containers[0].actions[0].trigger,
        ActionTrigger::HttpAfterRequest
    );
}

#[test]
fn test_fx_http_success() {
    let containers = container! {
        button fx-http-success=(ActionType::Log {
            message: "Success!".to_string(),
            level: LogLevel::Info,
        }) {
            "Submit"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(
        containers[0].actions[0].trigger,
        ActionTrigger::HttpRequestSuccess
    );
}

#[test]
fn test_fx_http_error() {
    let containers = container! {
        button fx-http-error=(ActionType::Log {
            message: "Error occurred".to_string(),
            level: LogLevel::Error,
        }) {
            "Submit"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(
        containers[0].actions[0].trigger,
        ActionTrigger::HttpRequestError
    );
}

#[test]
fn test_fx_http_abort() {
    let containers = container! {
        button fx-http-abort=(ActionType::Log {
            message: "Request aborted".to_string(),
            level: LogLevel::Warn,
        }) {
            "Submit"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(
        containers[0].actions[0].trigger,
        ActionTrigger::HttpRequestAbort
    );
}

#[test]
fn test_fx_http_timeout() {
    let containers = container! {
        button fx-http-timeout=(ActionType::Log {
            message: "Request timed out".to_string(),
            level: LogLevel::Warn,
        }) {
            "Submit"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(
        containers[0].actions[0].trigger,
        ActionTrigger::HttpRequestTimeout
    );
}

#[test]
fn test_multiple_http_event_handlers() {
    let containers = container! {
        button
            fx-http-before-request=(ActionType::Log {
                message: "Starting".to_string(),
                level: LogLevel::Info,
            })
            fx-http-success=(ActionType::Log {
                message: "Success".to_string(),
                level: LogLevel::Info,
            })
            fx-http-error=(ActionType::Log {
                message: "Error".to_string(),
                level: LogLevel::Error,
            })
        {
            "Submit"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 3);

    let triggers: Vec<_> = containers[0]
        .actions
        .iter()
        .map(|a| a.trigger.clone())
        .collect();

    assert!(triggers.contains(&ActionTrigger::HttpBeforeRequest));
    assert!(triggers.contains(&ActionTrigger::HttpRequestSuccess));
    assert!(triggers.contains(&ActionTrigger::HttpRequestError));
}

#[test]
fn test_http_event_with_action_effect() {
    let action = ActionEffect {
        action: ActionType::Log {
            message: "Request started".to_string(),
            level: LogLevel::Info,
        },
        delay_off: Some(1000),
        throttle: Some(500),
        unique: Some(true),
    };

    let containers = container! {
        button fx-http-before-request=(action) {
            "Submit"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);

    let action_effect = &containers[0].actions[0].effect;
    assert_eq!(action_effect.delay_off, Some(1000));
    assert_eq!(action_effect.throttle, Some(500));
    assert_eq!(action_effect.unique, Some(true));
}

#[test]
fn test_http_event_html_output() {
    let containers = container! {
        button fx-http-success=(ActionType::Log {
            message: "Success".to_string(),
            level: LogLevel::Info,
        }) {
            "Submit"
        }
    };

    let html = containers[0]
        .display_to_string_default(false, false)
        .unwrap();

    assert!(html.contains("fx-http-success"));
}

#[test]
fn test_http_event_with_hide_action() {
    let containers = container! {
        button fx-http-before-request=(ActionType::hide_by_id(Target::literal("spinner"))) {
            "Submit"
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 1);
    assert_eq!(
        containers[0].actions[0].trigger,
        ActionTrigger::HttpBeforeRequest
    );
}

#[test]
fn test_http_event_nested_elements() {
    let containers = container! {
        div {
            button fx-http-success=(ActionType::Log {
                message: "Button success".to_string(),
                level: LogLevel::Info,
            }) {
                "Submit"
            }
        }
    };

    assert_eq!(containers.len(), 1);
    assert_eq!(containers[0].actions.len(), 0);
    assert_eq!(containers[0].children.len(), 1);
    assert_eq!(containers[0].children[0].actions.len(), 1);
    assert_eq!(
        containers[0].children[0].actions[0].trigger,
        ActionTrigger::HttpRequestSuccess
    );
}
