use super::*;
use hyperchad_transformer::Input;
use hyperchad_transformer_models::Route;

fn root() -> Container {
    Container {
        str_id: Some("form".into()),
        element: Element::Form {
            action: Some("/post".into()),
            method: Some("POST".into()),
            multipart: false,
        },
        ..Container::default()
    }
}
fn field(key: &str, element: Element) -> Container {
    Container {
        str_id: Some(key.into()),
        element,
        ..Container::default()
    }
}
fn submit(root: &mut Container, values: &FormValues) -> Result<Vec<Effect>, ActionError> {
    let event = EventContext::new(Source::at(root, vec![], 0)?, EventKind::Submit, 0);
    ActionEngine::default().dispatch(root, &event, values)
}
fn option(value: &str, disabled: bool) -> Container {
    field(
        value,
        Element::Option {
            value: Some(value.into()),
            disabled: Some(disabled),
        },
    )
}
#[test]
fn textarea_password_select_disabled_and_semantic_overrides() {
    let mut root = root();
    root.children = vec![
        field(
            "textarea",
            Element::Textarea {
                name: Some("body".into()),
                value: "initial".into(),
                placeholder: None,
                rows: None,
                cols: None,
            },
        ),
        field(
            "password",
            Element::Input {
                name: Some("password".into()),
                input: Input::Password {
                    value: Some("secret".into()),
                    placeholder: None,
                },
                autofocus: None,
            },
        ),
        field(
            "select",
            Element::Select {
                name: Some("choice".into()),
                selected: Some("b".into()),
                multiple: Some(true),
                disabled: None,
                autofocus: None,
            },
        ),
        field(
            "disabled",
            Element::Select {
                name: Some("ignored".into()),
                selected: None,
                multiple: None,
                disabled: Some(true),
                autofocus: None,
            },
        ),
    ];
    root.children[2].children = vec![option("a", false), option("b", false), option("c", true)];
    root.children[3].children = vec![option("disabled-option", false)];
    let values = FormValues::from([
        ("textarea".into(), ControlValue::Text("edited\ntext".into())),
        (
            "select".into(),
            ControlValue::Selected(vec!["a".into(), "b".into()]),
        ),
    ]);
    let effects = submit(&mut root, &values).unwrap();
    let [Effect::Request(request)] = effects.as_slice() else {
        panic!("request");
    };
    assert_eq!(
        request.body,
        RequestBody::UrlEncoded("body=edited%0Atext&password=secret&choice=a&choice=b".into())
    );
    assert!(!request.headers.is_empty());
    let invalid = FormValues::from([("select".into(), ControlValue::Selected(vec!["c".into()]))]);
    assert!(matches!(
        submit(&mut root, &invalid),
        Err(ActionError::Invalid(_))
    ));
    let invalid_type = FormValues::from([("textarea".into(), ControlValue::Checked(true))]);
    assert!(matches!(
        submit(&mut root, &invalid_type),
        Err(ActionError::Invalid(_))
    ));
}
#[test]
fn form_route_keeps_form_source_when_submitted_from_child() {
    let mut root = root();
    root.route = Some(Route::Post {
        route: "/route".into(),
        trigger: None,
        target: Selector::SelfTarget,
        strategy: SwapStrategy::Children,
    });
    root.children.push(field(
        "submit",
        Element::Button {
            r#type: Some("submit".into()),
        },
    ));
    let event = EventContext::new(Source::at(&root, vec![0], 0).unwrap(), EventKind::Click, 0);
    let effects = ActionEngine::default()
        .dispatch(&mut root, &event, &FormValues::new())
        .unwrap();
    let [Effect::Request(route), Effect::Request(navigation)] = effects.as_slice() else {
        panic!("ordered requests");
    };
    assert_eq!(route.source.path, Vec::<usize>::new());
    assert_eq!(route.strategy, SwapStrategy::Children);
    assert!(!route.navigation);
    assert!(navigation.navigation);
}
#[test]
fn native_anchor_reset_button_and_prevent_default() {
    let mut root = root();
    root.children.push(field(
        "reset",
        Element::Button {
            r#type: Some("reset".into()),
        },
    ));
    let event = EventContext::new(Source::at(&root, vec![0], 0).unwrap(), EventKind::Click, 0);
    assert!(matches!(
        ActionEngine::default()
            .dispatch(&mut root, &event, &FormValues::new())
            .unwrap()
            .as_slice(),
        [Effect::Reset { .. }]
    ));
    root.children[0].element = Element::Anchor {
        href: Some("https://example.com/path".into()),
        target: Some(LinkTarget::Blank),
    };
    assert!(matches!(
        ActionEngine::default()
            .dispatch(&mut root, &event, &FormValues::new())
            .unwrap()
            .as_slice(),
        [Effect::Navigate {
            target: Some(LinkTarget::Blank),
            ..
        }]
    ));
    let mut event = event;
    event.prevent_default = true;
    assert!(
        ActionEngine::default()
            .dispatch(&mut root, &event, &FormValues::new())
            .unwrap()
            .is_empty()
    );
}
#[test]
fn resource_limits_and_capability_validation_fail_closed() {
    let mut root = root();
    root.children.push(field(
        "large",
        Element::Input {
            input: Input::Text {
                value: Some("oversized".into()),
                placeholder: None,
            },
            name: Some("n".into()),
            autofocus: None,
        },
    ));
    let event = EventContext::new(Source::at(&root, vec![], 0).unwrap(), EventKind::Submit, 0);
    let limits = ActionLimits {
        max_request_bytes: 2,
        ..ActionLimits::default()
    };
    assert!(matches!(
        ActionEngine::new(0, limits).dispatch(&mut root, &event, &FormValues::new()),
        Err(ActionError::Limit(_))
    ));
    let invalid = AuthorizedFile {
        token: String::new(),
        name: "file".into(),
        content_type: "text/plain".into(),
        size: 1,
    };
    assert!(
        submit(
            &mut root,
            &FormValues::from([("file".into(), ControlValue::Files(vec![invalid]))])
        )
        .is_err()
    );
    let body = RequestBody::Multipart(vec![FormPart::File {
        name: "field\r\nname".into(),
        file: AuthorizedFile {
            token: "opaque".into(),
            name: "a\"b".into(),
            content_type: "text/plain".into(),
            size: 0,
        },
    }]);
    let framed = body.multipart_segments("BOUNDARY", 4096).unwrap();
    let MultipartSegment::Bytes(header) = &framed[0] else {
        panic!("header");
    };
    let header = String::from_utf8(header.clone()).unwrap();
    assert!(header.contains("field%0D%0Aname"));
    assert!(header.contains("a%22b"));
}
