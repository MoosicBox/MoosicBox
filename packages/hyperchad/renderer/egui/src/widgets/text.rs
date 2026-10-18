//! Text galleys use calculated boxes, not native widget layout.

use crate::text_layout::{FontRegistry, request_job};
use eframe::egui::{self, Align, Color32, Stroke};
use hyperchad_transformer::layout::font::{TextFontFamily, TextFontWeight, TextMeasureRequest};
use hyperchad_transformer::{
    Container,
    models::{TextAlign, TextDecorationLine, TextDecorationStyle, TextOverflow, WhiteSpace},
};

use super::paint::color;

/// Inherited text values only, never a second clone of the calculated DOM.
#[derive(Clone, Default)]
pub(super) struct TextStyle {
    size: Option<f32>,
    color: Option<hyperchad_color::Color>,
    font_family: Option<Vec<String>>,
    font_weights: Vec<hyperchad_transformer::models::FontWeight>,
    text_decoration: Option<hyperchad_transformer::TextDecoration>,
    text_align: Option<TextAlign>,
    white_space: Option<WhiteSpace>,
}

impl TextStyle {
    pub(super) fn resolve(node: &Container, parent: Option<&Self>) -> Self {
        let inherited = parent.cloned().unwrap_or_default();
        Self {
            size: node.calculated_font_size.or(inherited.size),
            color: node.color.or(inherited.color),
            font_family: node.font_family.clone().or(inherited.font_family),
            font_weights: {
                let mut weights = inherited.font_weights;
                if let Some(weight) = node.font_weight {
                    weights.push(weight);
                }
                weights
            },
            text_decoration: node.text_decoration.clone().or(inherited.text_decoration),
            text_align: node.text_align.or(inherited.text_align),
            white_space: node.white_space.or(inherited.white_space),
        }
    }
}

fn resolved_weight(style: &TextStyle, baseline: Option<u16>) -> Result<TextFontWeight, String> {
    style.font_weights.iter().try_fold(
        TextFontWeight::CompatibilityDefault,
        |inherited, authored| {
            hyperchad_transformer::layout::font::resolve_text_weight(*authored, inherited, baseline)
                .map_err(|error| error.to_string())
        },
    )
}

pub(super) fn paint_text(
    painter: &egui::Painter,
    node: &Container,
    style: &TextStyle,
    value: &str,
    rect: egui::Rect,
    scale: f32,
) -> Result<(), String> {
    let foreground = style.color.map_or(Color32::BLACK, color);
    let size = style.size.unwrap_or(14.0) * scale;
    if !size.is_finite() || size <= 0.0 {
        return Err("native text requires a finite positive calculated font size".to_owned());
    }
    let logical_wrap_width = node.calculated_text_wrap_width.ok_or_else(|| {
        "native text is missing authoritative calculated_text_wrap_width".to_owned()
    })?;
    if logical_wrap_width.is_nan() || logical_wrap_width < 0.0 {
        return Err(
            "native text calculated wrap width must be nonnegative or positive infinity".to_owned(),
        );
    }
    let wrap_width = logical_wrap_width * scale;
    if logical_wrap_width.is_finite() && !wrap_width.is_finite() {
        return Err("native text projected wrap width overflow".to_owned());
    }
    let registry = FontRegistry::for_context(painter.ctx());
    let mut job = request_job(
        &registry,
        &TextMeasureRequest {
            text: value,
            family: style.font_family.as_deref().map_or(
                TextFontFamily::CompatibilityDefault,
                TextFontFamily::Authored,
            ),
            weight: resolved_weight(style, registry.compatibility_weight())?,
            size,
            wrap_width,
            whitespace: style.white_space,
        },
    )
    .map_err(|error| error.to_string())?;
    for section in &mut job.sections {
        section.format.color = foreground;
    }
    let decorations = text_decorations(style, size, rect, scale, foreground)?;
    job.halign = match style.text_align {
        None | Some(TextAlign::Start | TextAlign::Justify) => Align::LEFT,
        Some(TextAlign::Center) => Align::Center,
        Some(TextAlign::End) => Align::RIGHT,
    };
    job.justify = matches!(style.text_align, Some(TextAlign::Justify));
    if matches!(node.text_overflow, Some(TextOverflow::Ellipsis)) {
        job.wrap.max_width = rect.width().max(0.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        job.wrap.overflow_character = Some('…');
    }
    let galley = painter.layout_job(job);
    // Galley row positions already contain horizontal alignment offsets.
    let origin = egui::pos2(
        match style.text_align {
            Some(TextAlign::Center) => rect.center().x,
            Some(TextAlign::End) => rect.right(),
            None | Some(TextAlign::Start | TextAlign::Justify) => rect.left(),
        },
        rect.top(),
    );
    for (line, decoration_style, stroke) in decorations {
        for row in &galley.rows {
            let row_rect = row.rect().translate(origin.to_vec2());
            let y = match line {
                TextDecorationLine::Underline => row_rect.bottom() - stroke.width,
                TextDecorationLine::LineThrough => row_rect.center().y,
                TextDecorationLine::Overline => row_rect.top(),
                TextDecorationLine::None | TextDecorationLine::Inherit => continue,
            };
            decoration_line(
                painter,
                row_rect.left(),
                row_rect.right(),
                y,
                decoration_style,
                stroke,
            );
        }
    }
    painter.galley(origin, galley, foreground);
    Ok(())
}

type Decoration = (TextDecorationLine, TextDecorationStyle, Stroke);

fn text_decorations(
    style: &TextStyle,
    size: f32,
    rect: egui::Rect,
    scale: f32,
    foreground: Color32,
) -> Result<Vec<Decoration>, String> {
    let mut decorations = Vec::new();
    if let Some(decoration) = &style.text_decoration {
        let decoration_style = decoration.style.unwrap_or(TextDecorationStyle::Solid);
        let thickness = decoration.thickness.as_ref().map_or(1.0, |value| {
            value.calc(size / scale, rect.width() / scale, rect.height() / scale)
        }) * scale;
        if !thickness.is_finite() {
            return Err("native text decoration requires finite thickness".to_owned());
        }
        let stroke = Stroke::new(
            thickness.max(0.0),
            decoration.color.map_or(foreground, color),
        );
        for line in &decoration.line {
            match line {
                TextDecorationLine::Underline
                | TextDecorationLine::LineThrough
                | TextDecorationLine::Overline => {
                    decorations.push((*line, decoration_style, stroke));
                }
                TextDecorationLine::Inherit | TextDecorationLine::None => {}
            }
        }
    }
    Ok(decorations)
}

fn decoration_line(
    painter: &egui::Painter,
    left: f32,
    right: f32,
    y: f32,
    style: TextDecorationStyle,
    stroke: Stroke,
) {
    if stroke.width <= 0.0 || right <= left {
        return;
    }
    let point = |x, y| egui::pos2(x, y);
    match style {
        TextDecorationStyle::Inherit | TextDecorationStyle::Solid => {
            painter.line_segment([point(left, y), point(right, y)], stroke);
        }
        TextDecorationStyle::Double => {
            for offset in [-stroke.width, stroke.width] {
                painter.line_segment([point(left, y + offset), point(right, y + offset)], stroke);
            }
        }
        TextDecorationStyle::Dashed | TextDecorationStyle::Dotted => {
            let length = if style == TextDecorationStyle::Dotted {
                stroke.width
            } else {
                stroke.width * 3.0
            };
            let stride = stroke
                .width
                .mul_add(2.0, length)
                .max(0.5)
                .max((right - left) / 4095.0);
            for index in 0_u16..4096 {
                let x = f32::from(index).mul_add(stride, left);
                if x >= right {
                    break;
                }
                if style == TextDecorationStyle::Dotted {
                    painter.circle_filled(
                        point(stroke.width.mul_add(0.5, x), y),
                        stroke.width * 0.5,
                        stroke.color,
                    );
                } else {
                    painter.line_segment([point(x, y), point((x + length).min(right), y)], stroke);
                }
            }
        }
        TextDecorationStyle::Wavy => {
            let step = stroke.width.max(1.0).max((right - left) / 4095.0);
            let mut points = Vec::new();
            for index in 0_u16..4096 {
                let x = f32::from(index).mul_add(step, left);
                if x >= right {
                    break;
                }
                points.push(point(x, ((x - left) / (step * 2.0)).sin().mul_add(step, y)));
            }
            points.push(point(
                right,
                ((right - left) / (step * 2.0)).sin().mul_add(step, y),
            ));
            painter.add(egui::Shape::line(points, stroke));
        }
    }
}

#[cfg(test)]
fn normalize_whitespace(value: &str, white_space: Option<WhiteSpace>) -> String {
    crate::text_layout::normalize_whitespace(value, white_space.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_measured_text(node: &Container, zoom: f32) -> egui::Rect {
        use crate::geometry::{GeometrySnapshot, GeometryViewport};
        use std::collections::BTreeMap;
        let root = Container {
            id: 1,
            calculated_x: Some(0.0),
            calculated_y: Some(0.0),
            calculated_width: Some(200.0),
            calculated_height: Some(300.0),
            children: vec![node.clone()],
            ..Default::default()
        };
        GeometrySnapshot::build(
            &root,
            GeometryViewport {
                rect: egui::Rect::from_min_size(egui::pos2(20.0, 106.76), egui::vec2(400.0, 500.0)),
                zoom,
            },
            1,
            &BTreeMap::new(),
            |node| Some(node.id),
        )
        .unwrap()
        .node(2)
        .unwrap()
        .content_box
    }

    #[test]
    fn measured_projected_text_keeps_authoritative_line_breaks() {
        use crate::font_metrics::EguiFontMetrics;
        use hyperchad_transformer::layout::font::FontMetrics;

        for dpi in [1.0, 1.25, 1.5, 2.0] {
            for zoom in [1.0, 1.25] {
                for (value, constraint, expected_multiline) in [
                    ("My Collection", f32::INFINITY, false),
                    ("Home", f32::INFINITY, false),
                    ("My Collection has several albums", 65.0, true),
                ] {
                    let context = egui::Context::default();
                    context.set_pixels_per_point(dpi);
                    let mut expected_galley = None;
                    let mut projected_width = 0.0;
                    let output = context.run(egui::RawInput::default(), |ctx| {
                        let metrics = EguiFontMetrics::new(ctx.clone());
                        let request = TextMeasureRequest {
                            text: value,
                            family: TextFontFamily::CompatibilityDefault,
                            weight: TextFontWeight::CompatibilityDefault,
                            size: 19.6,
                            wrap_width: constraint,
                            whitespace: None,
                        };
                        let measured = metrics.measure_text(&request).unwrap();
                        assert_eq!(measured.rows.len() > 1, expected_multiline);
                        let child = Container {
                            id: 2,
                            element: hyperchad_transformer::Element::Text {
                                value: value.to_owned(),
                            },
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_width: Some(measured.width()),
                            calculated_height: Some(measured.height()),
                            calculated_font_size: Some(request.size),
                            calculated_text_wrap_width: Some(constraint),
                            ..Default::default()
                        };
                        let rect = project_measured_text(&child, zoom);
                        projected_width = rect.width();
                        // Compare against the same authoritative job at paint scale,
                        // not a fresh wrap constraint derived from projected bounds.
                        let projected_request = TextMeasureRequest {
                            size: request.size * zoom,
                            wrap_width: constraint * zoom,
                            ..request
                        };
                        let expected = metrics.layout_request(&projected_request).unwrap();
                        assert_eq!(expected.rows.len(), measured.rows.len());
                        expected_galley = Some(expected);
                        let node = &child;
                        paint_text(
                            &ctx.layer_painter(egui::LayerId::background()),
                            node,
                            &TextStyle::resolve(node, None),
                            value,
                            rect,
                            zoom,
                        )
                        .unwrap();
                    });
                    let painted = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) => Some(&text.galley),
                            _ => None,
                        })
                        .expect("actual projected text paint");
                    let expected = expected_galley.unwrap();
                    assert_eq!(
                        painted.rows.len(),
                        expected.rows.len(),
                        "{value}, dpi={dpi}, zoom={zoom}, projected_width={projected_width}"
                    );
                    // Egui normalizes finite wrap inputs to integer points before
                    // caching, independently of round_output_to_gui. Compare the
                    // actual measured job, not its pre-normalization input.
                    assert_eq!(painted.job.wrap.max_width, expected.job.wrap.max_width);
                    for (painted_row, measured_row) in painted.rows.iter().zip(&expected.rows) {
                        assert_eq!(painted_row.size, measured_row.size);
                        assert_eq!(painted_row.pos, measured_row.pos);
                    }
                }
            }
        }
    }

    #[test]
    fn c18_collection_intrinsic_width_is_not_a_new_wrap_constraint() {
        use crate::font_metrics::EguiFontMetrics;
        let context = egui::Context::default();
        let output = context.run(egui::RawInput::default(), |ctx| {
            let metrics = EguiFontMetrics::new(ctx.clone());
            let measured = metrics.layout_text("My Collection", 19.6, f32::INFINITY);
            assert_eq!(measured.rows.len(), 1);
            // Native C18's calculated text is projected at this nonzero origin;
            // the resulting f32 rectangle width is not the measurement input.
            let node = Container {
                id: 2,
                calculated_x: Some(0.0),
                calculated_y: Some(0.0),
                calculated_width: Some(120.380_005),
                calculated_height: Some(23.0),
                calculated_font_size: Some(19.6),
                calculated_text_wrap_width: Some(f32::INFINITY),
                ..Default::default()
            };
            let rect = project_measured_text(&node, 1.0);
            assert_eq!(rect.min, egui::pos2(20.0, 106.76));
            assert!((rect.width() - 120.380_005).abs() < 0.000_02);
            paint_text(
                &ctx.layer_painter(egui::LayerId::background()),
                &node,
                &TextStyle::resolve(&node, None),
                "My Collection",
                rect,
                1.0,
            )
            .unwrap();
        });
        let painted = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(&text.galley),
                _ => None,
            })
            .unwrap();
        assert_eq!(painted.rows.len(), 1);
        assert_eq!(painted.job.wrap.max_width, f32::INFINITY);
    }

    #[test]
    fn missing_or_invalid_measurement_constraint_is_diagnostic() {
        let context = egui::Context::default();
        let _ = context.run(egui::RawInput::default(), |ctx| {
            for width in [None, Some(f32::NAN), Some(-1.0), Some(f32::NEG_INFINITY)] {
                let node = Container {
                    calculated_font_size: Some(19.6),
                    calculated_text_wrap_width: width,
                    ..Default::default()
                };
                assert!(
                    paint_text(
                        &ctx.layer_painter(egui::LayerId::background()),
                        &node,
                        &TextStyle::resolve(&node, None),
                        "My Collection",
                        egui::Rect::from_min_size(
                            egui::pos2(20.0, 106.76),
                            egui::vec2(120.380_005, 23.0)
                        ),
                        1.0,
                    )
                    .is_err()
                );
            }
        });
    }

    #[test]
    fn fractional_dpi_width_measurement_matches_actual_painted_galley() {
        use crate::font_metrics::EguiFontMetrics;
        use hyperchad_transformer::layout::font::FontMetrics;

        let value = "Fractional widths wrap several words without rounding drift.";
        for dpi in [1.25, 1.5, 1.75] {
            for width in [73.25, 119.625, 183.375] {
                let context = egui::Context::default();
                context.set_pixels_per_point(dpi);
                let mut measured = None;
                let output = context.run(egui::RawInput::default(), |ctx| {
                    let metrics = EguiFontMetrics::new(ctx.clone());
                    measured = Some(
                        metrics
                            .measure_text(&TextMeasureRequest {
                                text: value,
                                family: TextFontFamily::CompatibilityDefault,
                                weight: TextFontWeight::CompatibilityDefault,
                                size: 17.375,
                                wrap_width: width,
                                whitespace: None,
                            })
                            .unwrap(),
                    );
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let node = Container {
                            calculated_font_size: Some(17.375),
                            calculated_text_wrap_width: Some(width),
                            ..Container::default()
                        };
                        let rect = egui::Rect::from_min_size(
                            egui::pos2(8.0, 8.0),
                            egui::vec2(width, 400.0),
                        );
                        paint_text(
                            ui.painter(),
                            &node,
                            &TextStyle::resolve(&node, None),
                            value,
                            rect,
                            1.0,
                        )
                        .unwrap();
                    });
                });
                let painted = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) => Some(&text.galley),
                        _ => None,
                    })
                    .expect("actual painted text galley");
                assert!(!painted.job.round_output_to_gui);
                let measured = measured.unwrap();
                assert_eq!(measured.rows.len(), painted.rows.len());
                for (index, (measured, painted_row)) in
                    measured.rows.iter().zip(&painted.rows).enumerate()
                {
                    let bottom = painted
                        .rows
                        .get(index + 1)
                        .map_or_else(|| painted.rect.bottom(), |row| row.pos.y);
                    assert!((measured.width - painted_row.size.x).abs() < 0.0001);
                    assert!((measured.height - (bottom - painted_row.pos.y)).abs() < 0.0001);
                }
                assert!((measured.height() - painted.rect.height()).abs() < 0.0001);
                assert!(painted.mesh_bounds.is_positive());
            }
        }
    }

    #[test]
    fn resolve_inherits_text_properties_without_copying_background() {
        let parent = Container {
            color: Some(hyperchad_color::Color {
                r: 1,
                g: 2,
                b: 3,
                a: None,
            }),
            calculated_font_size: Some(24.0),
            white_space: Some(WhiteSpace::PreserveWrap),
            ..Default::default()
        };
        let inherited = TextStyle::resolve(&parent, None);
        let child = TextStyle::resolve(&Container::default(), Some(&inherited));
        assert_eq!(child.size, Some(24.0));
        assert_eq!(child.color, parent.color);
        assert_eq!(child.white_space, Some(WhiteSpace::PreserveWrap));
    }

    #[test]
    fn relative_weight_uses_registry_baseline_through_shared_resolver() {
        let node = Container {
            font_weight: Some(hyperchad_transformer::models::FontWeight::Bolder),
            ..Default::default()
        };
        let style = TextStyle::resolve(&node, None);
        assert_eq!(
            resolved_weight(&style, Some(300)).unwrap(),
            TextFontWeight::Absolute(400)
        );
        assert_eq!(
            resolved_weight(&style, Some(400)).unwrap(),
            TextFontWeight::Absolute(700)
        );
        assert!(resolved_weight(&style, None).is_err());
    }

    #[test]
    fn inherited_explicit_regular_is_not_compatibility_default() {
        let parent = Container {
            font_weight: Some(hyperchad_transformer::models::FontWeight::Weight400),
            ..Default::default()
        };
        let inherited = TextStyle::resolve(&parent, None);
        let child = TextStyle::resolve(&Container::default(), Some(&inherited));
        assert_eq!(
            resolved_weight(&child, Some(300)).unwrap(),
            TextFontWeight::Absolute(400)
        );
        assert!(
            FontRegistry::compatibility()
                .resolve(None, Some(400), 14.0)
                .is_err()
        );
    }

    #[test]
    fn normal_whitespace_collapses_ascii_but_not_nbsp() {
        assert_eq!(
            normalize_whitespace("a \n\tb\u{00a0} c", None),
            "a b\u{00a0} c"
        );
    }

    #[test]
    fn preserved_whitespace_retains_line_breaks_and_indentation() {
        for mode in [WhiteSpace::Preserve, WhiteSpace::PreserveWrap] {
            assert_eq!(normalize_whitespace(" a\n  b", Some(mode)), " a\n  b");
        }
    }
}
