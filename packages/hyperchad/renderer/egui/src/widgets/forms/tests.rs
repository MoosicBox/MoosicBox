use super::*;
use crate::document::ViewOrigin;
use hyperchad_renderer::View;
use hyperchad_transformer::models::{Selector, SwapStrategy};

fn node(id: usize, element: Element, children: Vec<Container>) -> Container {
    Container {
        id,
        str_id: Some(format!("n{id}")),
        element,
        children,
        ..Container::default()
    }
}

fn input(id: usize, name: Option<&str>, input: Input) -> Container {
    node(
        id,
        Element::Input {
            input,
            name: name.map(str::to_owned),
            autofocus: None,
        },
        vec![],
    )
}

fn form(children: Vec<Container>) -> Container {
    node(
        1,
        Element::Form {
            action: Some("/submit".into()),
            method: Some("POST".into()),
            multipart: true,
        },
        children,
    )
}

fn document(container: Container) -> Document {
    let mut document = Document::new();
    document
        .apply_view(
            View {
                primary: Some(container),
                ..View::default()
            },
            ViewOrigin::Navigation,
        )
        .unwrap();
    document
}

fn key(document: &Document, id: usize) -> ElementKey {
    document.numeric_matches(id)[0]
}

fn text_input(id: usize, name: Option<&str>, value: &str) -> Container {
    input(
        id,
        name,
        Input::Text {
            value: Some(value.into()),
            placeholder: None,
        },
    )
}

fn replace(document: &mut Document, source: ElementKey, container: Container) {
    document
        .apply_view(
            View {
                primary: Some(Container::from(vec![container])),
                ..View::default()
            },
            ViewOrigin::Element {
                source,
                target: Selector::SelfTarget,
                strategy: SwapStrategy::This,
            },
        )
        .unwrap();
}

fn entries(submission: &FormSubmission) -> Vec<(&str, &str)> {
    submission
        .entries
        .iter()
        .map(|entry| {
            let FormValue::Text(value) = &entry.value else {
                panic!("expected text entry");
            };
            (entry.name.as_str(), value.as_str())
        })
        .collect()
}

#[test]
fn successful_fields_preserve_empty_repeated_names_and_hidden_values() {
    let document = document(form(vec![
        text_input(2, Some(""), "empty name"),
        text_input(3, Some("same"), "first"),
        input(
            4,
            Some("same"),
            Input::Hidden {
                value: Some("invisible".into()),
            },
        ),
        input(
            5,
            Some("checked"),
            Input::Checkbox {
                checked: Some(true),
            },
        ),
        input(
            6,
            Some("unchecked"),
            Input::Checkbox {
                checked: Some(false),
            },
        ),
        text_input(7, None, "unnamed"),
    ]));
    let mut adapter = FormsAdapter::new();
    adapter.reconcile(&document, &[]).unwrap();
    let submission = adapter.collect_submission(key(&document, 1), None).unwrap();
    assert_eq!(
        entries(&submission),
        [
            ("", "empty name"),
            ("same", "first"),
            ("same", "invisible"),
            ("checked", "on")
        ]
    );
    assert_eq!(submission.action.as_deref(), Some("/submit"));
    assert!(submission.multipart);
}

#[test]
fn unrelated_update_preserves_editing_and_identity_but_authoritative_value_resets() {
    let mut document = document(form(vec![
        text_input(2, Some("x"), "default"),
        node(3, Element::Div, vec![]),
    ]));
    let mut adapter = FormsAdapter::new();
    adapter.reconcile(&document, &[]).unwrap();
    let field_key = key(&document, 2);
    let field = adapter.fields.get_mut(&field_key).unwrap();
    let id = field.id();
    field.value = Value::Text("local edit".into());
    let revision = field.editor_revision;
    let sibling = key(&document, 3);
    replace(
        &mut document,
        sibling,
        node(
            3,
            Element::Text {
                value: "unrelated".into(),
            },
            vec![],
        ),
    );
    adapter.reconcile(&document, &[]).unwrap();
    let field = &adapter.fields[&field_key];
    assert!(field.value == Value::Text("local edit".into()));
    assert_eq!(field.id(), id);
    assert_eq!(field.editor_revision, revision);
    replace(
        &mut document,
        field_key,
        text_input(2, Some("x"), "authoritative"),
    );
    adapter.reconcile(&document, &[]).unwrap();
    let retained_key = key(&document, 2);
    let field = &adapter.fields[&retained_key];
    assert!(field.value == Value::Text("authoritative".into()));
    // Same-value writes can explicitly reset: a subtree generation alone cannot
    // distinguish those from unrelated authored updates.
    let revision_before_reset = field.editor_revision;
    adapter
        .reset_control(document.stamp(retained_key).unwrap())
        .unwrap();
    assert_eq!(
        adapter.fields[&retained_key].editor_revision,
        revision_before_reset + 1
    );
}

#[test]
fn form_reset_restores_authored_defaults_and_invalidates_file_work() {
    let document = document(form(vec![
        text_input(2, Some("x"), "initial"),
        input(3, Some("upload"), Input::File),
    ]));
    let mut adapter = FormsAdapter::new();
    adapter.reconcile(&document, &[]).unwrap();
    let text_key = key(&document, 2);
    adapter.fields.get_mut(&text_key).unwrap().value = Value::Text("edited".into());
    let file_key = key(&document, 3);
    let request = FileRequest {
        source: document.stamp(file_key).unwrap(),
        sequence: 1,
        multiple: false,
    };
    adapter.fields.get_mut(&file_key).unwrap().pending = Some(request);
    adapter.reset_form(key(&document, 1)).unwrap();
    adapter
        .complete_file(
            request,
            Some(vec![FormFile {
                name: "late".into(),
                media_type: None,
                bytes: Arc::from([1_u8]),
            }]),
        )
        .unwrap();
    assert!(adapter.fields[&file_key].files.is_empty());
    assert_eq!(adapter.pending_resources(), 0);
    assert!(adapter.fields[&text_key].value == Value::Text("initial".into()));
}

#[test]
fn file_results_reject_superseded_requests_and_accept_cancel_without_replacing_selection() {
    let document = document(form(vec![input(2, Some("file"), Input::File)]));
    let mut adapter = FormsAdapter::new();
    adapter.reconcile(&document, &[]).unwrap();
    let key = key(&document, 2);
    let old = FileRequest {
        source: document.stamp(key).unwrap(),
        sequence: 1,
        multiple: false,
    };
    let current = FileRequest { sequence: 2, ..old };
    adapter.fields.get_mut(&key).unwrap().pending = Some(current);
    adapter.complete_file(old, Some(vec![])).unwrap();
    assert_eq!(adapter.pending_resources(), 1);
    adapter
        .complete_file(
            current,
            Some(vec![FormFile {
                name: "picked".into(),
                media_type: None,
                bytes: Arc::from([1_u8, 2]),
            }]),
        )
        .unwrap();
    adapter.fields.get_mut(&key).unwrap().pending = Some(current);
    adapter.complete_file(current, None).unwrap();
    assert_eq!(adapter.fields[&key].files.len(), 1);
    let changes = adapter.take_changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].source, current.source);
    assert_eq!(adapter.pending_resources(), 0);
}

#[test]
fn multi_select_keeps_empty_and_duplicate_values_and_omits_disabled_options() {
    let options = vec![
        node(
            3,
            Element::Option {
                value: Some(String::new()),
                disabled: None,
            },
            vec![],
        ),
        node(
            4,
            Element::Option {
                value: Some("same".into()),
                disabled: None,
            },
            vec![],
        ),
        node(
            5,
            Element::Option {
                value: Some("same".into()),
                disabled: None,
            },
            vec![],
        ),
        node(
            6,
            Element::Option {
                value: Some("disabled".into()),
                disabled: Some(true),
            },
            vec![],
        ),
    ];
    let document = document(form(vec![node(
        2,
        Element::Select {
            name: Some("choice".into()),
            selected: None,
            multiple: Some(true),
            disabled: None,
            autofocus: None,
        },
        options,
    )]));
    let mut adapter = FormsAdapter::new();
    adapter.reconcile(&document, &[]).unwrap();
    adapter.fields.get_mut(&key(&document, 2)).unwrap().value =
        Value::Selected(BTreeSet::from([0, 1, 2, 3]));
    let submission = adapter.collect_submission(key(&document, 1), None).unwrap();
    assert_eq!(
        entries(&submission),
        [("choice", ""), ("choice", "same"), ("choice", "same")]
    );
}

#[test]
fn submitter_and_reset_are_explicit_coordinator_outputs() {
    let document = document(form(vec![
        text_input(2, Some("name"), "initial"),
        node(3, Element::Button { r#type: None }, vec![]),
        node(
            4,
            Element::Button {
                r#type: Some("reset".into()),
            },
            vec![],
        ),
    ]));
    let mut adapter = FormsAdapter::new();
    adapter.reconcile(&document, &[]).unwrap();
    let submitter = document.stamp(key(&document, 3)).unwrap();
    adapter.activate_button(&document, submitter).unwrap();
    let actions = adapter.take_actions();
    let FormAction::Submit(submission) = &actions[0] else {
        panic!("expected submit");
    };
    assert_eq!(submission.submitter, Some(submitter));
    adapter
        .activate_button(&document, document.stamp(key(&document, 4)).unwrap())
        .unwrap();
    assert!(matches!(
        adapter.take_actions()[0],
        FormAction::Reset { .. }
    ));
    assert!(adapter.take_actions().is_empty());
    assert!(
        adapter
            .collect_submission(key(&document, 1), document.stamp(key(&document, 2)))
            .is_err()
    );
}

fn geometry(key: ElementKey) -> NodeGeometry<ElementKey> {
    let rect = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(220.0, 32.0));
    NodeGeometry {
        key,
        container_id: 2,
        parent: None,
        containing_block: None,
        margin_box: rect,
        padding_box: rect,
        content_box: rect,
        border_box: rect,
        paint_clip: rect,
        descendant_clip: rect,
        transform: crate::geometry::GeometryTransform {
            origin: rect.min,
            scale: 1.0,
        },
        scroll_extent: rect,
        scroll_max: egui::Vec2::ZERO,
        scroll_offset: egui::Vec2::ZERO,
        visible: true,
        fixed: false,
    }
}

fn paint(
    context: &egui::Context,
    adapter: &mut FormsAdapter,
    document: &Document,
    key: ElementKey,
) -> egui::FullOutput {
    context.run(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            let authored = document.get(key).unwrap();
            adapter
                .paint_node(
                    ui,
                    document.stamp(key).unwrap(),
                    authored,
                    authored,
                    &geometry(key),
                )
                .unwrap();
        });
    })
}

#[test]
fn native_cursor_selection_and_focus_survive_unrelated_reconcile() {
    let mut document = document(form(vec![
        text_input(2, Some("name"), "abcdef"),
        node(3, Element::Div, vec![]),
    ]));
    let mut adapter = FormsAdapter::new();
    adapter.reconcile(&document, &[]).unwrap();
    let key = key(&document, 2);
    let context = egui::Context::default();
    let _ = paint(&context, &mut adapter, &document, key);
    let id = adapter.fields[&key].id();
    context.memory_mut(|memory| memory.request_focus(id));
    let mut editor = egui::text_edit::TextEditState::load(&context, id).unwrap();
    let selection =
        egui::text::CCursorRange::two(egui::text::CCursor::new(1), egui::text::CCursor::new(4));
    editor.cursor.set_char_range(Some(selection));
    editor.store(&context, id);
    let sibling = document.numeric_matches(3)[0];
    replace(
        &mut document,
        sibling,
        node(
            3,
            Element::Text {
                value: "changed sibling".into(),
            },
            vec![],
        ),
    );
    adapter.reconcile(&document, &[]).unwrap();
    let _ = paint(&context, &mut adapter, &document, key);
    assert_eq!(context.memory(egui::Memory::focused), Some(id));
    assert_eq!(
        egui::text_edit::TextEditState::load(&context, id)
            .unwrap()
            .cursor
            .char_range(),
        Some(selection)
    );
    adapter.reset_control(document.stamp(key).unwrap()).unwrap();
    let _ = paint(&context, &mut adapter, &document, key);
    assert_eq!(context.memory(egui::Memory::focused), Some(id));
    assert_ne!(
        egui::text_edit::TextEditState::load(&context, id)
            .unwrap()
            .cursor
            .char_range(),
        Some(selection)
    );
}

#[test]
fn password_native_output_never_contains_plaintext() {
    let mut input = input(
        2,
        Some("password"),
        Input::Password {
            value: Some("private-password-sentinel".into()),
            placeholder: None,
        },
    );
    input
        .data
        .insert("aria-label".into(), "Account password".into());
    let document = document(form(vec![input]));
    let mut adapter = FormsAdapter::new();
    adapter.reconcile(&document, &[]).unwrap();
    let key = key(&document, 2);
    let context = egui::Context::default();
    let _ = paint(&context, &mut adapter, &document, key);
    context.memory_mut(|memory| memory.request_focus(adapter.fields[&key].id()));
    let output = paint(&context, &mut adapter, &document, key);
    assert!(!format!("{:?}", output.platform_output.events).contains("private-password-sentinel"));
}

#[test]
fn trait_changes_forward_lossless_selection_and_file_bytes_once() {
    use crate::events::EventValue;
    let document = document(form(vec![text_input(2, Some("x"), "default")]));
    let mut adapter = FormsAdapter::new();
    adapter.reconcile(&document, &[]).unwrap();
    let source = document.stamp(key(&document, 2)).unwrap();
    adapter.changes.push(FormChange {
        source,
        values: vec![
            FormValue::Text(String::new()),
            FormValue::Text("repeated".into()),
            FormValue::Text("repeated".into()),
        ],
    });
    adapter.changes.push(FormChange {
        source,
        values: vec![FormValue::File(FormFile {
            name: "file".into(),
            media_type: None,
            bytes: Arc::from([0_u8, 255]),
        })],
    });
    let changes = WidgetAdapter::take_changes(&mut adapter);
    assert_eq!(
        changes[0],
        (
            source,
            EventValue::Array(vec![
                EventValue::String(String::new()),
                EventValue::String("repeated".into()),
                EventValue::String("repeated".into())
            ])
        )
    );
    let EventValue::Array(files) = &changes[1].1 else {
        panic!("expected file array");
    };
    let EventValue::Object(file) = &files[0] else {
        panic!("expected file object");
    };
    assert_eq!(
        file["bytes"],
        EventValue::Array(vec![EventValue::Number(0.0), EventValue::Number(255.0)])
    );
    assert_eq!(file["mediaType"], EventValue::Null);
    assert_eq!(WidgetAdapter::take_changes(&mut adapter), []);
    assert!(adapter.take_changes().is_empty());
}

#[test]
fn removed_fields_do_not_leak_edits_into_new_identity() {
    let mut document = document(form(vec![
        text_input(2, Some("x"), "default"),
        input(3, Some("file"), Input::File),
    ]));
    let mut adapter = FormsAdapter::new();
    adapter.reconcile(&document, &[]).unwrap();
    let form_key = key(&document, 1);
    let old = key(&document, 2);
    let old_file = key(&document, 3);
    adapter.fields.get_mut(&old).unwrap().value = Value::Text("secret local edit".into());
    let request = FileRequest {
        source: document.stamp(old_file).unwrap(),
        sequence: 1,
        multiple: false,
    };
    adapter.fields.get_mut(&old_file).unwrap().pending = Some(request);
    // A missing primary means "no primary operation", even with Delete.
    // Delete both controls explicitly, while keeping their enclosing form live.
    let mutation = document
        .apply_view(
            View {
                primary: Some(Container::default()),
                delete_selectors: vec![Selector::Id("n3".into())],
                ..View::default()
            },
            ViewOrigin::Element {
                source: old,
                target: Selector::SelfTarget,
                strategy: SwapStrategy::Delete,
            },
        )
        .unwrap();
    assert!(document.get(old).is_none());
    assert!(document.get(old_file).is_none());
    assert!(mutation.removed.iter().any(|stamp| stamp.key == old));
    assert!(mutation.removed.iter().any(|stamp| stamp.key == old_file));
    adapter.reconcile(&document, &[mutation]).unwrap();
    assert!(!adapter.fields.contains_key(&old));
    assert!(!adapter.fields.contains_key(&old_file));
    assert_eq!(adapter.pending_resources(), 0);

    // Reusing authored IDs is not permission to reuse retired native state.
    let mutation = document
        .apply_view(
            View {
                primary: Some(Container::from(vec![
                    text_input(2, Some("x"), "fresh default"),
                    input(3, Some("file"), Input::File),
                ])),
                ..View::default()
            },
            ViewOrigin::Element {
                source: form_key,
                target: Selector::SelfTarget,
                strategy: SwapStrategy::BeforeEnd,
            },
        )
        .unwrap();
    adapter.reconcile(&document, &[mutation]).unwrap();
    let fresh = key(&document, 2);
    let fresh_file = key(&document, 3);
    assert_ne!(fresh, old);
    assert_ne!(fresh_file, old_file);
    assert_ne!(
        adapter.fields[&fresh].id(),
        Id::new(("hyperchad-form", old))
    );
    assert!(adapter.fields[&fresh].value == Value::Text("fresh default".into()));
    adapter
        .complete_file(
            request,
            Some(vec![FormFile {
                name: "stale-private-file".into(),
                media_type: None,
                bytes: Arc::from([42_u8]),
            }]),
        )
        .unwrap();
    assert!(adapter.fields[&fresh_file].files.is_empty());
    let submission = adapter.collect_submission(form_key, None).unwrap();
    let FormValue::Text(value) = &submission.entries[0].value else {
        panic!("expected text");
    };
    assert_eq!(value, "fresh default");
}

#[test]
fn keyed_children_update_preserves_live_control_identity_and_local_edits() {
    let mut document = document(form(vec![text_input(2, Some("x"), "default")]));
    let mut adapter = FormsAdapter::new();
    adapter.reconcile(&document, &[]).unwrap();
    let form_key = key(&document, 1);
    let retained = key(&document, 2);
    adapter.fields.get_mut(&retained).unwrap().value = Value::Text("local edit".into());
    let mutation = document
        .apply_view(
            View {
                primary: Some(Container::from(vec![
                    text_input(2, Some("x"), "default"),
                    node(3, Element::Div, vec![]),
                ])),
                ..View::default()
            },
            ViewOrigin::Element {
                source: form_key,
                target: Selector::SelfTarget,
                strategy: SwapStrategy::Children,
            },
        )
        .unwrap();
    assert_eq!(key(&document, 2), retained);
    assert!(!mutation.removed.iter().any(|stamp| stamp.key == retained));
    adapter.reconcile(&document, &[mutation]).unwrap();
    assert!(adapter.fields[&retained].value == Value::Text("local edit".into()));
}
