//! Editor shaping tests exercise the actual painted galley and native cursor memory.
use super::*;
use crate::{document::ViewOrigin, font_metrics::EguiFontMetrics};
use hyperchad_renderer::View;

const BUFFER: &str = "é  \t\nA\u{00a0}β";

fn fixture(
    family: &str,
    weight: FontWeight,
    password: bool,
    child_weight: Option<FontWeight>,
) -> (Document, FormsAdapter, ElementStamp) {
    let mut document = Document::new();
    let input = if password {
        Input::Password {
            value: Some(BUFFER.into()),
            placeholder: None,
        }
    } else {
        Input::Text {
            value: Some(BUFFER.into()),
            placeholder: None,
        }
    };
    document
        .apply_view(
            View {
                primary: Some(Container {
                    id: 1,
                    font_family: Some(vec![family.into()]),
                    font_weight: Some(weight),
                    font_size: Some(hyperchad_transformer::Number::Real(24.0)),
                    white_space: Some(WhiteSpace::Normal),
                    children: vec![Container {
                        id: 2,
                        font_weight: child_weight,
                        element: Element::Input {
                            input,
                            name: Some("field".into()),
                            autofocus: None,
                        },
                        ..Container::default()
                    }],
                    ..Container::default()
                }),
                ..View::default()
            },
            ViewOrigin::Navigation,
        )
        .unwrap();
    let source = document.stamp(document.numeric_matches(2)[0]).unwrap();
    let mut forms = FormsAdapter::new();
    forms.reconcile(&document, &[]).unwrap();
    (document, forms, source)
}

fn geometry(source: ElementStamp) -> NodeGeometry<ElementKey> {
    let rect = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(600.0, 80.0));
    NodeGeometry {
        key: source.key,
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
            scale: 1.25,
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
    document: &Document,
    forms: &mut FormsAdapter,
    source: ElementStamp,
) -> (egui::FullOutput, Result<(), String>) {
    paint_input(context, document, forms, source, egui::RawInput::default())
}

fn paint_input(
    context: &egui::Context,
    document: &Document,
    forms: &mut FormsAdapter,
    source: ElementStamp,
    input: egui::RawInput,
) -> (egui::FullOutput, Result<(), String>) {
    let mut result = Ok(());
    let output = context.run(input, |context| {
        egui::CentralPanel::default().show(context, |ui| {
            let authored = document.get(source.key).unwrap();
            let mut calculated = authored.clone();
            calculated.calculated_font_size = Some(24.0);
            result = forms
                .paint_node(ui, source, authored, &calculated, &geometry(source))
                .map(|_| ());
        });
    });
    (output, result)
}

fn find_galley(shape: &egui::Shape, displayed: &str) -> Option<Arc<egui::Galley>> {
    match shape {
        egui::Shape::Text(text) if text.galley.job.text == displayed => {
            Some(Arc::clone(&text.galley))
        }
        egui::Shape::Vec(shapes) => shapes
            .iter()
            .find_map(|shape| find_galley(shape, displayed)),
        _ => None,
    }
}

fn assert_parity(context: &egui::Context, actual: &egui::Galley, family: &str, weight: u16) {
    let families = [family.to_owned()];
    let mut expected = None;
    let _ = context.run(egui::RawInput::default(), |_| {
        expected = Some(
            EguiFontMetrics::new(context.clone())
                .layout_request(&TextMeasureRequest {
                    text: BUFFER,
                    family: TextFontFamily::Authored(&families),
                    weight: TextFontWeight::Absolute(weight),
                    size: 30.0,
                    wrap_width: f32::INFINITY,
                    whitespace: Some(WhiteSpace::Normal),
                })
                .unwrap(),
        );
    });
    let expected = expected.unwrap();
    assert_eq!(
        actual.job.sections[0].format.font_id,
        expected.job.sections[0].format.font_id
    );
    assert!((actual.size().x - expected.size().x).abs() < 0.01);
    assert!((actual.size().y - expected.size().y).abs() < 0.01);
    assert_eq!(
        actual
            .rows
            .iter()
            .map(|row| row.glyphs.len())
            .sum::<usize>(),
        BUFFER.chars().count()
    );
}

fn assert_native_editor(
    context: &egui::Context,
    document: &Document,
    forms: &mut FormsAdapter,
    source: ElementStamp,
    family: &str,
    weight: u16,
) {
    forms.focus(context, source).unwrap();
    forms.select_all(context, source).unwrap();
    let (output, result) = paint(context, document, forms, source);
    result.unwrap();
    let display = editor_display(BUFFER, WhiteSpace::Normal, false);
    let galley = output
        .shapes
        .iter()
        .find_map(|shape| find_galley(&shape.shape, &display))
        .expect("actual TextEdit galley");
    let id = forms.focus_id(source).unwrap();
    let selection = egui::text_edit::TextEditState::load(context, id)
        .unwrap()
        .cursor
        .char_range()
        .unwrap();
    assert_eq!(
        selection.primary.index.max(selection.secondary.index),
        BUFFER.chars().count()
    );
    assert_eq!(selection.primary.index.min(selection.secondary.index), 0);
    assert_eq!(context.memory(egui::Memory::focused), Some(id));
    assert!(
        matches!(forms.current_value(source).unwrap(), crate::events::EventValue::String(value) if value == BUFFER)
    );
    let visible: String = display
        .chars()
        .filter(|character| *character != '\u{2060}')
        .collect();
    assert_eq!(
        visible,
        crate::text_layout::normalize_whitespace(BUFFER, WhiteSpace::Normal)
    );
    assert_parity(context, &galley, family, weight);
    forms.focus(context, source).unwrap();
    // Insertion at the original buffer's Unicode offset must not use the
    // shorter whitespace-normalized string's offset.
    let mut editor = egui::text_edit::TextEditState::load(context, id).unwrap();
    editor
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(7),
        )));
    editor.store(context, id);
    let (_, result) = paint_input(
        context,
        document,
        forms,
        source,
        egui::RawInput {
            events: vec![egui::Event::Text("Z".into())],
            ..egui::RawInput::default()
        },
    );
    result.unwrap();
    assert!(
        matches!(forms.current_value(source).unwrap(), crate::events::EventValue::String(value) if value == "é  \t\nA\u{00a0}Zβ")
    );
    assert_eq!(
        egui::text_edit::TextEditState::load(context, id)
            .unwrap()
            .cursor
            .char_range()
            .unwrap()
            .primary
            .index,
        8
    );
}

#[test]
fn inherited_genuine_bundled_light_size_and_unicode_selection_match_measurement() {
    let (document, mut forms, source) = fixture("Ubuntu Light", FontWeight::Light, false, None);
    assert_native_editor(
        &egui::Context::default(),
        &document,
        &mut forms,
        source,
        "Ubuntu Light",
        300,
    );
}

#[test]
fn supplied_exact_mono_face_matches_measurement_and_native_selection() {
    let context = egui::Context::default();
    let bundled = egui::FontDefinitions::default();
    let mut builder = FontRegistry::builder();
    builder
        .register_face(
            "Supplied Hack",
            400,
            bundled.font_data["Hack"].as_ref().clone(),
        )
        .unwrap();
    let registry = Arc::new(builder.build().unwrap());
    registry.install(&context);
    let (document, mut forms, source) = fixture(
        "Supplied Hack",
        FontWeight::Light,
        false,
        Some(FontWeight::Bolder),
    );
    assert_native_editor(
        &context,
        &document,
        &mut forms,
        source,
        "Supplied Hack",
        400,
    );
}

#[test]
fn unsupported_editor_face_fails_before_native_paint_or_value_mutation() {
    let (document, mut forms, source) = fixture("Ubuntu Light", FontWeight::Bold, false, None);
    let context = egui::Context::default();
    let (_, result) = paint(&context, &document, &mut forms, source);
    assert!(result.is_err());
    assert!(
        egui::text_edit::TextEditState::load(&context, forms.focus_id(source).unwrap()).is_none()
    );
    assert!(
        matches!(forms.current_value(source).unwrap(), crate::events::EventValue::String(value) if value == BUFFER)
    );
}

#[test]
fn calculated_ancestor_path_overrides_authored_weight_without_double_relative_resolution() {
    let (document, mut forms, source) = fixture("Ubuntu Light", FontWeight::Bold, false, None);
    let mut parent = document.roots()[0].clone();
    parent.font_weight = Some(FontWeight::Light);
    let mut calculated = document.get(source.key).unwrap().clone();
    calculated.calculated_font_size = Some(24.0);
    forms
        .set_calculated_text_path(source, &[&parent, &calculated])
        .unwrap();
    assert_native_editor(
        &egui::Context::default(),
        &document,
        &mut forms,
        source,
        "Ubuntu Light",
        300,
    );
    let stale = ElementStamp {
        generation: source.generation + 1,
        ..source
    };
    assert!(
        forms
            .set_calculated_text_path(stale, &[&parent, &calculated])
            .is_err()
    );
    assert!(forms.set_calculated_text_path(source, &[]).is_err());
}

#[test]
fn default_editor_policy_keeps_preserved_unicode_offsets_and_authored_size_requires_calculation() {
    let (document, mut forms, source) = fixture("Ubuntu Light", FontWeight::Light, false, None);
    assert_eq!(
        editor_display(BUFFER, WhiteSpace::PreserveWrap, false),
        BUFFER
    );
    let context = egui::Context::default();
    let mut result = Ok(());
    let _ = context.run(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            let authored = document.get(source.key).unwrap();
            result = forms
                .paint_node(ui, source, authored, authored, &geometry(source))
                .map(|_| ());
        });
    });
    assert!(result.is_err());
}

#[test]
fn custom_password_layouter_masks_every_unicode_character_without_exposing_buffer() {
    let (document, mut forms, source) = fixture("Ubuntu Light", FontWeight::Light, true, None);
    let context = egui::Context::default();
    forms.select_all(&context, source).unwrap();
    let (output, result) = paint(&context, &document, &mut forms, source);
    result.unwrap();
    let masked = editor_display(BUFFER, WhiteSpace::Normal, true);
    assert_eq!(masked.chars().count(), BUFFER.chars().count());
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| find_galley(&shape.shape, &masked).is_some())
    );
    assert!(!format!("{:?}", output.platform_output.events).contains(BUFFER));
}
