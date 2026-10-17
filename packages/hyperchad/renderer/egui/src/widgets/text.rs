//! Text galleys use calculated boxes, not native widget layout.

use eframe::egui::{
    self, Align, Color32, FontFamily, FontId, Stroke,
    text::{LayoutJob, TextFormat},
};
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
    font_weight: Option<hyperchad_transformer::models::FontWeight>,
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
            font_weight: node.font_weight.or(inherited.font_weight),
            text_decoration: node.text_decoration.clone().or(inherited.text_decoration),
            text_align: node.text_align.or(inherited.text_align),
            white_space: node.white_space.or(inherited.white_space),
        }
    }
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
    let family = font_family(painter, style)?;
    let format = TextFormat {
        font_id: FontId::new(size, family),
        color: foreground,
        ..TextFormat::default()
    };
    let decorations = text_decorations(style, size, rect, scale, foreground)?;
    // Match EguiFontMetrics: logical row/galley bounds are not GUI-rounded.
    // This leaves native glyph pixel snapping enabled at fractional DPI.
    let mut job = LayoutJob {
        round_output_to_gui: false,
        ..LayoutJob::default()
    };
    let text = normalize_whitespace(value, style.white_space);
    job.append(&text, 0.0, format);
    job.halign = match style.text_align {
        None | Some(TextAlign::Start | TextAlign::Justify) => Align::LEFT,
        Some(TextAlign::Center) => Align::Center,
        Some(TextAlign::End) => Align::RIGHT,
    };
    job.justify = matches!(style.text_align, Some(TextAlign::Justify));
    job.wrap.max_width = if matches!(style.white_space, Some(WhiteSpace::Preserve)) {
        f32::INFINITY
    } else {
        rect.width().max(0.0)
    };
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

fn font_family(painter: &egui::Painter, style: &TextStyle) -> Result<FontFamily, String> {
    #[expect(
        clippy::redundant_closure_for_method_calls,
        reason = "FontsView::families method item is not lifetime-general enough for Context::fonts; b12-tests confirms FnOnce failure"
    )]
    let available = painter.ctx().fonts(|fonts| fonts.families());
    let mut family = FontFamily::Proportional;
    if let Some(families) = &style.font_family {
        for name in families {
            let candidate = if name.eq_ignore_ascii_case("monospace") {
                FontFamily::Monospace
            } else if name.eq_ignore_ascii_case("sans-serif") {
                FontFamily::Proportional
            } else {
                FontFamily::Name(name.clone().into())
            };
            if available.contains(&candidate) {
                family = candidate;
                break;
            }
        }
    }
    if let Some(weight) = style.font_weight {
        use hyperchad_transformer::models::FontWeight;
        if !matches!(weight, FontWeight::Normal | FontWeight::Weight400) {
            let base = match &family {
                FontFamily::Name(name) => name.as_ref(),
                FontFamily::Monospace => "monospace",
                FontFamily::Proportional => "sans-serif",
            };
            let weighted = [format!("{base}-{weight}"), weight.to_string()]
                .into_iter()
                .map(|name| FontFamily::Name(name.into()))
                .find(|candidate| available.contains(candidate));
            family = weighted.ok_or_else(|| {
                format!("native text font weight {weight} requires a registered font family")
            })?;
        }
    }
    Ok(family)
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

fn normalize_whitespace(value: &str, white_space: Option<WhiteSpace>) -> String {
    if matches!(
        white_space,
        Some(WhiteSpace::Preserve | WhiteSpace::PreserveWrap)
    ) {
        value.to_owned()
    } else {
        // Collapse HTML ASCII whitespace only; NBSP is meaningful content.
        let mut result = String::new();
        let mut previous_space = false;
        for character in value.chars() {
            let space = matches!(character, ' ' | '\t' | '\n' | '\r' | '\u{000c}');
            if !space || !previous_space {
                result.push(if space { ' ' } else { character });
            }
            previous_space = space;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    measured = Some(metrics.measure_text(value, 17.375, width));
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let node = Container {
                            calculated_font_size: Some(17.375),
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
