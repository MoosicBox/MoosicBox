use super::*;
use hyperchad_actions::{Action, ActionEffect, ActionTrigger};
use hyperchad_renderer::{ReplaceContainer, ResponseCookie, ResponseMetadata};

fn config() -> BmuxConfig {
    BmuxConfig::default()
}
fn view(root: Container) -> View {
    View {
        primary: Some(root),
        ..View::default()
    }
}
fn with_action(action: ActionType) -> Container {
    Container {
        actions: vec![Action {
            trigger: ActionTrigger::Click,
            effect: ActionEffect {
                action,
                ..ActionEffect::default()
            },
        }],
        ..Container::default()
    }
}

#[test]
fn default_tree_and_empty_response_are_admitted() {
    validate_tree(&Container::default(), &config()).unwrap();
    validate_view(&View::default(), &config()).unwrap();
}

#[test]
fn large_password_is_rejected_without_echoing_secret() {
    let root = Container {
        element: Element::Input {
            input: Input::Password {
                value: Some("secret".repeat(100)),
                placeholder: None,
            },
            name: None,
            autofocus: None,
        },
        ..Container::default()
    };
    let mut config = config();
    config.max_text_bytes = 100;
    let error = validate_tree(&root, &config).unwrap_err().to_string();
    assert!(!error.contains("secret"));
    assert!(error.contains("byte"));
}

#[test]
fn textarea_and_hidden_values_are_counted() {
    let mut config = config();
    config.max_text_bytes = 32;
    for element in [
        Element::Textarea {
            value: "x".repeat(64),
            placeholder: None,
            name: None,
            rows: None,
            cols: None,
        },
        Element::Input {
            input: Input::Hidden {
                value: Some("x".repeat(64)),
            },
            name: None,
            autofocus: None,
        },
    ] {
        assert!(
            validate_tree(
                &Container {
                    element,
                    ..Container::default()
                },
                &config
            )
            .is_err()
        );
    }
}

#[test]
fn deep_actions_reject_before_clone() {
    let mut action = ActionType::NoOp;
    for _ in 0..200 {
        action = ActionType::Event {
            name: String::new(),
            action: Box::new(action),
        };
    }
    let root = with_action(action);
    let mut config = config();
    config.max_depth = 16;
    assert!(
        validate_tree(&root, &config)
            .unwrap_err()
            .to_string()
            .contains("nesting")
    );
}

#[test]
fn wide_actions_are_bounded_before_collecting_work() {
    let root = with_action(ActionType::Multi(
        (0..1000).map(|_| ActionType::NoOp).collect(),
    ));
    let mut config = config();
    config.max_text_bytes = 100;
    assert!(validate_tree(&root, &config).is_err());
}

#[test]
fn nested_dsl_and_match_patterns_are_bounded() {
    let mut expression = dsl::Expression::Literal(dsl::Literal::Unit);
    for _ in 0..80 {
        expression = dsl::Expression::Grouping(Box::new(expression));
    }
    let root = with_action(ActionType::Let {
        name: "x".into(),
        value: expression,
    });
    let mut config = config();
    config.max_depth = 16;
    assert!(validate_tree(&root, &config).is_err());
    let mut pattern = dsl::Pattern::Wildcard;
    for _ in 0..50 {
        pattern = dsl::Pattern::Variant {
            enum_name: "E".into(),
            variant: "V".into(),
            fields: vec![pattern],
        };
    }
    let root = with_action(ActionType::Let {
        name: "x".into(),
        value: dsl::Expression::Match {
            expr: Box::new(dsl::Expression::Literal(dsl::Literal::Unit)),
            arms: vec![dsl::MatchArm {
                pattern,
                body: dsl::Expression::Literal(dsl::Literal::Unit),
            }],
        },
    });
    assert!(validate_tree(&root, &config).is_err());
}

#[test]
fn nested_numbers_in_style_and_overrides_are_bounded() {
    let mut calc = Calculation::Number(Box::new(Number::Integer(1)));
    for _ in 0..80 {
        calc = Calculation::Grouping(Box::new(calc));
    }
    let root = Container {
        width: Some(Number::Calc(calc)),
        ..Container::default()
    };
    let mut config = config();
    config.max_depth = 16;
    assert!(validate_tree(&root, &config).is_err());
    let root = Container {
        overrides: vec![hyperchad_transformer::ConfigOverride {
            condition: OverrideCondition::ResponsiveTarget { name: "n".into() },
            overrides: vec![OverrideItem::Width(root.width.unwrap())],
            default: None,
        }],
        ..Container::default()
    };
    assert!(validate_tree(&root, &config).is_err());
}

#[test]
fn nested_logic_arithmetic_and_json_state_are_bounded() {
    let mut value = logic::Value::Real(1.0);
    for _ in 0..70 {
        value = logic::Value::Arithmetic(Box::new(logic::Arithmetic::Plus(
            value,
            logic::Value::Real(1.0),
        )));
    }
    let root = with_action(ActionType::Parameterized {
        action: Box::new(ActionType::NoOp),
        value,
    });
    let mut config = config();
    config.max_depth = 16;
    assert!(validate_tree(&root, &config).is_err());
    let mut state = serde_json::Value::Null;
    for _ in 0..70 {
        state = serde_json::Value::Array(vec![state]);
    }
    assert!(
        validate_tree(
            &Container {
                state: Some(state),
                ..Container::default()
            },
            &config
        )
        .is_err()
    );
}

#[test]
fn response_cookie_navigation_and_selectors_share_budget() {
    let mut config = config();
    config.max_text_bytes = 32;
    let response = ResponseMetadata {
        cookies: vec![ResponseCookie::secure("session", "x".repeat(64))],
        ..ResponseMetadata::default()
    };
    assert!(
        validate_view(
            &View {
                response,
                ..View::default()
            },
            &config
        )
        .is_err()
    );
    let response = ResponseMetadata {
        navigation: Some(ResponseNavigation::Internal(format!("/{}", "x".repeat(64)))),
        ..ResponseMetadata::default()
    };
    assert!(
        validate_view(
            &View {
                response,
                ..View::default()
            },
            &config
        )
        .is_err()
    );
    assert!(
        validate_view(
            &View {
                delete_selectors: vec![Selector::Id("x".repeat(64))],
                ..View::default()
            },
            &config
        )
        .is_err()
    );
}

#[test]
fn fragments_share_node_and_byte_budget() {
    let mut config = config();
    config.max_nodes = 2;
    let response = View {
        primary: Some(Container::default()),
        fragments: (0..2)
            .map(|_| ReplaceContainer {
                selector: Selector::SelfTarget,
                container: Container::default(),
            })
            .collect(),
        ..View::default()
    };
    assert!(validate_view(&response, &config).is_err());
}

#[test]
fn canonical_validator_rejects_accumulated_insertions() {
    let mut config = config();
    config.max_nodes = 3;
    let mut root = Container::default();
    for _ in 0..2 {
        let incoming = Container::default();
        validate_view(&view(incoming.clone()), &config).unwrap();
        root.children.push(incoming);
        validate_tree(&root, &config).unwrap();
    }
    root.children.push(Container::default());
    assert!(validate_tree(&root, &config).is_err());
}

#[test]
fn root_depth_and_node_boundaries_are_explicit() {
    let mut config = config();
    config.max_depth = 0;
    validate_tree(&Container::default(), &config).unwrap();
    let root = Container {
        children: vec![Container::default()],
        ..Container::default()
    };
    assert!(validate_tree(&root, &config).is_err());
    config.max_nodes = 0;
    assert!(validate_tree(&Container::default(), &config).is_err());
    validate_view(&View::default(), &config).unwrap();
}

#[test]
fn style_data_ids_fonts_and_image_payloads_count() {
    let mut config = config();
    config.max_text_bytes = 32;
    for root in [
        Container {
            str_id: Some("x".repeat(64)),
            ..Container::default()
        },
        Container {
            classes: vec!["x".repeat(64)],
            ..Container::default()
        },
        Container {
            font_family: Some(vec!["x".repeat(64)]),
            ..Container::default()
        },
        Container {
            data: [("key".into(), "x".repeat(64))].into(),
            ..Container::default()
        },
        with_action(ActionType::Style {
            target: ElementTarget::SelfTarget,
            action: StyleAction::SetBackground(Some("x".repeat(64))),
        }),
    ] {
        assert!(validate_tree(&root, &config).is_err());
    }
}
