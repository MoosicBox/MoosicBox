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
fn details_open_work_depends_only_on_details() {
    for unrelated in [0, 100, 1_000] {
        let mut inner = node("inner", Element::Details { open: Some(true) });
        inner.children.push(node("inner-summary", Element::Summary));
        let mut outer = node("outer", Element::Details { open: Some(false) });
        outer.children = vec![node("outer-summary", Element::Summary), inner];
        let mut root = Container::default();
        root.children.push(outer);
        root.children.extend(
            (0..unrelated)
                .map(|index| node(&format!("button-{index}"), Element::Button { r#type: None })),
        );
        let mut controls = Controls::new();
        controls.reconcile(&root);
        assert_eq!(controls.nodes.len(), unrelated + 4);
        for _ in 0..2 {
            let mut visited = 0;
            let actual = controls.details_open_with_visit(|| visited += 1);
            assert_eq!(actual, controls.details_open());
            assert_eq!(
                actual,
                BTreeMap::from([("inner".into(), true), ("outer".into(), false)])
            );
            assert_eq!(
                visited, 2,
                "details query inspected unrelated controls: {unrelated}"
            );
        }
    }
}

fn assert_details(controls: &Controls, expected: &[(&str, bool)]) {
    let mut visited = 0;
    let actual = controls.details_open_with_visit(|| visited += 1);
    assert_eq!(actual, controls.details_open());
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|(key, open)| ((*key).into(), *open))
            .collect()
    );
    assert_eq!(visited, expected.len());
}

#[test]
fn details_index_tracks_reconciliation_and_reset() {
    let mut inner = node("inner", Element::Details { open: Some(true) });
    inner.children.push(node("inner-summary", Element::Summary));
    let mut outer = node("outer", Element::Details { open: Some(false) });
    outer.children = vec![node("outer-summary", Element::Summary), inner];
    let mut root = node(
        "form",
        Element::Form {
            action: None,
            method: None,
            multipart: false,
        },
    );
    root.children.push(outer);
    let mut controls = Controls::new();
    assert_details(&controls, &[]);
    controls.reconcile(&root);
    assert_details(&controls, &[("inner", true), ("outer", false)]);

    // Local toggles survive unchanged external values and repeated reconciliation.
    controls.activate("outer-summary");
    controls.activate("inner-summary");
    assert_details(&controls, &[("inner", false), ("outer", true)]);
    for _ in 0..2 {
        controls.reconcile(&root);
        assert_details(&controls, &[("inner", false), ("outer", true)]);
    }
    controls.reset("form");
    assert_details(&controls, &[("inner", true), ("outer", false)]);

    // Changed external values replace local values, including nested Details.
    root.children[0].element = Element::Details { open: Some(true) };
    root.children[0].children[1].element = Element::Details { open: Some(false) };
    controls.reconcile(&root);
    assert_details(&controls, &[("inner", false), ("outer", true)]);

    // Same-kind identity replacement must reset local state via the real report lane.
    controls.activate("outer-summary");
    let report = crate::state::ApplyReport {
        removed: BTreeSet::from(["outer".into()]),
        ..Default::default()
    };
    controls.reconcile_report(&root, &report);
    assert_details(&controls, &[("inner", false), ("outer", true)]);

    // Reused keys changing kind in both directions cannot leave stale membership.
    root.children[0].children[1].element = Element::Button { r#type: None };
    controls.reconcile(&root);
    assert_details(&controls, &[("outer", true)]);
    root.children[0].children[1].element = Element::Details { open: None };
    controls.reconcile(&root);
    assert_details(&controls, &[("inner", false), ("outer", true)]);

    root.children[0].children.pop();
    controls.reconcile_report(
        &root,
        &crate::state::ApplyReport {
            removed: BTreeSet::from(["inner".into()]),
            ..Default::default()
        },
    );
    assert_details(&controls, &[("outer", true)]);
    controls.reconcile(&Container::default());
    assert_details(&controls, &[]);
    controls.reconcile(&root);
    assert_details(&controls, &[("outer", true)]);
}

#[test]
fn focused_editor_paints_visible_cursor_and_contrasting_input_chrome() {
    let root = node(
        "url",
        Element::Input {
            input: Input::Text {
                value: Some("http://localhost:8500".into()),
                placeholder: None,
            },
            name: Some("api-url".into()),
            autofocus: None,
        },
    );
    let mut controls = Controls::new();
    controls.reconcile(&root);
    prepare(&mut controls, "url", 40, 1);
    controls.focus(Some("url"));
    let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 1));
    let mut frame = bmux_tui::frame::Frame::new(&mut buffer);
    controls.paint("url", &mut PaintCx::new(&mut frame));
    assert!(
        frame.cursor().is_some_and(|cursor| cursor.visible),
        "focused input must request a terminal caret"
    );
    assert!(
        buffer
            .cells()
            .iter()
            .any(|cell| { cell.style.bg == Some(bmux_tui::style::Color::Rgb(238, 238, 238)) }),
        "focused editor must paint a visible block caret for embedded hosts"
    );
    assert!(
        buffer
            .cells()
            .iter()
            .any(|cell| cell.style.bg == Some(bmux_tui::style::Color::Rgb(45, 65, 85)))
    );
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
    if !controls
        .prepared_keys
        .iter()
        .any(|prepared| prepared == key)
    {
        controls.prepared_keys.push(key.to_owned());
    }
}

fn assert_prepared_membership(controls: &Controls) {
    let actual: BTreeSet<_> = controls
        .nodes
        .iter()
        .filter(|(_, control)| control.prepared.is_some())
        .map(|(key, _)| key.clone())
        .collect();
    let indexed: BTreeSet<_> = controls.prepared_keys.iter().cloned().collect();
    assert_eq!(actual, indexed);
}

#[test]
fn mouse_target_work_is_prepared_not_registered_controls() {
    use bmux_tui::event::MouseEvent;
    for unrelated in [100, 1_000] {
        let mut root = Container {
            children: (0..unrelated)
                .map(|index| {
                    node(
                        &format!("offscreen-{index}"),
                        Element::Button { r#type: None },
                    )
                })
                .collect(),
            ..Default::default()
        };
        root.children.extend([
            node("z", Element::Button { r#type: None }),
            node("a", Element::Button { r#type: None }),
        ]);
        let mut controls = Controls::new();
        controls.reconcile(&root);
        // Reverse lexical order deliberately: equal-order targeting must retain
        // the last BTreeMap match, not the last prepared-key insertion.
        prepare(&mut controls, "z", 10, 1);
        prepare(&mut controls, "a", 10, 1);
        assert_prepared_membership(&controls);
        let mouse = MouseEvent::new(MouseEventKind::ScrollDown, Point::new(2, 0));
        let mut visited = 0;
        assert_eq!(
            controls
                .mouse_target_with_visit(&mouse, || visited += 1)
                .as_deref(),
            Some("z")
        );
        assert_eq!(visited, 2, "unrelated controls: {unrelated}");
    }
}

#[test]
fn mouse_target_preserves_priority_clipping_capture_and_admission() {
    use bmux_tui::event::MouseEvent;
    let root = Container {
        children: vec![
            node("z", Element::Button { r#type: None }),
            node("a", Element::Button { r#type: None }),
        ],
        ..Default::default()
    };
    let mut controls = Controls::new();
    controls.reconcile(&root);
    prepare(&mut controls, "z", 10, 1);
    prepare(&mut controls, "a", 10, 1);
    let mouse = MouseEvent::new(MouseEventKind::Down(MouseButton::Left), Point::new(2, 0));
    assert_eq!(
        controls.mouse_target_with_visit(&mouse, || {}).as_deref(),
        Some("z")
    );
    controls
        .nodes
        .get_mut("a")
        .unwrap()
        .prepared
        .as_mut()
        .unwrap()
        .order = 1;
    assert_eq!(
        controls.mouse_target_with_visit(&mouse, || {}).as_deref(),
        Some("a")
    );
    controls.nodes.get_mut("a").unwrap().spec.disabled = true;
    assert_eq!(
        controls.mouse_target_with_visit(&mouse, || {}).as_deref(),
        Some("z")
    );
    controls.nodes.get_mut("a").unwrap().spec.disabled = false;
    controls
        .nodes
        .get_mut("a")
        .unwrap()
        .prepared
        .as_mut()
        .unwrap()
        .clip = CellRect::new(5, 0, 5, 1);
    assert_eq!(
        controls.mouse_target_with_visit(&mouse, || {}).as_deref(),
        Some("z")
    );
    controls
        .nodes
        .get_mut("a")
        .unwrap()
        .prepared
        .as_mut()
        .unwrap()
        .clip = CellRect::new(0, 0, 10, 1);
    // Reject the topmost replacement; never fall through to the lower owner.
    let outcome = controls.event_admitted(&Event::Mouse(mouse), |key| key != "a");
    assert!(outcome.consumed);
    assert!(controls.captured.is_none());
    controls.captured = Some("z".into());
    let mut visited = 0;
    assert_eq!(
        controls
            .mouse_target_with_visit(&mouse, || visited += 1)
            .as_deref(),
        Some("z")
    );
    assert_eq!(visited, 0);
    controls.event_admitted(&Event::Mouse(mouse), |_| false);
    assert!(controls.captured.is_none());
}

#[test]
fn prepared_membership_survives_projection_focus_reset_and_restoration() {
    use crate::layout::{DefaultLeafMeasure, LayoutOptions, RetainedLayout, layout};
    use std::sync::Arc;
    let mut root = node(
        "form",
        Element::Form {
            action: None,
            method: None,
            multipart: false,
        },
    );
    root.overflow_y = hyperchad_transformer::models::LayoutOverflow::Auto;
    root.children = (0..20)
        .map(|index| {
            let mut child = input(&format!("input-{index}"), Some("value"));
            child.height = Some(hyperchad_transformer::Number::Integer(16));
            child
        })
        .collect();
    let mut controls = Controls::new();
    controls.reconcile(&root);
    let tree = Arc::new(
        layout(
            &root,
            &LayoutOptions {
                viewport: bmux_tui::geometry::Size::new(30, 4),
                ..Default::default()
            },
            &DefaultLeafMeasure,
        )
        .unwrap(),
    );
    controls.prepare(&tree);
    assert_prepared_membership(&controls);
    // Full-layout deferred focus materializes a previously offscreen placement.
    assert!(controls.nodes["input-19"].prepared.is_none());
    controls.focus(Some("input-19"));
    assert!(controls.nodes["input-19"].prepared.is_some());
    assert_prepared_membership(&controls);
    let retained = RetainedLayout::new(tree);
    let committed = retained.project(&BTreeMap::new()).unwrap();
    controls.prepare_projection(&committed);
    assert_prepared_membership(&controls);
    controls.reset("form");
    assert_prepared_membership(&controls);
    // Failed/uncommitted candidate geometry must be restorable through the same
    // projection-preparation path used by the program's committed input scene.
    let candidate = committed
        .reproject(&BTreeMap::from([(
            "form".into(),
            crate::layout::ScrollOffset { x: 0, y: 10 },
        )]))
        .unwrap();
    controls.prepare_projection(&candidate);
    assert_prepared_membership(&controls);
    controls.prepare_projection(&committed);
    assert_prepared_membership(&controls);
    controls.focus(Some("input-18"));
    assert!(controls.nodes["input-18"].prepared.is_some());
    assert_prepared_membership(&controls);
    controls.reconcile(&Container::default());
    assert_prepared_membership(&controls);
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
