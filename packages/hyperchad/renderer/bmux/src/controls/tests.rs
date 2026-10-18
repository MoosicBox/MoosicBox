use super::*;
use bmux_keyboard::KeyStroke;
use bmux_tui::buffer::Buffer;
use hyperchad_transformer::{Element, Input};

fn node(id: &str, element: Element) -> Container {
    Container {
        str_id: Some(id.into()),
        element,
        ..Container::default()
    }
}

#[test]
fn multirow_button_wraps_complete_library_action_without_native_chrome() {
    use crate::layout::{DefaultLeafMeasure, LayoutOptions, layout};
    let mut button = node(
        "remove",
        Element::Button {
            r#type: Some("button".into()),
        },
    );
    button.direction = hyperchad_transformer::models::LayoutDirection::Row;
    button.width = Some(hyperchad_transformer::Number::Integer(130));
    button.height = Some(hyperchad_transformer::Number::Integer(40));
    button.children = vec![node(
        "label",
        Element::Raw {
            value: "Remove from Library".into(),
        },
    )];
    let root = Container {
        children: vec![button],
        ..Default::default()
    };
    let tree = layout(&root, &LayoutOptions::default(), &DefaultLeafMeasure).unwrap();
    let bounds = tree.get("remove").unwrap().content_bounds;
    let fragments: Vec<_> = tree.nodes.iter().flat_map(|node| &node.text).collect();
    assert!(
        fragments
            .iter()
            .any(|fragment| fragment.bounds.y > bounds.y),
        "production row button must actually wrap"
    );
    assert!(
        fragments
            .iter()
            .all(|fragment| fragment.bounds.intersection(bounds) == fragment.bounds),
        "every label fragment stays inside the button box without clipping"
    );
    let mut controls = Controls::new();
    controls.reconcile(&root);
    controls.prepare(&tree);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 24));
    {
        let mut frame = bmux_tui::frame::Frame::new(&mut buffer);
        let mut cx = PaintCx::new(&mut frame);
        crate::presentation::paint_text(&tree, &controls, &mut cx);
        controls.paint("remove", &mut cx);
    }
    let rows: Vec<_> = (0..3)
        .map(|row| buffer.row_symbols(row).unwrap().trim().to_owned())
        .collect();
    assert_eq!(
        rows.join(" ").split_whitespace().collect::<Vec<_>>(),
        ["Remove", "from", "Library"]
    );
    assert!(
        !rows.join("").contains('['),
        "no one-line native label over wrapped source text"
    );
    assert!(controls.nodes["remove"].spec.button_content == ButtonContent::CompositeChildren);
    controls.focus(Some("remove"));
    let outcome = controls.event(&key(KeyCode::Enter));
    assert_eq!(
        outcome
            .events
            .iter()
            .filter(|event| matches!(event, ControlEvent::Activate { key } if key == "remove"))
            .count(),
        1
    );
}

#[test]
fn composite_button_retains_single_native_keyboard_and_mouse_activation() {
    use bmux_tui::event::{MouseButton, MouseEvent, MouseEventKind};
    let mut button = node(
        "play",
        Element::Button {
            r#type: Some("button".into()),
        },
    );
    button.children = vec![
        node(
            "icon",
            Element::Image {
                source: Some("/public/img/play-button.svg".into()),
                alt: None,
                fit: None,
                source_set: None,
                sizes: None,
                loading: None,
            },
        ),
        node(
            "label",
            Element::Raw {
                value: "Play".into(),
            },
        ),
    ];
    let mut controls = Controls::new();
    controls.reconcile(&button);
    assert_eq!(controls.nodes["play"].spec.label, "Play");
    assert!(controls.nodes["play"].spec.button_content == ButtonContent::CompositeChildren);
    prepare(&mut controls, "play", 16, 3);
    controls.focus(Some("play"));
    for code in [KeyCode::Enter, KeyCode::Space] {
        let outcome = controls.event(&key(code));
        assert_eq!(
            outcome
                .events
                .iter()
                .filter(|event| matches!(event, ControlEvent::Activate { key } if key == "play"))
                .count(),
            1
        );
    }
    let mouse = |kind| Event::Mouse(MouseEvent::new(kind, Point::new(2, 0)));
    let down = controls.event(&mouse(MouseEventKind::Down(MouseButton::Left)));
    let up = controls.event(&mouse(MouseEventKind::Up(MouseButton::Left)));
    assert_eq!(
        down.events
            .iter()
            .chain(&up.events)
            .filter(|event| matches!(event, ControlEvent::Activate { key } if key == "play"))
            .count(),
        1
    );
}

fn input(id: &str, value: Option<&str>) -> Container {
    node(
        id,
        Element::Input {
            input: Input::Text {
                value: value.map(str::to_owned),
                placeholder: None,
            },
            name: Some(id.into()),
            autofocus: None,
        },
    )
}

fn key(code: KeyCode) -> Event {
    Event::Key(KeyStroke::simple(code))
}

fn prepare(controls: &mut Controls, key: &str, width: u64, height: u64) {
    let control = controls.nodes.get_mut(key).unwrap();
    let layout = control.component(key, true).layout(
        Constraints::new(width, width, height, Some(height)),
        &mut LayoutCx::new(),
    );
    control.prepared = Some(Prepared {
        layout,
        bounds: CellRect::new(0, 0, width, height),
        clip: CellRect::new(0, 0, width, height),
        order: 0,
        path: Vec::new(),
        hit_regions: Vec::new(),
    });
}

#[test]
fn compatible_reconcile_preserves_native_history_but_external_change_resets() {
    let mut root = input("edit", Some("a"));
    let mut controls = Controls::new();
    controls.reconcile(&root);
    prepare(&mut controls, "edit", 20, 1);
    controls.focus(Some("edit"));
    controls.event(&Event::Paste("b".into()));
    let edited = controls.value("edit");
    controls.reconcile(&root);
    assert_eq!(controls.value("edit"), edited);
    if let NativeState::Text(state) = &mut controls.nodes.get_mut("edit").unwrap().state {
        assert!(state.get_mut().buffer_mut().undo());
        assert_eq!(state.get_mut().buffer().text(), "a");
    } else {
        panic!("native edit state missing");
    }
    if let Element::Input {
        input: Input::Text { value, .. },
        ..
    } = &mut root.element
    {
        *value = Some("server".into());
    }
    controls.reconcile(&root);
    assert_eq!(
        controls.value("edit"),
        Some(ControlValue::Text("server".into()))
    );
}

#[test]
fn uncontrolled_values_and_checkbox_edits_survive_absent_defaults() {
    let mut root = input("edit", None);
    let mut controls = Controls::new();
    controls.reconcile(&root);
    prepare(&mut controls, "edit", 20, 1);
    controls.focus(Some("edit"));
    controls.event(&Event::Paste("local".into()));
    controls.reconcile(&root);
    assert_eq!(
        controls.value("edit"),
        Some(ControlValue::Text("local".into()))
    );
    root = node(
        "check",
        Element::Input {
            input: Input::Checkbox { checked: None },
            name: None,
            autofocus: None,
        },
    );
    controls.reconcile(&root);
    prepare(&mut controls, "check", 20, 1);
    controls.focus(Some("check"));
    controls.event(&key(KeyCode::Space));
    controls.reconcile(&root);
    assert_eq!(controls.value("check"), Some(ControlValue::Checked(true)));
}

#[test]
fn multi_select_keyboard_changes_only_active_enabled_row() {
    let mut root = node(
        "select",
        Element::Select {
            name: Some("tags".into()),
            selected: None,
            multiple: Some(true),
            disabled: None,
            autofocus: None,
        },
    );
    root.children = [false, true, false]
        .iter()
        .enumerate()
        .map(|(index, disabled)| {
            let mut option = node(
                &format!("opt{index}"),
                Element::Option {
                    value: Some(index.to_string()),
                    disabled: Some(*disabled),
                },
            );
            option.children.push(node(
                "",
                Element::Text {
                    value: index.to_string(),
                },
            ));
            option
        })
        .collect();
    let mut controls = Controls::new();
    controls.reconcile(&root);
    prepare(&mut controls, "select", 20, 3);
    controls.focus(Some("select"));
    controls.event(&key(KeyCode::Space));
    assert_eq!(
        controls.value("select"),
        Some(ControlValue::Multiple(vec!["0".into()]))
    );
    controls.event(&key(KeyCode::Down));
    controls.event(&key(KeyCode::Space));
    assert_eq!(
        controls.value("select"),
        Some(ControlValue::Multiple(vec!["0".into(), "2".into()]))
    );
}

#[test]
fn paste_sanitizes_terminal_controls_and_password_never_paints_secret() {
    let root = node(
        "secret",
        Element::Input {
            input: Input::Password {
                value: Some("秘密".into()),
                placeholder: None,
            },
            name: Some("password".into()),
            autofocus: None,
        },
    );
    let mut controls = Controls::new();
    controls.reconcile(&root);
    prepare(&mut controls, "secret", 12, 1);
    controls.focus(Some("secret"));
    let result = controls.event(&Event::Paste("\u{1b}\n\tZ\u{7f}".into()));
    assert!(result.events.iter().any(|event| matches!(
        event,
        ControlEvent::Change {
            value: ControlValue::SecretChanged,
            ..
        }
    )));
    let mut buffer = Buffer::empty(Rect::new(0, 0, 12, 1));
    let mut frame = bmux_tui::frame::Frame::new(&mut buffer);
    let mut cx = PaintCx::new(&mut frame);
    controls.paint("secret", &mut cx);
    assert!(!buffer.row_symbols(0).unwrap().contains('秘'));
    let values = controls.action_values();
    if let crate::actions::ControlValue::Text(value) = &values["secret"] {
        assert!(value.contains('Z'));
        assert!(!value.chars().any(char::is_control));
    } else {
        panic!("password missing");
    }
}

#[test]
fn canonical_tree_is_unchanged_and_form_names_remain_ordered() {
    let mut root = node(
        "form",
        Element::Form {
            action: None,
            method: None,
            multipart: false,
        },
    );
    root.children = vec![
        input("first", Some("one")),
        input("second", Some("two")),
        node(
            "hidden",
            Element::Input {
                input: Input::Hidden {
                    value: Some("three".into()),
                },
                name: Some("second".into()),
                autofocus: None,
            },
        ),
    ];
    let before = root.clone();
    let mut controls = Controls::new();
    controls.reconcile(&root);
    assert_eq!(root, before);
    assert_eq!(
        controls.form_values("form").0,
        vec![
            ("first".into(), "one".into()),
            ("second".into(), "two".into()),
            ("second".into(), "three".into())
        ]
    );
}

#[test]
fn disabled_ancestor_prevents_focus_and_input() {
    let mut root = Container::default();
    root.data.insert("disabled".into(), "true".into());
    root.children.push(input("edit", None));
    let mut controls = Controls::new();
    controls.reconcile(&root);
    prepare(&mut controls, "edit", 20, 1);
    assert!(!controls.focus(Some("edit")).consumed);
    assert!(
        !controls
            .dispatch_to("edit", &Event::Paste("blocked".into()))
            .consumed
    );
}

#[test]
fn measurements_use_native_component_chrome() {
    let mut root = node(
        "button",
        Element::Button {
            r#type: Some("button".into()),
        },
    );
    root.children
        .push(node("", Element::Text { value: "Go".into() }));
    let mut controls = Controls::new();
    controls.reconcile(&root);
    let size = controls
        .measure("button", Constraints::new(0, 80, 0, None))
        .unwrap();
    assert_eq!(size, LogicalSize::new(6, 1));
}

#[test]
fn inline_anchor_preserves_full_mixed_text_and_neighbor_cells() {
    let mut anchor = node(
        "link",
        Element::Anchor {
            href: Some("/album".into()),
            target: None,
        },
    );
    let mut span = node("styled", Element::Span);
    span.children.push(node(
        "title",
        Element::Text {
            value: "First Light".into(),
        },
    ));
    anchor.children.push(span);
    let mut controls = Controls::new();
    controls.reconcile(&anchor);
    assert_eq!(
        controls.measure("link", Constraints::new(0, 120, 0, None)),
        Some(LogicalSize::new(11, 1))
    );
    prepare(&mut controls, "link", 11, 1);
    let mut buffer = Buffer::empty(Rect::new(0, 0, 120, 60));
    buffer.write_line(
        Rect::new(0, 0, 30, 1),
        &bmux_tui::text::Line::from("First Light · lossless"),
    );
    let before = buffer.row_symbols(0).unwrap();
    {
        let mut frame = bmux_tui::frame::Frame::new(&mut buffer);
        assert!(!controls.paint("link", &mut PaintCx::new(&mut frame)));
        assert!(
            frame
                .hits()
                .regions()
                .iter()
                .any(|region| region.id.as_str() == "link")
        );
    }
    assert_eq!(buffer.row_symbols(0).unwrap(), before);
    controls.focus(Some("link"));
    assert!(
        controls
            .inline_style(&[0, 0], bmux_tui::style::Style::new())
            .modifiers
            .contains(bmux_tui::style::Modifier::REVERSED)
    );
    let result = controls.event(&key(KeyCode::Enter));
    assert!(
        result
            .events
            .iter()
            .any(|event| matches!(event, ControlEvent::Activate { key } if key == "link"))
    );
}

#[test]
fn inline_anchor_mouse_uses_fragment_regions_not_bounding_box_gaps() {
    use bmux_tui::event::MouseEvent;
    let mut anchor = node(
        "link",
        Element::Anchor {
            href: Some("/album".into()),
            target: None,
        },
    );
    anchor.children.push(node(
        "title",
        Element::Text {
            value: "First Light".into(),
        },
    ));
    let mut controls = Controls::new();
    controls.reconcile(&anchor);
    prepare(&mut controls, "link", 11, 2);
    controls
        .nodes
        .get_mut("link")
        .unwrap()
        .prepared
        .as_mut()
        .unwrap()
        .hit_regions = vec![CellRect::new(6, 0, 5, 1), CellRect::new(0, 1, 6, 1)];
    let mouse = |kind, x, y| Event::Mouse(MouseEvent::new(kind, Point::new(x, y)));
    assert!(
        !controls
            .event(&mouse(MouseEventKind::Down(MouseButton::Left), 1, 0))
            .consumed
    );
    controls.event(&mouse(MouseEventKind::Down(MouseButton::Left), 7, 0));
    let result = controls.event(&mouse(MouseEventKind::Up(MouseButton::Left), 7, 0));
    assert!(
        result
            .events
            .iter()
            .any(|event| matches!(event, ControlEvent::Activate { .. }))
    );
    controls.event(&mouse(MouseEventKind::Down(MouseButton::Left), 7, 0));
    let result = controls.event(&mouse(MouseEventKind::Up(MouseButton::Left), 1, 0));
    assert!(
        !result
            .events
            .iter()
            .any(|event| matches!(event, ControlEvent::Activate { .. }))
    );
}
