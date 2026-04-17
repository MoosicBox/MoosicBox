use hyperchad_actions::{
    Action, ActionEffect, ActionTrigger, ActionType,
    logic::{CalcValue, Value},
};
use hyperchad_renderer::{
    View,
    transformer::{
        Container, Element,
        models::{Selector, Visibility},
    },
};
use hyperchad_renderer_testing::{
    FormSubmission, Harness, HttpEventKind, HttpEventPayload, client::event::CustomEvent,
    time::ManualClock,
};

fn div(id: usize, str_id: &str) -> Container {
    Container {
        id,
        str_id: Some(str_id.to_string()),
        element: Element::Div,
        ..Default::default()
    }
}

fn raw(id: usize, value: &str) -> Container {
    Container {
        id,
        element: Element::Raw {
            value: value.to_string(),
        },
        ..Default::default()
    }
}

#[test_log::test]
fn accept_full_view_initial_navigation() {
    let mut harness = Harness::with_default_renderer();
    harness.route_full("/", View::builder().with_primary(div(1, "root")).build());

    harness.navigate_to("/").unwrap();
    harness.assert_stream_kinds(&["view"]).unwrap();
    harness.assert_selector_exists("#root").unwrap();
}

#[test_log::test]
fn accept_partial_fragment_update_by_id() {
    let mut harness = Harness::with_default_renderer();

    let mut root = div(1, "root");
    root.children.push(div(2, "target"));

    harness.apply_view(View::builder().with_primary(root).build());
    harness.apply_view(
        View::builder()
            .with_fragment(hyperchad_renderer::ReplaceContainer {
                selector: Selector::Id("target".to_string()),
                container: div(3, "target"),
            })
            .build(),
    );

    harness
        .assert_stream_kinds(&["view", "partial_view"])
        .unwrap();
}

#[test_log::test]
fn accept_delete_selectors_applied() {
    let mut harness = Harness::with_default_renderer();

    let mut root = div(1, "root");
    let mut removable = div(2, "remove-me");
    removable.classes.push("old-element".to_string());
    root.children.push(removable);
    root.children.push(div(3, "keep-me"));

    harness.apply_view(View::builder().with_primary(root).build());
    harness.apply_view(
        View::builder()
            .with_delete_selector(Selector::Class("old-element".to_string()))
            .build(),
    );

    harness.assert_selector_exists("#keep-me").unwrap();
    assert!(harness.assert_selector_exists("#remove-me").is_err());
}

#[test_log::test]
fn accept_hx_request_partial_vs_full() {
    let mut harness = Harness::with_default_renderer();
    harness.route_full_and_partial(
        "/page",
        View::builder().with_primary(div(1, "page-full")).build(),
        View::builder()
            .with_fragment(div(2, "page-partial"))
            .build(),
    );

    harness.navigate_to("/page").unwrap();
    harness.navigate_hx("/page").unwrap();

    harness
        .assert_stream_kinds(&["view", "partial_view"])
        .unwrap();
}

#[test_log::test]
fn accept_custom_event_dispatch_chain() {
    let mut harness = Harness::with_default_renderer();

    let parent_action = Action {
        trigger: ActionTrigger::Event("ping".to_string()),
        effect: ActionEffect {
            action: ActionType::Custom {
                action: "got_ping".to_string(),
            },
            ..Default::default()
        },
    };

    let mut child = div(3, "child");
    child.children.push(raw(4, "payload"));

    let mut parent = div(2, "parent");
    parent.actions.push(parent_action);
    parent.children.push(child);

    let mut root = div(1, "root");
    root.children.push(parent);

    harness.apply_view(View::builder().with_primary(root).build());
    let effects = harness
        .dispatch_custom_event("#child", CustomEvent::new("ping", Some("ok".to_string())))
        .unwrap();

    assert!(
        effects
            .custom_actions
            .iter()
            .any(|(name, _)| name == "got_ping")
    );
}

#[test_log::test]
fn accept_click_triggered_style_action() {
    let mut harness = Harness::with_default_renderer();

    let mut button = div(2, "button");
    button.actions.push(Action {
        trigger: ActionTrigger::Click,
        effect: ActionType::hide_self().into(),
    });

    let mut root = div(1, "root");
    root.children.push(button);

    harness.apply_view(View::builder().with_primary(root).build());
    let _effects = harness.click("#button").unwrap();

    let snapshot = harness.renderer().snapshot();
    let visibility = snapshot
        .dom
        .root()
        .and_then(|root| root.find_element_by_str_id("button"))
        .and_then(|button| button.visibility);
    assert_eq!(visibility, Some(Visibility::Hidden));
}

#[test_log::test]
fn accept_http_lifecycle_triggers_context() {
    let mut harness = Harness::with_default_renderer();

    let mut button = div(2, "button");
    button.actions.push(Action {
        trigger: ActionTrigger::HttpRequestSuccess,
        effect: ActionEffect {
            action: ActionType::Parameterized {
                action: Box::new(ActionType::Custom {
                    action: "http_success".to_string(),
                }),
                value: Value::Calc(CalcValue::EventValue),
            },
            ..Default::default()
        },
    });

    let mut root = div(1, "root");
    root.children.push(button);
    harness.apply_view(View::builder().with_primary(root).build());

    let effects = harness
        .dispatch_http_event(
            "#button",
            HttpEventKind::RequestSuccess,
            &HttpEventPayload {
                url: "/api/test".to_string(),
                method: "GET".to_string(),
                status: Some(200),
                headers: None,
                duration_ms: Some(10),
                error: None,
            },
        )
        .unwrap();

    let value = effects
        .custom_actions
        .iter()
        .find_map(|(name, value)| (name == "http_success").then_some(value))
        .and_then(|value| value.as_ref());

    let value = value.expect("expected custom action value");
    if let Value::String(value) = value {
        assert!(value.contains("\"status\":200"));
    } else {
        panic!("expected Value::String, got {value:?}");
    }
}

#[test_log::test]
fn accept_form_submit_order_hx_then_action() {
    let mut harness = Harness::with_default_renderer();
    harness.route_full_and_partial(
        "/hx",
        View::builder().with_primary(div(10, "hx-full")).build(),
        View::builder().with_fragment(div(11, "hx-partial")).build(),
    );
    harness.route_full(
        "/action",
        View::builder().with_primary(div(20, "action-full")).build(),
    );

    harness
        .submit_form(
            &FormSubmission::new()
                .with_hx_route("/hx")
                .with_action_route("/action"),
        )
        .unwrap();

    harness
        .assert_stream_kinds(&["partial_view", "view"])
        .unwrap();
    assert_eq!(
        harness.navigation_history(),
        &["/hx".to_string(), "/action".to_string()]
    );
}

#[test_log::test]
fn accept_throttle_and_delay_off_deterministic() {
    let mut clock = ManualClock::new();
    clock.schedule(100, "throttle-window-end");
    clock.schedule(250, "delay-off-end");

    assert!(clock.advance(99).is_empty());
    assert_eq!(clock.advance(1), vec!["throttle-window-end".to_string()]);
    assert_eq!(clock.advance(150), vec!["delay-off-end".to_string()]);
}

#[test_log::test]
fn accept_snapshot_stability_dom_and_transcript() {
    let mut harness = Harness::with_default_renderer();

    let mut root = div(1, "root");
    root.children.push(div(2, "child"));
    harness.apply_view(View::builder().with_primary(root).build());
    harness
        .dispatch_custom_event("#child", CustomEvent::new("loaded", None))
        .unwrap();

    insta::assert_snapshot!("dom_snapshot", harness.dom_snapshot());
    insta::assert_snapshot!("stream_snapshot", harness.stream_snapshot());
}
