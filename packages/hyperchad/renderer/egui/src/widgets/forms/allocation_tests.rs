//! Shared allocation, not native intrinsic minima, owns form paint/hit bounds.
use super::*;
use crate::document::ViewOrigin;
use hyperchad_renderer::View;

fn fixture(element: Element) -> (Document, FormsAdapter, ElementStamp) {
    let mut document = Document::new();
    document
        .apply_view(
            View {
                primary: Some(Container {
                    id: 1,
                    element,
                    ..Container::default()
                }),
                ..View::default()
            },
            ViewOrigin::Navigation,
        )
        .unwrap();
    let mut forms = FormsAdapter::new();
    forms.reconcile(&document, &[]).unwrap();
    let source = forms.control_stamps()[0];
    (document, forms, source)
}

fn geometry(source: ElementStamp, size: egui::Vec2) -> NodeGeometry<ElementKey> {
    let rect = egui::Rect::from_min_size(egui::pos2(40.0, 40.0), size);
    NodeGeometry {
        key: source.key,
        container_id: 1,
        parent: None,
        containing_block: None,
        margin_box: rect,
        padding_box: rect,
        content_box: rect,
        border_box: rect,
        paint_clip: rect.shrink2(size * 0.1),
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

fn paint_control(
    context: &egui::Context,
    document: &Document,
    forms: &mut FormsAdapter,
    source: ElementStamp,
    geometry: &NodeGeometry<ElementKey>,
    input: egui::RawInput,
) -> (egui::FullOutput, egui::Response) {
    let mut response = None;
    let output = context.run(input, |context| {
        let mut ui = egui::Ui::new(
            context.clone(),
            Id::new("allocation-test"),
            egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
        );
        let authored = document.get(source.key).unwrap();
        response = Some(
            forms
                .paint_node(&mut ui, source, authored, authored, geometry)
                .unwrap()
                .response,
        );
    });
    (output, response.unwrap())
}

fn assert_control_bounds(element: Element, size: egui::Vec2, editable: bool) {
    let (document, mut forms, source) = fixture(element);
    let context = egui::Context::default();
    // Native DOM geometry is exact, unlike egui's default nearest-widget
    // selection within five points. This policy must be installed by the frame
    // host before begin_pass: hover/click candidates are chosen at frame start.
    context.style_mut(|style| style.interaction.interact_radius = 0.0);
    let geometry = geometry(source, size);
    let original_value = forms.current_value(source).unwrap();
    if editable {
        forms.focus(&context, source).unwrap();
        forms.select_all(&context, source).unwrap();
    }
    let (output, response) = paint_control(
        &context,
        &document,
        &mut forms,
        source,
        &geometry,
        egui::RawInput::default(),
    );
    assert_eq!(response.rect, response.rect.intersect(geometry.content_box));
    assert_eq!(
        response.interact_rect,
        response.interact_rect.intersect(geometry.paint_clip)
    );
    let registered = context.read_response(response.id).unwrap();
    assert_eq!(
        registered.interact_rect,
        registered.interact_rect.intersect(geometry.paint_clip)
    );
    assert!(
        output.shapes.iter().any(|shape| {
            shape.clip_rect == geometry.paint_clip && !matches!(shape.shape, egui::Shape::Noop)
        }),
        "native control must actually paint"
    );
    for shape in output
        .shapes
        .iter()
        .filter(|shape| !matches!(shape.shape, egui::Shape::Noop))
    {
        assert_eq!(
            shape.clip_rect,
            shape.clip_rect.intersect(geometry.paint_clip)
        );
    }
    if editable {
        let id = forms.focus_id(source).unwrap();
        let selection = egui::text_edit::TextEditState::load(&context, id)
            .unwrap()
            .cursor
            .char_range()
            .unwrap();
        assert_eq!(
            selection.primary.index.max(selection.secondary.index),
            match &original_value {
                crate::events::EventValue::String(value) => value.chars().count(),
                _ => panic!("expected text"),
            }
        );
        assert_eq!(context.memory(egui::Memory::focused), Some(id));
        assert_eq!(forms.current_value(source).unwrap(), original_value);
    }
    assert_outside_pointer_rejected(
        &context,
        &document,
        &mut forms,
        source,
        &geometry,
        &registered,
        &original_value,
    );
}

fn assert_outside_pointer_rejected(
    context: &egui::Context,
    document: &Document,
    forms: &mut FormsAdapter,
    source: ElementStamp,
    geometry: &NodeGeometry<ElementKey>,
    registered: &egui::Response,
    original_value: &crate::events::EventValue,
) {
    let outside = geometry.content_box.max + egui::vec2(1.0, 1.0);
    let (_, outside_response) = paint_control(
        context,
        document,
        forms,
        source,
        geometry,
        egui::RawInput {
            events: vec![egui::Event::PointerMoved(outside)],
            ..egui::RawInput::default()
        },
    );
    assert!(!outside_response.hovered());
    assert!(!outside_response.contains_pointer());
    assert!(!registered.interact_rect.contains(outside));
    for pressed in [true, false] {
        let (_, response) = paint_control(
            context,
            document,
            forms,
            source,
            geometry,
            egui::RawInput {
                events: vec![egui::Event::PointerButton {
                    pos: outside,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::default(),
                }],
                ..egui::RawInput::default()
            },
        );
        assert!(!response.clicked());
        assert!(!response.hovered());
        assert!(!response.is_pointer_button_down_on());
        assert_eq!(&forms.current_value(source).unwrap(), original_value);
    }
}

fn text(input: Input) -> Element {
    Element::Input {
        input,
        name: Some("accessible name, not allocated child text".into()),
        autofocus: None,
    }
}

#[test]
fn text_password_and_multiline_tiny_and_calculated_boxes_confine_native_hit_and_paint() {
    for size in [
        egui::vec2(7.0, 5.0),
        egui::vec2(220.0, 20.0),
        egui::vec2(220.0, 40.0),
    ] {
        for element in [
            text(Input::Text {
                value: Some("é long β".into()),
                placeholder: None,
            }),
            text(Input::Password {
                value: Some("é long β".into()),
                placeholder: None,
            }),
            Element::Textarea {
                value: "é long β\nsecond line".into(),
                placeholder: None,
                name: None,
                rows: None,
                cols: None,
            },
        ] {
            assert_control_bounds(element, size, true);
        }
    }
}

#[test]
fn checkbox_ignores_egui_interact_minimum_and_does_not_allocate_label_text() {
    for size in [
        egui::vec2(5.0, 5.0),
        egui::vec2(14.0, 14.0),
        egui::vec2(30.0, 20.0),
    ] {
        assert_control_bounds(
            text(Input::Checkbox {
                checked: Some(true),
            }),
            size,
            false,
        );
    }
}
