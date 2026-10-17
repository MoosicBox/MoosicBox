use super::*;
use hyperchad_actions::{
    Action, Target,
    dsl::{Block, Expression, Literal, Statement},
    logic::{Arithmetic, CalcValue, If},
};
use hyperchad_transformer::Input;
use hyperchad_transformer_models::{ElementTarget, Route};

fn node(id: &str) -> Container {
    Container {
        str_id: Some(id.into()),
        ..Container::default()
    }
}
fn action(trigger: ActionTrigger, action: ActionType) -> Action {
    Action {
        trigger,
        effect: ActionEffect {
            action,
            delay_off: None,
            throttle: None,
            unique: None,
        },
    }
}
fn event(root: &Container, path: &[usize], kind: EventKind, time: u64) -> EventContext {
    EventContext::new(Source::at(root, path.to_vec(), 0).unwrap(), kind, time)
}
fn custom(name: &str) -> ActionType {
    ActionType::Custom {
        action: name.into(),
    }
}
fn style(action: StyleAction) -> ActionType {
    ActionType::Style {
        target: ElementTarget::SelfTarget,
        action,
    }
}
fn dispatch(root: &mut Container, kind: EventKind) -> Result<Vec<Effect>, ActionError> {
    let context = event(root, &[], kind, 0);
    ActionEngine::default().dispatch(root, &context, &FormValues::new())
}

#[test]
fn all_represented_triggers_are_distinct() {
    let pairs = vec![
        (ActionTrigger::Click, EventKind::Click),
        (ActionTrigger::ClickOutside, EventKind::ClickOutside),
        (ActionTrigger::MouseDown, EventKind::MouseDown),
        (ActionTrigger::KeyDown, EventKind::KeyDown),
        (ActionTrigger::Hover, EventKind::Hover),
        (ActionTrigger::Change, EventKind::Change),
        (ActionTrigger::Resize, EventKind::Resize),
        (ActionTrigger::Immediate, EventKind::Immediate),
        (
            ActionTrigger::Event("named".into()),
            EventKind::Named("named".into()),
        ),
        (
            ActionTrigger::HttpBeforeRequest,
            EventKind::Http(HttpPhase::Before),
        ),
        (
            ActionTrigger::HttpAfterRequest,
            EventKind::Http(HttpPhase::After),
        ),
        (
            ActionTrigger::HttpRequestSuccess,
            EventKind::Http(HttpPhase::Success),
        ),
        (
            ActionTrigger::HttpRequestError,
            EventKind::Http(HttpPhase::Error),
        ),
        (
            ActionTrigger::HttpRequestAbort,
            EventKind::Http(HttpPhase::Abort),
        ),
        (
            ActionTrigger::HttpRequestTimeout,
            EventKind::Http(HttpPhase::Timeout),
        ),
    ];
    for (i, (trigger, kind)) in pairs.iter().enumerate() {
        for (j, (_, other)) in pairs.iter().enumerate() {
            assert_eq!(other.matches(trigger), i == j);
        }
        let mut root = node("root");
        root.actions.push(action(trigger.clone(), custom("hit")));
        let mut context = event(&root, &[], kind.clone(), 0);
        if matches!(kind, EventKind::Http(_)) {
            context.http = Some(HttpEventContext {
                url: "/request".into(),
                method: "POST".into(),
                status: Some(200),
                headers: None,
                duration_ms: Some(1),
                error: None,
            });
        }
        assert!(matches!(
            ActionEngine::default()
                .dispatch(&mut root, &context, &FormValues::new())
                .unwrap()
                .as_slice(),
            [Effect::Custom { .. }]
        ));
    }
    assert!(!EventKind::Click.matches(&ActionTrigger::Event("click".into())));
}

#[test]
#[allow(clippy::too_many_lines)] // Asserts sequencing across all action variants in one dispatch.
fn exhaustive_action_variants_and_ordered_parameters() {
    let mut root = node("root");
    root.children.push(Container {
        str_id: Some("edit".into()),
        id: 42,
        element: Element::Input {
            input: Input::Text {
                value: None,
                placeholder: None,
            },
            name: None,
            autofocus: None,
        },
        ..Container::default()
    });
    root.actions.push(action(
        ActionTrigger::Click,
        ActionType::Multi(vec![
            ActionType::NoOp,
            ActionType::Let {
                name: "target".into(),
                value: Expression::Literal(Literal::String("edit".into())),
            },
            ActionType::Input(InputActionType::Select {
                target: ElementTarget::ById(Target::Ref("target".into())),
            }),
            style(StyleAction::SetDisplay(false)),
            style(StyleAction::SetVisibility(Visibility::Hidden)),
            style(StyleAction::SetBackground(Some("#112233".into()))),
            style(StyleAction::SetFocus(true)),
            ActionType::Navigate {
                url: "/next".into(),
            },
            ActionType::Log {
                message: "hello".into(),
                level: LogLevel::Info,
            },
            ActionType::Event {
                name: "later".into(),
                action: Box::new(custom("registered")),
            },
            ActionType::MultiEffect(vec![ActionEffect {
                action: custom("effect"),
                throttle: Some(5),
                delay_off: None,
                unique: None,
            }]),
            ActionType::Parameterized {
                action: Box::new(custom("param")),
                value: Value::Arithmetic(Box::new(Arithmetic::Plus(
                    Value::Real(2.0),
                    Value::Real(3.0),
                ))),
            },
            ActionType::Logic(If {
                condition: Condition::Eq(
                    Value::Calc(CalcValue::Display {
                        target: ElementTarget::SelfTarget,
                    }),
                    Value::Display(false),
                ),
                actions: vec![custom("then").into()],
                else_actions: vec![custom("else").into()],
            }),
        ]),
    ));
    let context = event(&root, &[], EventKind::Click, 0);
    let mut engine = ActionEngine::default();
    let effects = engine
        .dispatch(&mut root, &context, &FormValues::new())
        .unwrap();
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Select { source } if source.path == [0]))
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Focus { focused: true, .. }))
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Navigate { url, .. } if url == "/next"))
    );
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::Log { message, .. } if message == "hello"))
    );
    let customs: Vec<_> = effects
        .iter()
        .filter_map(|e| {
            if let Effect::Custom { action, value, .. } = e {
                Some((action.as_str(), value.clone()))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        customs,
        vec![
            ("effect", None),
            ("param", Some(Value::Real(5.0))),
            ("then", None)
        ]
    );
    assert_eq!(root.hidden, Some(true));
    assert_eq!(root.visibility, Some(Visibility::Hidden));
    assert!(root.background.is_some());
    let named = event(&root, &[], EventKind::Named("later".into()), 1);
    assert!(
        matches!(engine.dispatch(&mut root, &named, &FormValues::new()).unwrap().as_slice(), [Effect::Custom { action, .. }] if action == "registered")
    );
}

#[test]
fn failures_are_transactional_and_not_noops() {
    let mut root = node("root");
    root.actions.push(action(
        ActionTrigger::Click,
        ActionType::Multi(vec![
            style(StyleAction::SetDisplay(false)),
            ActionType::Let {
                name: "x".into(),
                value: Expression::RawRust("unsafe_code()".into()),
            },
        ]),
    ));
    assert!(matches!(
        dispatch(&mut root, EventKind::Click),
        Err(ActionError::Unsupported(_))
    ));
    assert_eq!(root.hidden, None);
    root.actions[0].effect.action = ActionType::Input(InputActionType::Select {
        target: ElementTarget::SelfTarget,
    });
    assert!(matches!(
        dispatch(&mut root, EventKind::Click),
        Err(ActionError::Invalid(_))
    ));
    root.actions[0].effect.action = ActionType::Navigate {
        url: "javascript:bad()".into(),
    };
    assert!(matches!(
        dispatch(&mut root, EventKind::Click),
        Err(ActionError::Invalid(_))
    ));
    root.actions[0].effect.action = style(StyleAction::SetBackground(Some("invalid".into())));
    assert!(dispatch(&mut root, EventKind::Click).is_err());
}

#[test]
fn effect_owners_do_not_share_throttle_and_hover_restores_baseline() {
    let mut root = node("root");
    root.hidden = Some(true);
    let mut first = action(ActionTrigger::Hover, style(StyleAction::SetDisplay(true)));
    first.effect.delay_off = Some(10);
    first.effect.throttle = Some(5);
    root.actions = vec![
        first,
        action(
            ActionTrigger::Click,
            style(StyleAction::SetBackground(Some("#abcdef".into()))),
        ),
    ];
    let mut engine = ActionEngine::default();
    let enter = event(&root, &[], EventKind::Hover, 0);
    engine
        .dispatch(&mut root, &enter, &FormValues::new())
        .unwrap();
    assert_eq!(root.hidden, Some(false));
    let click = event(&root, &[], EventKind::Click, 0);
    engine
        .dispatch(&mut root, &click, &FormValues::new())
        .unwrap();
    assert!(root.background.is_some());
    let leave = event(&root, &[], EventKind::Hover, 2);
    engine.release(&mut root, &leave).unwrap();
    assert_eq!(engine.next_deadline(), Some(12));
    engine.advance(&mut root, 11).unwrap();
    assert_eq!(root.hidden, Some(false));
    engine.advance(&mut root, 12).unwrap();
    assert_eq!(root.hidden, Some(true));
    assert!(root.background.is_some());
}

#[test]
fn immediate_unique_and_throttle_have_independent_lifetimes() {
    let mut root = node("root");
    let mut unique = action(ActionTrigger::Click, custom("unique"));
    unique.effect.unique = Some(true);
    let mut throttle = action(ActionTrigger::Click, custom("throttle"));
    throttle.effect.throttle = Some(10);
    root.actions = vec![
        unique,
        throttle,
        action(ActionTrigger::Immediate, custom("initial")),
    ];
    let mut engine = ActionEngine::default();
    for (time, count) in [(0, 2), (1, 0), (10, 1)] {
        let context = event(&root, &[], EventKind::Click, time);
        assert_eq!(
            engine
                .dispatch(&mut root, &context, &FormValues::new())
                .unwrap()
                .len(),
            count
        );
    }
    let context = event(&root, &[], EventKind::Immediate, 10);
    assert_eq!(
        engine
            .dispatch(&mut root, &context, &FormValues::new())
            .unwrap()
            .len(),
        1
    );
    assert!(
        engine
            .dispatch(&mut root, &context, &FormValues::new())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn selectors_target_all_and_distinguish_ids_from_css() {
    let mut root = node("root");
    root.children = vec![node("a"), node("b")];
    for child in &mut root.children {
        child.classes.push("item".into());
    }
    root.actions.push(action(
        ActionTrigger::Click,
        ActionType::Style {
            target: ElementTarget::Selector(Target::Literal(":scope > .item".into())),
            action: StyleAction::SetVisibility(Visibility::Hidden),
        },
    ));
    dispatch(&mut root, EventKind::Click).unwrap();
    assert!(
        root.children
            .iter()
            .all(|c| c.visibility == Some(Visibility::Hidden))
    );
    root.actions[0].effect.action = ActionType::Style {
        target: ElementTarget::Selector(Target::Literal(".item:hover".into())),
        action: StyleAction::SetDisplay(false),
    };
    assert!(matches!(
        dispatch(&mut root, EventKind::Click),
        Err(ActionError::Selector(_))
    ));
}

#[test]
fn stale_sources_and_http_without_context_are_rejected() {
    let mut root = node("root");
    let mut engine = ActionEngine::default();
    let old = event(&root, &[], EventKind::Click, 0);
    engine.reconcile(&root, 1).unwrap();
    assert_eq!(
        engine
            .dispatch(&mut root, &old, &FormValues::new())
            .unwrap_err(),
        ActionError::StaleSource
    );
    let http = EventContext::new(
        Source::at(&root, vec![], 1).unwrap(),
        EventKind::Http(HttpPhase::Error),
        0,
    );
    assert_eq!(
        engine
            .dispatch(&mut root, &http, &FormValues::new())
            .unwrap_err(),
        ActionError::MissingHttpContext
    );
}

#[test]
fn while_loop_and_effect_expansion_are_bounded() {
    let mut root = node("root");
    root.actions.push(action(
        ActionTrigger::Click,
        ActionType::Let {
            name: "forever".into(),
            value: Expression::Block(Block {
                statements: vec![Statement::While {
                    condition: Expression::Literal(Literal::Bool(true)),
                    body: Block { statements: vec![] },
                }],
            }),
        },
    ));
    let context = event(&root, &[], EventKind::Click, 0);
    let limits = ActionLimits {
        max_steps: 32,
        ..ActionLimits::default()
    };
    assert!(matches!(
        ActionEngine::new(0, limits).dispatch(&mut root, &context, &FormValues::new()),
        Err(ActionError::Limit(_))
    ));
    root.actions[0].effect.action = ActionType::Multi(vec![custom("one"), custom("two")]);
    let limits = ActionLimits {
        max_entries: 1,
        ..ActionLimits::default()
    };
    assert!(matches!(
        ActionEngine::new(0, limits).dispatch(&mut root, &context, &FormValues::new()),
        Err(ActionError::Limit(_))
    ));
}

fn form() -> Container {
    let mut root = node("form");
    root.element = Element::Form {
        action: Some("/submit?existing=1#end".into()),
        method: Some("GET".into()),
        multipart: false,
    };
    for (id, name, input) in [
        (
            "text",
            "q",
            Input::Text {
                value: Some("a b&é".into()),
                placeholder: None,
            },
        ),
        (
            "hidden",
            "q",
            Input::Hidden {
                value: Some("token".into()),
            },
        ),
        (
            "check",
            "check",
            Input::Checkbox {
                checked: Some(true),
            },
        ),
        (
            "unchecked",
            "no",
            Input::Checkbox {
                checked: Some(false),
            },
        ),
    ] {
        let mut child = node(id);
        child.element = Element::Input {
            input,
            name: Some(name.into()),
            autofocus: None,
        };
        root.children.push(child);
    }
    root
}
#[test]
fn form_urlencoding_duplicates_hidden_checkbox_and_reset() {
    let mut root = form();
    let effects = dispatch(&mut root, EventKind::Submit).unwrap();
    let [Effect::Request(request)] = effects.as_slice() else {
        panic!("request");
    };
    assert_eq!(request.method, "GET");
    assert_eq!(
        request.url,
        "/submit?existing=1&q=a+b%26%C3%A9&q=token&check=on#end"
    );
    assert_eq!(request.body, RequestBody::Empty);
    assert!(request.navigation);
    let effects = dispatch(&mut root, EventKind::Reset).unwrap();
    let [Effect::Reset { values, .. }] = effects.as_slice() else {
        panic!("reset");
    };
    assert_eq!(values.len(), 4);
}
#[test]
fn multipart_preserves_opaque_capabilities_and_rejects_get_files() {
    let mut root = form();
    root.element = Element::Form {
        action: Some("/upload".into()),
        method: Some("POST".into()),
        multipart: true,
    };
    let mut child = node("file");
    child.element = Element::Input {
        input: Input::File,
        name: Some("upload".into()),
        autofocus: None,
    };
    root.children.push(child);
    let key = crate::layout::node_key(&root.children[4], &[4]);
    let file = AuthorizedFile {
        token: "host-issued-not-a-path".into(),
        name: "cover.png".into(),
        content_type: "image/png".into(),
        size: 10,
    };
    let values = BTreeMap::from([(key, ControlValue::Files(vec![file.clone()]))]);
    let context = event(&root, &[], EventKind::Submit, 0);
    let effects = ActionEngine::default()
        .dispatch(&mut root, &context, &values)
        .unwrap();
    let [Effect::Request(request)] = effects.as_slice() else {
        panic!("request");
    };
    let segments = request
        .body
        .multipart_segments("safe-boundary", 4096)
        .unwrap();
    assert!(
        segments
            .iter()
            .any(|s| matches!(s, MultipartSegment::File(f) if f == &file))
    );
    assert!(request.body.multipart_segments("bad\r\n", 4096).is_err());
    assert!(request.body.multipart_segments("safe", 1).is_err());
    root.element = Element::Form {
        action: Some("/upload".into()),
        method: Some("GET".into()),
        multipart: true,
    };
    assert!(
        ActionEngine::default()
            .dispatch(&mut root, &context, &values)
            .is_err()
    );
}
#[test]
fn routes_preserve_every_swap_and_method() {
    for strategy in [
        SwapStrategy::This,
        SwapStrategy::Children,
        SwapStrategy::BeforeBegin,
        SwapStrategy::AfterBegin,
        SwapStrategy::BeforeEnd,
        SwapStrategy::AfterEnd,
        SwapStrategy::Delete,
        SwapStrategy::None,
    ] {
        for method in ["GET", "POST", "PUT", "DELETE", "PATCH"] {
            let mut root = node("root");
            let target = Selector::Class("dest".into());
            let route = "/route".to_owned();
            let trigger = None;
            root.route = Some(match method {
                "GET" => Route::Get {
                    route,
                    trigger,
                    target,
                    strategy: strategy.clone(),
                },
                "POST" => Route::Post {
                    route,
                    trigger,
                    target,
                    strategy: strategy.clone(),
                },
                "PUT" => Route::Put {
                    route,
                    trigger,
                    target,
                    strategy: strategy.clone(),
                },
                "DELETE" => Route::Delete {
                    route,
                    trigger,
                    target,
                    strategy: strategy.clone(),
                },
                _ => Route::Patch {
                    route,
                    trigger,
                    target,
                    strategy: strategy.clone(),
                },
            });
            let mut context = event(&root, &[], EventKind::Click, 0);
            context.request_id = Some("correlation".into());
            let effects = ActionEngine::default()
                .dispatch(&mut root, &context, &FormValues::new())
                .unwrap();
            let [Effect::Request(request)] = effects.as_slice() else {
                panic!("request");
            };
            assert_eq!(request.method, method);
            assert_eq!(request.strategy, strategy);
            assert_eq!(request.target, Selector::Class("dest".into()));
            assert_eq!(request.request_id.as_deref(), Some("correlation"));
        }
    }
}
#[test]
fn arithmetic_variants_geometry_event_value_and_else() {
    for (arithmetic, expected) in [
        (Arithmetic::Plus(Value::Real(6.0), Value::Real(2.0)), 8.0),
        (Arithmetic::Minus(Value::Real(6.0), Value::Real(2.0)), 4.0),
        (
            Arithmetic::Multiply(Value::Real(6.0), Value::Real(2.0)),
            12.0,
        ),
        (Arithmetic::Divide(Value::Real(6.0), Value::Real(2.0)), 3.0),
        (Arithmetic::Min(Value::Real(6.0), Value::Real(2.0)), 2.0),
        (Arithmetic::Max(Value::Real(6.0), Value::Real(2.0)), 6.0),
        (
            Arithmetic::Grouping(Box::new(Arithmetic::Plus(
                Value::Real(1.0),
                Value::Real(2.0),
            ))),
            3.0,
        ),
    ] {
        let mut root = node("root");
        root.actions.push(action(
            ActionTrigger::Click,
            ActionType::Parameterized {
                action: Box::new(custom("result")),
                value: Value::Arithmetic(Box::new(arithmetic)),
            },
        ));
        assert!(
            matches!(dispatch(&mut root, EventKind::Click).unwrap().as_slice(), [Effect::Custom { value: Some(Value::Real(v)), .. }] if (*v - expected).abs() < f32::EPSILON)
        );
    }
    let mut root = node("root");
    root.actions.push(action(
        ActionTrigger::Change,
        ActionType::Parameterized {
            action: Box::new(custom("value")),
            value: Value::Calc(CalcValue::EventValue),
        },
    ));
    let mut context = event(&root, &[], EventKind::Change, 0);
    context.value = Some("changed".into());
    assert!(
        matches!(ActionEngine::default().dispatch(&mut root, &context, &FormValues::new()).unwrap().as_slice(), [Effect::Custom { value: Some(Value::String(v)), .. }] if v == "changed")
    );
    root.actions = vec![action(
        ActionTrigger::Click,
        ActionType::Logic(If {
            condition: Condition::Bool(false),
            actions: vec![custom("wrong").into()],
            else_actions: vec![custom("else").into()],
        }),
    )];
    assert!(
        matches!(dispatch(&mut root, EventKind::Click).unwrap().as_slice(), [Effect::Custom { action, .. }] if action == "else")
    );
}
