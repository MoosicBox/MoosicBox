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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_is_visible_widget_fully_inside_viewport() {
        // Widget at (10, 10) with size 50x50, viewport at (0, 0) with size 800x600
        let (visible, dist) = is_visible(0.0, 0.0, 800.0, 600.0, 10.0, 10.0, 50.0, 50.0);

        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_widget_partially_overlapping_viewport() {
        // Widget partially overlapping viewport from the right
        let (visible, dist) = is_visible(0.0, 0.0, 800.0, 600.0, 750.0, 100.0, 100.0, 100.0);

        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_widget_completely_outside_viewport_right() {
        // Widget completely to the right of viewport
        let (visible, dist) = is_visible(0.0, 0.0, 800.0, 600.0, 850.0, 100.0, 50.0, 50.0);

        assert!(!visible);
        assert!(dist > 0.0);
    }

    #[test_log::test]
    fn test_is_visible_widget_completely_outside_viewport_left() {
        // Widget completely to the left of viewport
        let (visible, dist) = is_visible(0.0, 0.0, 800.0, 600.0, -100.0, 100.0, 50.0, 50.0);

        assert!(!visible);
        assert!(dist > 0.0);
    }

    #[test_log::test]
    fn test_is_visible_widget_completely_outside_viewport_below() {
        // Widget completely below viewport
        let (visible, dist) = is_visible(0.0, 0.0, 800.0, 600.0, 100.0, 650.0, 50.0, 50.0);

        assert!(!visible);
        assert!(dist > 0.0);
    }

    #[test_log::test]
    fn test_is_visible_widget_completely_outside_viewport_above() {
        // Widget completely above viewport
        let (visible, dist) = is_visible(0.0, 0.0, 800.0, 600.0, 100.0, -100.0, 50.0, 50.0);

        assert!(!visible);
        assert!(dist > 0.0);
    }

    #[test_log::test]
    fn test_is_visible_widget_at_viewport_edge() {
        // Widget touching the right edge of viewport
        let (visible, dist) = is_visible(0.0, 0.0, 800.0, 600.0, 750.0, 100.0, 50.0, 50.0);

        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_widget_at_viewport_corner() {
        // Widget at bottom-right corner
        let (visible, dist) = is_visible(0.0, 0.0, 800.0, 600.0, 750.0, 550.0, 50.0, 50.0);

        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_with_negative_viewport_position() {
        // Viewport scrolled to negative position
        let (visible, dist) = is_visible(-100.0, -50.0, 800.0, 600.0, 10.0, 10.0, 50.0, 50.0);

        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_zero_size_widget() {
        // Widget with zero size at origin
        let (visible, dist) = is_visible(0.0, 0.0, 800.0, 600.0, 0.0, 0.0, 0.0, 0.0);

        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_max_f32_returns_larger_value() {
        assert!((max_f32(5.0, 10.0) - 10.0).abs() < f32::EPSILON);
        assert!((max_f32(10.0, 5.0) - 10.0).abs() < f32::EPSILON);
        assert!((max_f32(7.5, 7.5) - 7.5).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_max_f32_with_negative_values() {
        assert!((max_f32(-5.0, -10.0) - (-5.0)).abs() < f32::EPSILON);
        assert!((max_f32(-10.0, -5.0) - (-5.0)).abs() < f32::EPSILON);
        assert!((max_f32(-5.0, 10.0) - 10.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_is_visible_widget_larger_than_viewport() {
        // Widget is larger than viewport but overlaps with it
        let (visible, dist) = is_visible(100.0, 100.0, 200.0, 200.0, 0.0, 0.0, 500.0, 500.0);

        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_widget_completely_containing_viewport() {
        // Widget completely contains the viewport
        let (visible, dist) = is_visible(100.0, 100.0, 50.0, 50.0, 50.0, 50.0, 200.0, 200.0);

        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_widget_exactly_at_boundary_right() {
        // Widget starting exactly at the right boundary (just outside)
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 100.0, 50.0, 50.0, 50.0);

        // Widget x=100 is exactly at boundary, so x - viewport_w = 100 - 100 = 0
        // This means dist_x = 0, so it should be visible
        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_widget_exactly_at_boundary_bottom() {
        // Widget starting exactly at the bottom boundary
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 50.0, 100.0, 50.0, 50.0);

        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_widget_just_past_right_boundary() {
        // Widget starting just past the right boundary
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 100.1, 50.0, 50.0, 50.0);

        assert!(!visible);
        assert!(dist > 0.0);
    }

    #[test_log::test]
    fn test_is_visible_widget_just_past_bottom_boundary() {
        // Widget starting just past the bottom boundary
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 50.0, 100.1, 50.0, 50.0);

        assert!(!visible);
        assert!(dist > 0.0);
    }

    #[test_log::test]
    fn test_is_visible_widget_just_before_left_boundary() {
        // Widget ending just before entering the viewport from the left
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, -50.1, 50.0, 50.0, 50.0);

        assert!(!visible);
        assert!(dist > 0.0);
    }

    #[test_log::test]
    fn test_is_visible_widget_just_before_top_boundary() {
        // Widget ending just before entering the viewport from the top
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 50.0, -50.1, 50.0, 50.0);

        assert!(!visible);
        assert!(dist > 0.0);
    }

    #[test_log::test]
    fn test_is_visible_widget_touching_left_boundary() {
        // Widget just touching the left boundary of viewport
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, -50.0, 50.0, 50.0, 50.0);

        // x = -50, w = 50, so x + w = 0, meaning it touches the boundary
        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_widget_touching_top_boundary() {
        // Widget just touching the top boundary of viewport
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 50.0, -50.0, 50.0, 50.0);

        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_distance_calculation_outside_right() {
        // Widget 100 pixels outside to the right
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 200.0, 50.0, 50.0, 50.0);

        assert!(!visible);
        // Widget at x=200, viewport width=100, so distance = 200 - 100 = 100
        assert!((dist - 100.0).abs() < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_distance_calculation_outside_bottom() {
        // Widget 100 pixels outside at the bottom
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 50.0, 200.0, 50.0, 50.0);

        assert!(!visible);
        // Widget at y=200, viewport height=100, so distance = 200 - 100 = 100
        assert!((dist - 100.0).abs() < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_diagonal_distance_returns_max() {
        // Widget outside both right and bottom - should return max of the two distances
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 200.0, 300.0, 50.0, 50.0);

        assert!(!visible);
        // dist_x = 200 - 100 = 100, dist_y = 300 - 100 = 200
        // max(100, 200) = 200
        assert!((dist - 200.0).abs() < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_with_scrolled_viewport() {
        // Viewport scrolled to position 500, 500
        let (visible, dist) = is_visible(500.0, 500.0, 100.0, 100.0, 550.0, 550.0, 50.0, 50.0);

        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_is_visible_widget_outside_scrolled_viewport() {
        // Viewport scrolled to position 500, 500, widget at origin
        let (visible, dist) = is_visible(500.0, 500.0, 100.0, 100.0, 0.0, 0.0, 50.0, 50.0);

        assert!(!visible);
        // Widget (0,0 to 50,50) vs viewport starting at (500,500)
        // After subtracting viewport position: widget at (-500, -500)
        // dist_x = max(0, max(-(−500+50), -500 - 100)) = max(0, max(450, -600)) = 450
        // dist_y = max(0, max(-(−500+50), -500 - 100)) = max(0, max(450, -600)) = 450
        assert!(dist > 0.0);
    }
}
