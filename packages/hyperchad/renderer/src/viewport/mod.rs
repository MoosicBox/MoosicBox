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
    fn test_is_visible_returns_correct_distance_to_right() {
        // Widget 50 units to the right of viewport edge
        // Viewport: (0,0) with size 100x100
        // Widget: (150, 50) with size 20x20 - starts 50 units past viewport right edge
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 150.0, 50.0, 20.0, 20.0);

        assert!(!visible);
        // Distance should be 50.0 (widget_x - viewport_w = 150 - 100)
        assert!(
            (dist - 50.0).abs() < 0.01,
            "Expected dist ~50.0, got {dist}"
        );
    }

    #[test_log::test]
    fn test_is_visible_returns_correct_distance_below() {
        // Widget 30 units below viewport edge
        // Viewport: (0,0) with size 100x100
        // Widget: (50, 130) with size 20x20 - starts 30 units past viewport bottom edge
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 50.0, 130.0, 20.0, 20.0);

        assert!(!visible);
        // Distance should be 30.0 (widget_y - viewport_h = 130 - 100)
        assert!(
            (dist - 30.0).abs() < 0.01,
            "Expected dist ~30.0, got {dist}"
        );
    }

    #[test_log::test]
    fn test_is_visible_returns_correct_distance_to_left() {
        // Widget to the left of viewport
        // Viewport: (0,0) with size 100x100
        // Widget: (-70, 50) with size 20x20 - ends 50 units before viewport left edge
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, -70.0, 50.0, 20.0, 20.0);

        assert!(!visible);
        // Distance should be 50.0: -(x + w) = -(-70 + 20) = -(-50) = 50
        assert!(
            (dist - 50.0).abs() < 0.01,
            "Expected dist ~50.0, got {dist}"
        );
    }

    #[test_log::test]
    fn test_is_visible_returns_correct_distance_above() {
        // Widget above viewport
        // Viewport: (0,0) with size 100x100
        // Widget: (50, -80) with size 20x20 - ends 60 units above viewport top edge
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 50.0, -80.0, 20.0, 20.0);

        assert!(!visible);
        // Distance should be 60.0: -(y + h) = -(-80 + 20) = -(-60) = 60
        assert!(
            (dist - 60.0).abs() < 0.01,
            "Expected dist ~60.0, got {dist}"
        );
    }

    #[test_log::test]
    fn test_is_visible_returns_max_distance_when_outside_both_axes() {
        // Widget outside viewport on both axes - should return max of x and y distances
        // Viewport: (0,0) with size 100x100
        // Widget: (150, 200) - 50 units to the right, 100 units below
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 150.0, 200.0, 20.0, 20.0);

        assert!(!visible);
        // x_dist = 150 - 100 = 50, y_dist = 200 - 100 = 100
        // max(50, 100) = 100
        assert!(
            (dist - 100.0).abs() < 0.01,
            "Expected dist ~100.0, got {dist}"
        );
    }

    #[test_log::test]
    fn test_is_visible_with_scrolled_viewport() {
        // Viewport scrolled to position (500, 300)
        // Widget visible within scrolled viewport
        let (visible, dist) = is_visible(500.0, 300.0, 100.0, 100.0, 520.0, 320.0, 20.0, 20.0);

        assert!(visible);
        assert!(dist < 0.001, "Expected dist ~0.0, got {dist}");
    }

    #[test_log::test]
    fn test_is_visible_widget_just_barely_outside() {
        // Widget just 1 pixel outside viewport edge (past threshold)
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 101.0, 50.0, 20.0, 20.0);

        assert!(!visible);
        assert!((dist - 1.0).abs() < 0.01, "Expected dist ~1.0, got {dist}");
    }

    #[test_log::test]
    fn test_is_visible_widget_just_barely_inside_threshold() {
        // Widget overlapping by 0.0001 - should still be considered visible due to 0.001 threshold
        // Viewport ends at 100, widget starts at 99.9995 with width 20
        let (visible, dist) = is_visible(0.0, 0.0, 100.0, 100.0, 99.9995, 50.0, 20.0, 20.0);

        assert!(
            visible,
            "Widget overlapping by tiny amount should be visible"
        );
        assert!(dist < 0.001);
    }
}
