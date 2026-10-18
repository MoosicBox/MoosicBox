//! Native response and paint ownership regression tests (no window or renderer).
use super::*;
use crate::{document::ViewOrigin, geometry::GeometryTransform};
use egui::{Rect, pos2, vec2};

#[derive(Default)]
struct Adapter(usize);
impl WidgetAdapter for Adapter {
    fn reconcile(&mut self, _: &Document, _: &[MutationSummary]) -> Result<(), String> {
        Ok(())
    }
    fn paint_node(
        &mut self,
        ui: &mut egui::Ui,
        stamp: ElementStamp,
        _: &Container,
        _: &Container,
        geometry: &NodeGeometry<ElementKey>,
    ) -> Result<NodeOutput, String> {
        self.0 += 1;
        Ok(NodeOutput {
            response: ui.interact(
                geometry.content_box,
                egui::Id::new(stamp.key),
                egui::Sense::click(),
            ),
            value: Some("adapter-value".to_owned()),
        })
    }
}

fn document(node: Container) -> (Document, ElementStamp) {
    let mut document = Document::new();
    document
        .apply_view(
            hyperchad_renderer::View {
                primary: Some(node),
                ..Default::default()
            },
            ViewOrigin::Navigation,
        )
        .unwrap();
    let stamp = document.stamp(document.root_keys()[0]).unwrap();
    (document, stamp)
}

fn geometry(stamp: ElementStamp) -> NodeGeometry<ElementKey> {
    let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(120.0, 40.0));
    NodeGeometry {
        key: stamp.key,
        container_id: 0,
        parent: None,
        containing_block: None,
        margin_box: rect,
        padding_box: rect,
        content_box: rect,
        border_box: rect,
        paint_clip: rect,
        descendant_clip: rect,
        transform: GeometryTransform {
            origin: rect.min,
            scale: 1.0,
        },
        scroll_extent: Rect::from_min_size(pos2(0.0, 0.0), rect.size()),
        scroll_max: egui::Vec2::ZERO,
        scroll_offset: egui::Vec2::ZERO,
        visible: true,
        fixed: false,
    }
}

#[test]
fn button_does_not_paint_descendant_text_or_allocate_child_layout() {
    let authored = Container {
        element: Element::Button { r#type: None },
        children: vec![Container {
            element: Element::Text {
                value: "one label".to_owned(),
            },
            ..Default::default()
        }],
        ..Default::default()
    };
    let (document, stamp) = document(authored.clone());
    let context = egui::Context::default();
    let mut painter =
        WidgetPainter::new(Adapter::default(), Adapter::default(), Adapter::default());
    painter.reconcile(&document, &[]).unwrap();
    let mut text_shapes = 0;
    let _ = context.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let output = painter
                .paint_node(ui, stamp, &authored, &authored, &geometry(stamp))
                .unwrap()
                .unwrap();
            assert_eq!(output.response.rect, geometry(stamp).border_box);
            assert_eq!(output.value, None);
        });
    });
    // A structural button has no native label widget. Descendant traversal is
    // the only code that may emit its text, verified below using the shape list.
    let output = context.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            painter
                .paint_node(ui, stamp, &authored, &authored, &geometry(stamp))
                .unwrap();
        });
    });
    for shape in output.shapes {
        if matches!(shape.shape, egui::Shape::Text(_)) {
            text_shapes += 1;
        }
    }
    assert_eq!(text_shapes, 0);
    assert_eq!(painter.forms.0, 0);
}

#[test]
fn textarea_delegates_exactly_once_and_preserves_changed_value() {
    let authored = Container {
        element: Element::Textarea {
            value: "native".to_owned(),
            placeholder: None,
            name: None,
            rows: None,
            cols: None,
        },
        ..Default::default()
    };
    let (document, stamp) = document(authored.clone());
    let mut painter =
        WidgetPainter::new(Adapter::default(), Adapter::default(), Adapter::default());
    painter.reconcile(&document, &[]).unwrap();
    let context = egui::Context::default();
    let _ = context.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let result = painter
                .paint_node(ui, stamp, &authored, &authored, &geometry(stamp))
                .unwrap()
                .unwrap();
            assert_eq!(result.value.as_deref(), Some("adapter-value"));
        });
    });
    assert_eq!(painter.forms.0, 1);
    assert_eq!(painter.resources.0, 0);
    assert_eq!(painter.canvas.0, 0);
}

#[test]
fn custom_link_has_native_role_name_and_clipped_geometry() {
    let authored = Container {
        element: Element::Anchor {
            target: None,
            href: Some("/album".to_owned()),
        },
        children: vec![Container {
            element: Element::Text {
                value: "Album".to_owned(),
            },
            ..Default::default()
        }],
        ..Default::default()
    };
    let (document, stamp) = document(authored.clone());
    let context = egui::Context::default();
    context.enable_accesskit();
    let mut painter =
        WidgetPainter::new(Adapter::default(), Adapter::default(), Adapter::default());
    painter.reconcile(&document, &[]).unwrap();
    let mut projected = geometry(stamp);
    projected.paint_clip = Rect::from_min_max(pos2(30.0, 25.0), pos2(100.0, 55.0));
    let output = context.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            painter
                .paint_node(ui, stamp, &authored, &authored, &projected)
                .unwrap();
        });
    });
    let update = output
        .platform_output
        .accesskit_update
        .expect("native accessibility update");
    let node = update
        .nodes
        .iter()
        .map(|(_, node)| node)
        .find(|node| node.role() == egui::accesskit::Role::Link)
        .expect("link semantic node");
    assert_eq!(node.label(), Some("Album"));
    assert_eq!(
        node.bounds(),
        Some(egui::accesskit::Rect {
            x0: 30.0,
            y0: 25.0,
            x1: 100.0,
            y1: 55.0
        })
    );
}

#[test]
fn frame_trait_forwards_disclosure_policy_and_suppresses_native_select_text() {
    let authored = Container {
        element: Element::Select {
            name: None,
            selected: None,
            multiple: None,
            disabled: None,
            autofocus: None,
        },
        children: vec![Container {
            element: Element::Option {
                value: Some("x".to_owned()),
                disabled: None,
            },
            children: vec![Container {
                element: Element::Text {
                    value: "Choice".to_owned(),
                },
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let (document, stamp) = document(authored);
    let option = document.children(stamp.key)[0];
    let text = document.children(option)[0];
    let mut painter =
        WidgetPainter::new(Adapter::default(), Adapter::default(), Adapter::default());
    let frame_painter: &mut dyn FramePainter = &mut painter;
    frame_painter.reconcile(&document, &[]).unwrap();
    let mut roots = document.roots().to_vec();
    frame_painter.prepare_layout(&document, &mut roots).unwrap();
    assert!(frame_painter.allows_node(stamp.key));
    assert!(!frame_painter.allows_node(option));
    assert!(!frame_painter.allows_node(text));
}

#[test]
fn closed_details_paints_native_fallback_with_expanded_semantics() {
    let authored = Container {
        element: Element::Details { open: None },
        ..Default::default()
    };
    let (document, stamp) = document(authored.clone());
    let context = egui::Context::default();
    context.enable_accesskit();
    let mut painter =
        WidgetPainter::new(Adapter::default(), Adapter::default(), Adapter::default());
    painter.reconcile(&document, &[]).unwrap();
    let output = context.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            assert!(
                painter
                    .paint_node(ui, stamp, &authored, &authored, &geometry(stamp))
                    .unwrap()
                    .is_some()
            );
        });
    });
    let update = output.platform_output.accesskit_update.unwrap();
    let node = update
        .nodes
        .iter()
        .map(|(_, node)| node)
        .find(|node| node.role() == egui::accesskit::Role::Button)
        .unwrap();
    assert_eq!(node.label(), Some("Details"));
    assert_eq!(node.is_expanded(), Some(false));
}

#[test]
fn raw_source_is_literal_native_text_without_angle_or_entity_heuristics() {
    for value in ["<strong>not HTML</strong>", "a < b &amp; c > d"] {
        let authored = Container {
            element: Element::Raw {
                value: value.to_owned(),
            },
            ..Default::default()
        };
        let (document, stamp) = document(authored.clone());
        let context = egui::Context::default();
        let mut painter =
            WidgetPainter::new(Adapter::default(), Adapter::default(), Adapter::default());
        painter.reconcile(&document, &[]).unwrap();
        let mut calculated = authored.clone();
        calculated.calculated_text_wrap_width = Some(geometry(stamp).content_box.width());
        let output = context.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(
                    painter
                        .paint_node(ui, stamp, &authored, &calculated, &geometry(stamp))
                        .unwrap()
                        .is_some()
                );
            });
        });
        assert!(output.shapes.iter().any(
            |shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == value)
        ));
    }
}

#[test]
fn production_html_raw_labels_emit_visible_white_glyph_meshes() {
    let parsed = Container::try_from("<div><h1>Heading</h1><span>Name</span><span>Enabled</span><button>Submit</button><a href='/next'>Continue</a></div>").unwrap();
    let (document, _) = document(parsed);
    let raw_keys: Vec<_> = document.keys().into_iter().filter(|key| matches!(&document.get(*key).unwrap().element, Element::Raw { value } if !value.trim().is_empty())).collect();
    assert_eq!(
        raw_keys.len(),
        5,
        "fixture must exercise real production parser Raw nodes"
    );
    let context = egui::Context::default();
    let mut painter =
        WidgetPainter::new(Adapter::default(), Adapter::default(), Adapter::default());
    painter.reconcile(&document, &[]).unwrap();
    let output = context.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(640.0, 480.0))),
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                for (index, &key) in raw_keys.iter().enumerate() {
                    let stamp = document.stamp(key).unwrap();
                    let authored = document.get(key).unwrap();
                    let mut calculated = authored.clone();
                    calculated.color = Some(hyperchad_color::Color {
                        r: 255,
                        g: 255,
                        b: 255,
                        a: None,
                    });
                    calculated.calculated_text_wrap_width =
                        Some(geometry(stamp).content_box.width());
                    calculated.calculated_font_size = Some(20.0);
                    let mut projected = geometry(stamp);
                    let offset = egui::vec2(0.0, f32::from(u16::try_from(index).unwrap()) * 45.0);
                    projected.content_box = projected.content_box.translate(offset);
                    projected.border_box = projected.content_box;
                    projected.padding_box = projected.content_box;
                    projected.paint_clip = projected.content_box;
                    painter
                        .paint_node(ui, stamp, authored, &calculated, &projected)
                        .unwrap();
                }
            });
        },
    );
    let labels: Vec<_> = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text),
            _ => None,
        })
        .collect();
    assert_eq!(labels.len(), 5);
    for label in labels {
        assert!(!label.galley.is_empty());
        assert_eq!(
            label.galley.job.sections[0].format.color,
            egui::Color32::WHITE
        );
        assert!(label.galley.mesh_bounds.is_positive());
    }
    let primitives = context.tessellate(output.shapes, output.pixels_per_point);
    let visible_glyph_vertices = primitives
        .iter()
        .filter_map(|primitive| match &primitive.primitive {
            egui::epaint::Primitive::Mesh(mesh) => Some((primitive.clip_rect, mesh)),
            egui::epaint::Primitive::Callback(_) => None,
        })
        .flat_map(|(clip, mesh)| {
            mesh.vertices.iter().filter(move |vertex| {
                vertex.color == egui::Color32::WHITE
                    && vertex.uv != egui::epaint::WHITE_UV
                    && clip.contains(vertex.pos)
            })
        })
        .count();
    assert!(
        visible_glyph_vertices > 20,
        "actual clipped glyph meshes must contain opaque white vertices, not just semantic nodes"
    );
}
