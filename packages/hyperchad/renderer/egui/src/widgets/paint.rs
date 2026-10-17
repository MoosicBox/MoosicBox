//! Screen-space structural paint primitives. No egui layout allocation occurs here.

use eframe::egui::{self, Color32, Pos2, Rect, Shape, Stroke, pos2};
use hyperchad_color::Color;
use hyperchad_transformer::Container;

/// Convert straight-alpha document color; opacity is applied by the painter once.
pub(super) fn color(value: Color) -> Color32 {
    Color32::from_rgba_unmultiplied(value.r, value.g, value.b, value.a.unwrap_or(255))
}

/// CSS-normalized independent corner radii in TL, TR, BR, BL order.
pub(super) fn radii(node: &Container, rect: Rect, scale: f32) -> [f32; 4] {
    normalize_radii(
        [
            node.calculated_border_top_left_radius.unwrap_or(0.0) * scale,
            node.calculated_border_top_right_radius.unwrap_or(0.0) * scale,
            node.calculated_border_bottom_right_radius.unwrap_or(0.0) * scale,
            node.calculated_border_bottom_left_radius.unwrap_or(0.0) * scale,
        ],
        rect.width(),
        rect.height(),
    )
}

fn normalize_radii(mut radii: [f32; 4], width: f32, height: f32) -> [f32; 4] {
    for radius in &mut radii {
        *radius = if radius.is_finite() {
            radius.max(0.0)
        } else {
            0.0
        };
    }
    let mut factor = 1.0_f32;
    for (length, sum) in [
        (width, radii[0] + radii[1]),
        (width, radii[3] + radii[2]),
        (height, radii[0] + radii[3]),
        (height, radii[1] + radii[2]),
    ] {
        if sum > 0.0 {
            factor = factor.min(length.max(0.0) / sum);
        }
    }
    radii.map(|radius| radius * factor)
}

/// Quarter-circle samples, including both tangent endpoints.
fn arc(center: Pos2, radius: f32, start: f32, end: f32) -> Vec<Pos2> {
    (0_u16..=12)
        .map(|step| {
            let angle = start + (end - start) * (f32::from(step) / 12.0);
            center + egui::vec2(angle.cos(), angle.sin()) * radius
        })
        .collect()
}

fn corners(rect: Rect, radii: [f32; 4]) -> [Vec<Pos2>; 4] {
    use std::f32::consts::{FRAC_PI_2, PI};
    [
        arc(
            pos2(rect.left() + radii[0], rect.top() + radii[0]),
            radii[0],
            PI,
            PI + FRAC_PI_2,
        ),
        arc(
            pos2(rect.right() - radii[1], rect.top() + radii[1]),
            radii[1],
            PI + FRAC_PI_2,
            PI * 2.0,
        ),
        arc(
            pos2(rect.right() - radii[2], rect.bottom() - radii[2]),
            radii[2],
            0.0,
            FRAC_PI_2,
        ),
        arc(
            pos2(rect.left() + radii[3], rect.bottom() - radii[3]),
            radii[3],
            FRAC_PI_2,
            PI,
        ),
    ]
}

/// Background and independent colored/weighted borders with rounded corners.
/// Border tessellation splits each corner between its two adjacent sides; there
/// are no full-rect strokes that would overwrite differently colored neighbors.
pub(super) fn box_paint(painter: &egui::Painter, node: &Container, rect: Rect, scale: f32) {
    if !rect.is_positive() {
        return;
    }
    let radii = radii(node, rect, scale);
    let outer = corners(rect, radii);
    if let Some(fill) = node.background {
        painter.add(Shape::convex_polygon(
            outer.iter().flatten().copied().collect(),
            color(fill),
            Stroke::NONE,
        ));
    }
    let borders = [
        node.calculated_border_top,
        node.calculated_border_right,
        node.calculated_border_bottom,
        node.calculated_border_left,
    ];
    let mut widths =
        borders.map(|border| border.map_or(0.0, |(_, width)| (width * scale).max(0.0)));
    for (first, second, length) in [(1, 3, rect.width()), (0, 2, rect.height())] {
        let total = widths[first] + widths[second];
        if total > length {
            let factor = length / total;
            widths[first] *= factor;
            widths[second] *= factor;
        }
    }
    let inner_rect = Rect::from_min_max(
        pos2(rect.left() + widths[3], rect.top() + widths[0]),
        pos2(rect.right() - widths[1], rect.bottom() - widths[2]),
    );
    // Elliptical inner radii retain the outer curve's center for unequal widths.
    let mut inner = corners(inner_rect, radii);
    for (index, points) in inner.iter_mut().enumerate() {
        let (horizontal, vertical) = match index {
            0 => (widths[3], widths[0]),
            1 => (widths[1], widths[0]),
            2 => (widths[1], widths[2]),
            3 => (widths[3], widths[2]),
            _ => unreachable!(),
        };
        let origin = match index {
            0 => pos2(rect.left() + radii[0], rect.top() + radii[0]),
            1 => pos2(rect.right() - radii[1], rect.top() + radii[1]),
            2 => pos2(rect.right() - radii[2], rect.bottom() - radii[2]),
            3 => pos2(rect.left() + radii[3], rect.bottom() - radii[3]),
            _ => unreachable!(),
        };
        for (point, outside) in points.iter_mut().zip(&outer[index]) {
            let delta = *outside - origin;
            let radius = radii[index];
            *point = if radius > 0.0 {
                origin
                    + egui::vec2(
                        delta.x * (radius - horizontal).max(0.0) / radius,
                        delta.y * (radius - vertical).max(0.0) / radius,
                    )
            } else {
                match index {
                    0 => inner_rect.left_top(),
                    1 => inner_rect.right_top(),
                    2 => inner_rect.right_bottom(),
                    3 => inner_rect.left_bottom(),
                    _ => unreachable!(),
                }
            };
            *point = inner_rect.clamp(*point);
        }
    }
    for (side, border) in borders.iter().enumerate() {
        let Some((border_color, width)) = border else {
            continue;
        };
        if *width <= 0.0 {
            continue;
        }
        let start = side;
        let end = (side + 1) % 4;
        let outside: Vec<_> = outer[start][6..]
            .iter()
            .chain(&outer[end][..=6])
            .copied()
            .collect();
        let inside: Vec<_> = inner[start][6..]
            .iter()
            .chain(&inner[end][..=6])
            .copied()
            .collect();
        let mut mesh = egui::Mesh::default();
        for (&outside, &inside) in outside.iter().zip(&inside) {
            mesh.colored_vertex(outside, color(*border_color));
            mesh.colored_vertex(inside, color(*border_color));
        }
        for index in 0..(outside.len() - 1) {
            let index = u32::try_from(index * 2).expect("bounded corner tessellation");
            mesh.add_triangle(index, index + 1, index + 2);
            mesh.add_triangle(index + 1, index + 3, index + 2);
        }
        painter.add(Shape::mesh(mesh));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_normalization_preserves_independence_and_ratios() {
        assert_eq!(
            normalize_radii([40.0, 20.0, 0.0, 10.0], 30.0, 100.0),
            [20.0, 10.0, 0.0, 5.0]
        );
        assert_eq!(
            normalize_radii([f32::NAN, -1.0, 5.0, 2.0], 100.0, 100.0),
            [0.0, 0.0, 5.0, 2.0]
        );
    }

    #[test]
    fn absent_alpha_is_opaque_and_explicit_alpha_is_preserved() {
        assert_eq!(
            color(Color {
                r: 20,
                g: 40,
                b: 60,
                a: None
            })
            .a(),
            255
        );
        assert_eq!(
            color(Color {
                r: 20,
                g: 40,
                b: 60,
                a: Some(64)
            })
            .a(),
            64
        );
    }
}
