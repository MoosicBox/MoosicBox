//! Viewport visibility calculations and rendering modes.
//!
//! This module provides utilities for determining widget visibility within a viewport,
//! along with sub-modules for different viewport rendering strategies.
//!
//! * `immediate` - Immediate mode viewport rendering (enabled with `viewport-immediate` feature)
//! * `retained` - Retained mode viewport rendering (enabled with `viewport-retained` feature)

/// Immediate mode viewport rendering with per-frame visibility calculations.
///
/// See the module documentation for details.
#[cfg(feature = "viewport-immediate")]
pub mod immediate;

/// Retained mode viewport rendering with persistent widget positions.
///
/// See the module documentation for details.
#[cfg(feature = "viewport-retained")]
pub mod retained;

fn max_f32(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

/// Checks if a widget is visible within a viewport.
///
/// Calculates whether a rectangular widget intersects with a viewport's visible area
/// and returns both the visibility status and the distance from the viewport if not visible.
///
/// # Parameters
///
/// * `viewport_x` - X coordinate of the viewport's top-left corner
/// * `viewport_y` - Y coordinate of the viewport's top-left corner
/// * `viewport_w` - Width of the viewport
/// * `viewport_h` - Height of the viewport
/// * `widget_x` - X coordinate of the widget's top-left corner
/// * `widget_y` - Y coordinate of the widget's top-left corner
/// * `widget_w` - Width of the widget
/// * `widget_h` - Height of the widget
///
/// # Returns
///
/// A tuple containing:
/// * `bool` - Whether the widget is visible (within the viewport)
/// * `f32` - Distance from the viewport if not visible, 0.0 if visible
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn is_visible(
    viewport_x: f32,
    viewport_y: f32,
    viewport_w: f32,
    viewport_h: f32,
    widget_x: f32,
    widget_y: f32,
    widget_w: f32,
    widget_h: f32,
) -> (bool, f32) {
    let mut x = widget_x;
    let mut y = widget_y;
    let w = widget_w;
    let h = widget_h;
    log::trace!("is_widget_visible: widget x={x} y={y} w={w} h={h}");

    log::trace!(
        "is_widget_visible: {x} -= {} = {}",
        viewport_x,
        x - viewport_x
    );
    x -= viewport_x;
    log::trace!(
        "is_widget_visible: {y} -= {} = {}",
        viewport_y,
        y - viewport_y
    );
    y -= viewport_y;

    #[allow(clippy::cast_sign_loss)]
    let dist_x = max_f32(0.0, max_f32(-(x + w), x - viewport_w));
    #[allow(clippy::cast_sign_loss)]
    let dist_y = max_f32(0.0, max_f32(-(y + h), y - viewport_h));

    let dist = max_f32(dist_x, dist_y);

    log::trace!(
        "is_widget_visible:\n\t\
            {dist_x} == 0 &&\n\t\
            {dist_y} == 0"
    );

    if dist_x < 0.001 && dist_y < 0.001 {
        log::trace!("is_widget_visible: visible");
        return (true, dist);
    }

    log::trace!("is_widget_visible: not visible");

    (false, dist)
}
