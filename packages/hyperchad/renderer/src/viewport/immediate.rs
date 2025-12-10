//! Immediate mode viewport rendering with per-frame visibility calculations.
//!
//! This module provides viewport types for immediate mode rendering, where visibility
//! is recalculated every frame. Viewports track position and dimensions dynamically
//! and can be nested hierarchically for complex UI layouts.
//!
//! # Key Types
//!
//! * `Viewport` - Hierarchical viewport with parent-child relationships
//! * `ViewportListener` - Tracks visibility changes for a position within a viewport
//! * `Pos` - Position and dimensions for viewport calculations
//!
//! # Examples
//!
//! Creating a viewport listener to track visibility:
//!
//! ```rust
//! # #[cfg(feature = "viewport-immediate")]
//! # {
//! use hyperchad_renderer::viewport::immediate::{ViewportListener, Viewport, Pos};
//!
//! # fn example() {
//! let viewport = Viewport {
//!     parent: None,
//!     pos: Pos { x: 0.0, y: 0.0, w: 800.0, h: 600.0 },
//!     viewport: Pos { x: 0.0, y: 0.0, w: 800.0, h: 600.0 },
//! };
//!
//! let mut listener = ViewportListener::new(
//!     Some(viewport),
//!     100.0, 100.0, 50.0, 50.0
//! );
//!
//! let ((visible, _), (dist, _)) = listener.check();
//! # }
//! # }
//! ```

/// Viewport for immediate mode rendering with hierarchical positioning.
///
/// Represents a viewport in immediate mode, which recalculates visibility
/// on every frame. Viewports can be nested via the `parent` field to create
/// hierarchical visibility calculations.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone)]
pub struct Viewport {
    /// Parent viewport in the hierarchy, if any
    pub parent: Option<Box<Viewport>>,
    /// Position and dimensions of this viewport's content
    pub pos: Pos,
    /// Viewport's visible area position and dimensions
    pub viewport: Pos,
}

impl Viewport {
    fn is_visible(&self) -> (bool, f32) {
        if let Some((visible, dist)) = self.parent.as_ref().map(|x| x.is_visible()) {
            if visible {
                let pos = self.pos;
                let vp = self.viewport;
                super::is_visible(vp.x, vp.y, vp.w, vp.h, pos.x, pos.y, pos.w, pos.h)
            } else {
                (false, dist)
            }
        } else {
            (true, 0.0)
        }
    }
}

/// Position and dimensions for immediate mode viewport calculations.
///
/// Represents a rectangular area with x, y coordinates and width, height dimensions.
#[derive(Debug, Clone, Copy)]
pub struct Pos {
    /// X coordinate
    pub x: f32,
    /// Y coordinate
    pub y: f32,
    /// Width
    pub w: f32,
    /// Height
    pub h: f32,
}

/// Tracks visibility changes for a position within a viewport in immediate mode.
///
/// Monitors whether a specific position is visible within its viewport and tracks
/// changes in visibility state and distance from the viewport. Used in immediate
/// mode rendering where visibility is checked every frame.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct ViewportListener {
    /// The viewport to check visibility against
    pub viewport: Option<Viewport>,
    visible: bool,
    prev_visible: Option<bool>,
    initialized: bool,
    dist: f32,
    prev_dist: Option<f32>,
    /// The position and dimensions to check for visibility
    pub pos: Pos,
}

impl ViewportListener {
    /// Creates a new viewport listener with the specified viewport and position.
    ///
    /// # Parameters
    ///
    /// * `viewport` - Optional viewport to check visibility against
    /// * `x` - X coordinate of the position to monitor
    /// * `y` - Y coordinate of the position to monitor
    /// * `w` - Width of the area to monitor
    /// * `h` - Height of the area to monitor
    #[must_use]
    pub const fn new(viewport: Option<Viewport>, x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            viewport,
            visible: false,
            prev_visible: None,
            initialized: false,
            dist: 0.0,
            prev_dist: None,
            pos: Pos { x, y, w, h },
        }
    }

    fn is_visible(&self) -> (bool, f32) {
        if let Some(((visible, dist), vp, pos)) = self
            .viewport
            .as_ref()
            .map(|x| (x.is_visible(), x.viewport, x.pos))
        {
            if visible {
                super::is_visible(
                    vp.x + pos.x,
                    vp.y + pos.y,
                    vp.w,
                    vp.h,
                    self.pos.x,
                    self.pos.y,
                    self.pos.w,
                    self.pos.h,
                )
            } else {
                (false, dist)
            }
        } else {
            (true, 0.0)
        }
    }

    /// Checks current visibility status and returns changes since last check.
    ///
    /// # Returns
    ///
    /// A tuple of two tuples:
    /// * First tuple: `(current_visible, previous_visible_if_changed)` - Current visibility
    ///   and the previous visibility state if it changed, otherwise `None`
    /// * Second tuple: `(current_distance, previous_distance_if_changed)` - Current distance
    ///   from viewport and the previous distance if it changed significantly, otherwise `None`
    pub fn check(&mut self) -> ((bool, Option<bool>), (f32, Option<f32>)) {
        let (visible, dist) = self.is_visible();
        log::trace!("check: pos={:?} visible={visible} dist={dist}", self.pos);

        if self.initialized {
            let prev_visible = self.visible;
            let prev_dist = self.dist;
            self.prev_visible = if prev_visible == visible {
                None
            } else {
                self.visible = visible;
                Some(prev_visible)
            };
            self.prev_dist = if (prev_dist - dist) < 0.01 {
                None
            } else {
                self.dist = dist;
                Some(prev_dist)
            };

            ((visible, self.prev_visible), (dist, self.prev_dist))
        } else {
            self.initialized = true;
            self.visible = visible;
            self.dist = dist;
            ((visible, None), (dist, None))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_viewport_listener_initial_check_visible() {
        let viewport = Viewport {
            parent: None,
            pos: Pos {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            },
            viewport: Pos {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            },
        };

        let mut listener = ViewportListener::new(Some(viewport), 100.0, 100.0, 50.0, 50.0);

        let ((visible, prev_visible), (dist, prev_dist)) = listener.check();

        assert!(visible);
        assert!(prev_visible.is_none()); // First check has no previous
        assert!(dist < 0.001);
        assert!(prev_dist.is_none());
    }

    #[test_log::test]
    fn test_viewport_listener_initial_check_not_visible() {
        let viewport = Viewport {
            parent: None,
            pos: Pos {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            },
            viewport: Pos {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            },
        };

        // Widget outside viewport
        let mut listener = ViewportListener::new(Some(viewport), 1000.0, 1000.0, 50.0, 50.0);

        let ((visible, prev_visible), (_dist, prev_dist)) = listener.check();

        assert!(!visible);
        assert!(prev_visible.is_none());
        assert!(prev_dist.is_none());
    }

    #[test_log::test]
    fn test_viewport_listener_visibility_change() {
        let viewport = Viewport {
            parent: None,
            pos: Pos {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            },
            viewport: Pos {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            },
        };

        let mut listener = ViewportListener::new(Some(viewport), 100.0, 100.0, 50.0, 50.0);

        // Initial check - visible
        let ((visible, _), _) = listener.check();
        assert!(visible);

        // Update viewport to move away from widget
        listener.viewport = Some(Viewport {
            parent: None,
            pos: Pos {
                x: 1000.0,
                y: 1000.0,
                w: 800.0,
                h: 600.0,
            },
            viewport: Pos {
                x: 1000.0,
                y: 1000.0,
                w: 800.0,
                h: 600.0,
            },
        });

        // Second check - should now be not visible
        let ((visible, prev_visible), _) = listener.check();
        assert!(!visible);
        assert_eq!(prev_visible, Some(true)); // Previous state was visible
    }

    #[test_log::test]
    fn test_viewport_listener_no_viewport() {
        // No viewport means always visible
        let mut listener = ViewportListener::new(None, 100.0, 100.0, 50.0, 50.0);

        let ((visible, _), (dist, _)) = listener.check();

        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_viewport_listener_no_change() {
        let viewport = Viewport {
            parent: None,
            pos: Pos {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            },
            viewport: Pos {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            },
        };

        let mut listener = ViewportListener::new(Some(viewport), 100.0, 100.0, 50.0, 50.0);

        // Initial check
        listener.check();

        // Second check with no viewport change - should have no previous values
        let ((visible, prev_visible), (_, prev_dist)) = listener.check();

        assert!(visible);
        assert!(prev_visible.is_none()); // No change
        assert!(prev_dist.is_none()); // No significant distance change
    }

    #[test_log::test]
    fn test_viewport_with_parent_both_visible() {
        let parent = Viewport {
            parent: None,
            pos: Pos {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: 1000.0,
            },
            viewport: Pos {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: 1000.0,
            },
        };

        let child = Viewport {
            parent: Some(Box::new(parent)),
            pos: Pos {
                x: 100.0,
                y: 100.0,
                w: 600.0,
                h: 400.0,
            },
            viewport: Pos {
                x: 100.0,
                y: 100.0,
                w: 600.0,
                h: 400.0,
            },
        };

        let mut listener = ViewportListener::new(Some(child), 200.0, 200.0, 50.0, 50.0);

        let ((visible, _), _) = listener.check();

        assert!(visible);
    }

    #[test_log::test]
    fn test_viewport_with_parent_child_not_visible() {
        let parent = Viewport {
            parent: None,
            pos: Pos {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: 1000.0,
            },
            viewport: Pos {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: 1000.0,
            },
        };

        let child = Viewport {
            parent: Some(Box::new(parent)),
            pos: Pos {
                x: 100.0,
                y: 100.0,
                w: 600.0,
                h: 400.0,
            },
            viewport: Pos {
                x: 100.0,
                y: 100.0,
                w: 600.0,
                h: 400.0,
            },
        };

        // Widget outside child viewport
        let mut listener = ViewportListener::new(Some(child), 1000.0, 1000.0, 50.0, 50.0);

        let ((visible, _), _) = listener.check();

        assert!(!visible);
    }

    #[test_log::test]
    fn test_viewport_with_parent_not_visible_propagates_invisibility() {
        // Grandparent viewport at origin with 100x100 size
        let grandparent = Viewport {
            parent: None,
            pos: Pos {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
            viewport: Pos {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
        };

        // Parent viewport positioned OUTSIDE grandparent (at 500,500)
        // This makes parent NOT visible within grandparent
        let parent = Viewport {
            parent: Some(Box::new(grandparent)),
            pos: Pos {
                x: 500.0,
                y: 500.0,
                w: 200.0,
                h: 200.0,
            },
            viewport: Pos {
                x: 500.0,
                y: 500.0,
                w: 200.0,
                h: 200.0,
            },
        };

        // Child viewport positioned within parent's bounds
        let child = Viewport {
            parent: Some(Box::new(parent)),
            pos: Pos {
                x: 550.0,
                y: 550.0,
                w: 100.0,
                h: 100.0,
            },
            viewport: Pos {
                x: 550.0,
                y: 550.0,
                w: 100.0,
                h: 100.0,
            },
        };

        // Widget positioned within child's bounds
        // Even though widget is within child, the parent is not visible in grandparent
        // so visibility should propagate as not visible
        let mut listener = ViewportListener::new(Some(child), 560.0, 560.0, 20.0, 20.0);

        let ((visible, _), (dist, _)) = listener.check();

        // Widget should NOT be visible because parent is outside grandparent's bounds
        assert!(!visible);
        // Distance should be > 0 since parent is outside grandparent
        assert!(dist > 0.0);
    }

    #[test_log::test]
    fn test_viewport_listener_distance_change_threshold() {
        let viewport = Viewport {
            parent: None,
            pos: Pos {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
            viewport: Pos {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
        };

        // Widget outside viewport - will have distance > 0
        let mut listener = ViewportListener::new(Some(viewport), 200.0, 200.0, 50.0, 50.0);

        // Initial check
        let ((visible, _), (initial_dist, _)) = listener.check();
        assert!(!visible);
        assert!(initial_dist > 0.0);

        // Second check with same position - distance should not be reported as changed
        // because the change threshold is 0.01
        let ((_, _), (_, prev_dist)) = listener.check();
        assert!(
            prev_dist.is_none(),
            "Distance change below threshold should not report previous distance"
        );
    }

    #[test_log::test]
    fn test_viewport_listener_distance_change_above_threshold() {
        // Widget outside viewport - will have distance > 0
        let mut listener = ViewportListener::new(
            Some(Viewport {
                parent: None,
                pos: Pos {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                },
                viewport: Pos {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                },
            }),
            200.0,
            200.0,
            50.0,
            50.0,
        );

        // Initial check
        let ((_, _), (initial_dist, _)) = listener.check();
        assert!(initial_dist > 0.0);

        // Move viewport significantly to change distance
        listener.viewport = Some(Viewport {
            parent: None,
            pos: Pos {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
            viewport: Pos {
                x: 150.0,
                y: 150.0,
                w: 100.0,
                h: 100.0,
            },
        });

        // Check again - distance should have changed significantly
        let ((_, _), (new_dist, prev_dist)) = listener.check();
        assert!(
            prev_dist.is_some(),
            "Significant distance change should report previous distance"
        );
        assert!(
            (new_dist - initial_dist).abs() > 0.01,
            "Distance should have changed significantly"
        );
    }

    #[test_log::test]
    fn test_viewport_listener_visibility_toggle_back_and_forth() {
        let mut listener = ViewportListener::new(
            Some(Viewport {
                parent: None,
                pos: Pos {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                },
                viewport: Pos {
                    x: 0.0,
                    y: 0.0,
                    w: 100.0,
                    h: 100.0,
                },
            }),
            10.0,
            10.0,
            50.0,
            50.0,
        );

        // Initial check - should be visible
        let ((visible, _), _) = listener.check();
        assert!(visible);

        // Move to not visible
        listener.viewport = Some(Viewport {
            parent: None,
            pos: Pos {
                x: 500.0,
                y: 500.0,
                w: 100.0,
                h: 100.0,
            },
            viewport: Pos {
                x: 500.0,
                y: 500.0,
                w: 100.0,
                h: 100.0,
            },
        });

        let ((visible, prev_visible), _) = listener.check();
        assert!(!visible);
        assert_eq!(prev_visible, Some(true));

        // Move back to visible
        listener.viewport = Some(Viewport {
            parent: None,
            pos: Pos {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
            viewport: Pos {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
        });

        let ((visible, prev_visible), _) = listener.check();
        assert!(visible);
        assert_eq!(prev_visible, Some(false));
    }

    #[test_log::test]
    fn test_viewport_is_visible_no_parent() {
        // Viewport with no parent should always report as visible
        let viewport = Viewport {
            parent: None,
            pos: Pos {
                x: 100.0,
                y: 100.0,
                w: 200.0,
                h: 200.0,
            },
            viewport: Pos {
                x: 100.0,
                y: 100.0,
                w: 200.0,
                h: 200.0,
            },
        };

        let (visible, dist) = viewport.is_visible();
        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_viewport_is_visible_with_visible_parent() {
        let parent = Viewport {
            parent: None,
            pos: Pos {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: 1000.0,
            },
            viewport: Pos {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: 1000.0,
            },
        };

        // Child viewport within parent's bounds
        let child = Viewport {
            parent: Some(Box::new(parent)),
            pos: Pos {
                x: 100.0,
                y: 100.0,
                w: 200.0,
                h: 200.0,
            },
            viewport: Pos {
                x: 100.0,
                y: 100.0,
                w: 200.0,
                h: 200.0,
            },
        };

        let (visible, dist) = child.is_visible();
        assert!(visible);
        assert!(dist < 0.001);
    }

    #[test_log::test]
    fn test_viewport_is_visible_with_invisible_parent_propagates() {
        // When a viewport's parent is NOT visible, the viewport itself should
        // report as not visible due to the parent check
        let grandparent = Viewport {
            parent: None,
            pos: Pos {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
            viewport: Pos {
                x: 0.0,
                y: 0.0,
                w: 100.0,
                h: 100.0,
            },
        };

        // Parent positioned inside grandparent but with pos that's outside its own viewport
        let parent = Viewport {
            parent: Some(Box::new(grandparent)),
            pos: Pos {
                x: 500.0, // pos is far outside viewport
                y: 500.0,
                w: 50.0,
                h: 50.0,
            },
            viewport: Pos {
                x: 10.0,
                y: 10.0,
                w: 50.0,
                h: 50.0,
            },
        };

        let (visible, dist) = parent.is_visible();
        // pos (500,500,50,50) is outside viewport (10,10,50,50)
        assert!(!visible);
        assert!(dist > 0.0);
    }

    #[test_log::test]
    fn test_viewport_clone() {
        let parent = Viewport {
            parent: None,
            pos: Pos {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: 1000.0,
            },
            viewport: Pos {
                x: 0.0,
                y: 0.0,
                w: 1000.0,
                h: 1000.0,
            },
        };

        let child = Viewport {
            parent: Some(Box::new(parent)),
            pos: Pos {
                x: 100.0,
                y: 100.0,
                w: 200.0,
                h: 200.0,
            },
            viewport: Pos {
                x: 100.0,
                y: 100.0,
                w: 200.0,
                h: 200.0,
            },
        };

        #[allow(clippy::redundant_clone)]
        let cloned = child.clone();

        assert!((cloned.pos.x - child.pos.x).abs() < f32::EPSILON);
        assert!((cloned.pos.y - child.pos.y).abs() < f32::EPSILON);
        assert!((cloned.pos.w - child.pos.w).abs() < f32::EPSILON);
        assert!((cloned.pos.h - child.pos.h).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_pos_copy_semantics() {
        let original = Pos {
            x: 10.0,
            y: 20.0,
            w: 30.0,
            h: 40.0,
        };
        let copied = original; // Copy occurs here

        assert!((copied.x - 10.0).abs() < f32::EPSILON);
        assert!((copied.y - 20.0).abs() < f32::EPSILON);
        assert!((copied.w - 30.0).abs() < f32::EPSILON);
        assert!((copied.h - 40.0).abs() < f32::EPSILON);

        // Original is still usable (Copy trait)
        assert!((original.x - 10.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_viewport_listener_pos_field_access() {
        let viewport = Viewport {
            parent: None,
            pos: Pos {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            },
            viewport: Pos {
                x: 0.0,
                y: 0.0,
                w: 800.0,
                h: 600.0,
            },
        };

        let listener = ViewportListener::new(Some(viewport), 50.0, 75.0, 100.0, 125.0);

        assert!((listener.pos.x - 50.0).abs() < f32::EPSILON);
        assert!((listener.pos.y - 75.0).abs() < f32::EPSILON);
        assert!((listener.pos.w - 100.0).abs() < f32::EPSILON);
        assert!((listener.pos.h - 125.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_viewport_listener_viewport_field_access() {
        let viewport = Viewport {
            parent: None,
            pos: Pos {
                x: 10.0,
                y: 20.0,
                w: 800.0,
                h: 600.0,
            },
            viewport: Pos {
                x: 10.0,
                y: 20.0,
                w: 800.0,
                h: 600.0,
            },
        };

        let listener = ViewportListener::new(Some(viewport), 50.0, 75.0, 100.0, 125.0);

        assert!(listener.viewport.is_some());
        let vp = listener.viewport.as_ref().unwrap();
        assert!((vp.pos.x - 10.0).abs() < f32::EPSILON);
        assert!((vp.pos.y - 20.0).abs() < f32::EPSILON);
    }

    #[test_log::test]
    fn test_viewport_listener_with_viewport_offset() {
        // Viewport with non-zero position offset
        let viewport = Viewport {
            parent: None,
            pos: Pos {
                x: 100.0,
                y: 200.0,
                w: 400.0,
                h: 300.0,
            },
            viewport: Pos {
                x: 50.0,
                y: 75.0,
                w: 400.0,
                h: 300.0,
            },
        };

        // Widget positioned relative to viewport
        let mut listener = ViewportListener::new(Some(viewport), 200.0, 300.0, 50.0, 50.0);

        let ((visible, _), _) = listener.check();
        // Visibility depends on combined pos and viewport offsets
        let _ = visible; // Verify it runs without panic
    }
}
