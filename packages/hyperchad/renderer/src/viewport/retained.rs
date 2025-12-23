//! Retained mode viewport rendering with persistent widget positions.
//!
//! This module provides viewport types for retained mode rendering, where widget positions
//! are stored and reused across frames. This is more efficient for UIs where elements don't
//! move frequently, as visibility calculations can use cached position data.
//!
//! # Key Types
//!
//! * `Viewport` - Hierarchical viewport with retained widget positions
//! * `ViewportListener` - Monitors visibility changes and invokes callbacks
//! * `ViewportPosition` - Trait for providing viewport dimensions and visibility checks
//! * `WidgetPosition` - Trait for providing widget position and dimensions
//!
//! # Examples
//!
//! Implementing a simple widget position provider:
//!
//! ```rust
//! # #[cfg(feature = "viewport-retained")]
//! # {
//! use hyperchad_renderer::viewport::retained::{WidgetPosition, ViewportPosition, Viewport};
//!
//! struct MyWidget {
//!     x: i32,
//!     y: i32,
//!     width: i32,
//!     height: i32,
//! }
//!
//! impl WidgetPosition for MyWidget {
//!     fn widget_x(&self) -> i32 { self.x }
//!     fn widget_y(&self) -> i32 { self.y }
//!     fn widget_w(&self) -> i32 { self.width }
//!     fn widget_h(&self) -> i32 { self.height }
//! }
//!
//! impl ViewportPosition for MyWidget {
//!     fn viewport_x(&self) -> i32 { self.x }
//!     fn viewport_y(&self) -> i32 { self.y }
//!     fn viewport_w(&self) -> i32 { self.width }
//!     fn viewport_h(&self) -> i32 { self.height }
//!     fn as_widget_position(&self) -> Box<dyn WidgetPosition> {
//!         Box::new(MyWidget {
//!             x: self.x,
//!             y: self.y,
//!             width: self.width,
//!             height: self.height,
//!         })
//!     }
//! }
//! # }
//! ```

use std::sync::Arc;

/// Trait for types that provide widget position and dimensions.
///
/// Used in retained mode viewport rendering to query the current position
/// and size of widgets for visibility calculations.
pub trait WidgetPosition: Send + Sync {
    /// Returns the X coordinate of the widget
    fn widget_x(&self) -> i32;
    /// Returns the Y coordinate of the widget
    fn widget_y(&self) -> i32;
    /// Returns the width of the widget
    fn widget_w(&self) -> i32;
    /// Returns the height of the widget
    fn widget_h(&self) -> i32;
}

impl std::fmt::Debug for dyn WidgetPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "({}, {}, {}, {})",
            self.widget_x(),
            self.widget_y(),
            self.widget_w(),
            self.widget_h()
        ))
    }
}

/// Viewport for retained mode rendering with hierarchical positioning.
///
/// Represents a viewport in retained mode, where widget positions are stored
/// and reused across frames. Viewports can be nested via the `parent` field
/// to create hierarchical visibility calculations.
#[derive(Clone)]
pub struct Viewport {
    widget: Arc<Box<dyn WidgetPosition>>,
    parent: Option<Box<Self>>,
    position: Arc<Box<dyn ViewportPosition + Send + Sync>>,
}

impl std::fmt::Debug for Viewport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut binding = f.debug_struct("Viewport");
        let x = binding
            .field("x", &self.x())
            .field("y", &self.y())
            .field("w", &self.w())
            .field("h", &self.h());

        if let Some(parent) = &self.parent {
            x.field("parent", &parent);
        }

        x.finish_non_exhaustive()
    }
}

impl Viewport {
    /// Creates a new viewport with the given parent and position.
    ///
    /// # Parameters
    ///
    /// * `parent` - Optional parent viewport for hierarchical visibility checking
    /// * `position` - The position provider for this viewport
    #[must_use]
    pub fn new(
        parent: Option<Self>,
        position: impl ViewportPosition + Send + Sync + 'static,
    ) -> Self {
        Self {
            widget: Arc::new(position.as_widget_position()),
            parent: parent.map(Box::new),
            position: Arc::new(Box::new(position)),
        }
    }

    fn x(&self) -> i32 {
        self.position.viewport_x()
    }

    fn y(&self) -> i32 {
        self.position.viewport_y()
    }

    fn w(&self) -> i32 {
        self.position.viewport_w()
    }

    fn h(&self) -> i32 {
        self.position.viewport_h()
    }

    /// Checks if a widget is visible within this viewport hierarchy.
    ///
    /// Recursively checks visibility through parent viewports. The widget is only
    /// visible if it's visible in both the current viewport and all ancestor viewports.
    ///
    /// # Parameters
    ///
    /// * `widget` - The widget to check for visibility
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// * `bool` - Whether the widget is visible in this viewport and all parent viewports
    /// * `u32` - Combined distance from viewports if not visible, 0 if visible
    fn is_widget_visible(&self, widget: &dyn WidgetPosition) -> (bool, u32) {
        let (visible_in_current_viewport, dist) =
            self.position.is_widget_visible(&**self.widget, widget);

        // FIXME: This doesn't correctly check the position leaf widget (the param above)
        // within this viewport itself, but this probably isn't a huge issue since nested
        // `Viewport`s isn't super likely yet.
        if visible_in_current_viewport {
            self.parent
                .as_ref()
                .map_or((visible_in_current_viewport, dist), |parent| {
                    let (parent_visible, parent_dist) = parent.is_widget_visible(&**self.widget);

                    (
                        visible_in_current_viewport && parent_visible,
                        dist + parent_dist,
                    )
                })
        } else {
            (false, dist)
        }
    }
}

#[allow(clippy::module_name_repetitions)]
/// Trait for types that provide viewport position and dimensions.
///
/// Used in retained mode viewport rendering to query the viewport's visible
/// area and perform visibility checks for widgets within that area.
pub trait ViewportPosition {
    /// Returns the X coordinate of the viewport
    fn viewport_x(&self) -> i32;
    /// Returns the Y coordinate of the viewport
    fn viewport_y(&self) -> i32;
    /// Returns the width of the viewport
    fn viewport_w(&self) -> i32;
    /// Returns the height of the viewport
    fn viewport_h(&self) -> i32;
    /// Converts this viewport position to a widget position
    fn as_widget_position(&self) -> Box<dyn WidgetPosition>;

    /// Checks if a widget is visible within this viewport.
    ///
    /// # Parameters
    ///
    /// * `this_widget` - The widget representing this viewport's position
    /// * `widget` - The widget to check for visibility
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// * `bool` - Whether the widget is visible
    /// * `u32` - Distance from the viewport if not visible, 0 if visible
    fn is_widget_visible(
        &self,
        this_widget: &dyn WidgetPosition,
        widget: &dyn WidgetPosition,
    ) -> (bool, u32) {
        #[allow(clippy::cast_precision_loss)]
        let (visible, dist) = super::is_visible(
            this_widget.widget_x() as f32,
            this_widget.widget_y() as f32,
            self.viewport_w() as f32,
            self.viewport_y() as f32,
            widget.widget_x() as f32,
            widget.widget_y() as f32,
            widget.widget_w() as f32,
            widget.widget_h() as f32,
        );

        #[allow(clippy::cast_possible_truncation)]
        #[allow(clippy::cast_sign_loss)]
        (visible, dist.round() as u32)
    }
}

impl std::fmt::Debug for dyn ViewportPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewportPosition")
            .field("x", &self.viewport_x())
            .field("y", &self.viewport_y())
            .field("w", &self.viewport_w())
            .field("h", &self.viewport_h())
            .finish()
    }
}

impl std::fmt::Debug for Box<dyn ViewportPosition + Send + Sync> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewportPosition")
            .field("x", &self.viewport_x())
            .field("y", &self.viewport_y())
            .field("w", &self.viewport_w())
            .field("h", &self.viewport_h())
            .finish()
    }
}

/// Tracks visibility changes for a widget within a viewport in retained mode.
///
/// Monitors whether a specific widget is visible within its viewport and invokes
/// a callback when visibility state or distance changes. Used in retained mode
/// rendering where widget positions are stored and reused across frames.
#[allow(clippy::module_name_repetitions)]
pub struct ViewportListener {
    widget: Box<dyn WidgetPosition>,
    viewport: Option<Viewport>,
    visible: bool,
    dist: u32,
    callback: Box<dyn FnMut(bool, u32) + Send + Sync>,
}

impl std::fmt::Debug for ViewportListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewportListener")
            .field("widget", &self.widget)
            .field("viewport", &self.viewport)
            .field("visible", &self.visible)
            .finish_non_exhaustive()
    }
}

impl ViewportListener {
    /// Creates a new viewport listener for the given widget.
    ///
    /// The callback is invoked immediately with the initial visibility state,
    /// and then subsequently whenever the visibility or distance changes.
    ///
    /// # Parameters
    ///
    /// * `widget` - The widget to monitor for visibility
    /// * `viewport` - Optional viewport to check visibility against
    /// * `callback` - Function called with `(visible, distance)` when changes occur
    pub fn new(
        widget: impl WidgetPosition + 'static,
        viewport: Option<Viewport>,
        callback: impl FnMut(bool, u32) + Send + Sync + 'static,
    ) -> Self {
        let mut this = Self {
            widget: Box::new(widget),
            viewport,
            visible: false,
            dist: 0,
            callback: Box::new(callback),
        };

        this.init();
        this
    }

    fn is_visible(&self) -> (bool, u32) {
        if let Some((visible, dist)) = self
            .viewport
            .as_ref()
            .map(|x| x.is_widget_visible(&*self.widget))
        {
            (visible, dist)
        } else {
            (true, 0)
        }
    }

    fn init(&mut self) {
        let (visible, dist) = self.is_visible();
        self.visible = visible;
        self.dist = dist;
        (self.callback)(visible, dist);
    }

    /// Checks current visibility status and invokes callback if changed.
    ///
    /// Recalculates the widget's visibility within its viewport and calls
    /// the callback if either the visibility state or distance has changed.
    pub fn check(&mut self) {
        let (visible, dist) = self.is_visible();

        if visible != self.visible || dist != self.dist {
            self.visible = visible;
            self.dist = dist;
            (self.callback)(visible, dist);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct TestWidget {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    }

    impl WidgetPosition for TestWidget {
        fn widget_x(&self) -> i32 {
            self.x
        }
        fn widget_y(&self) -> i32 {
            self.y
        }
        fn widget_w(&self) -> i32 {
            self.w
        }
        fn widget_h(&self) -> i32 {
            self.h
        }
    }

    #[derive(Clone)]
    struct TestViewportPosition {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    }

    impl WidgetPosition for TestViewportPosition {
        fn widget_x(&self) -> i32 {
            self.x
        }
        fn widget_y(&self) -> i32 {
            self.y
        }
        fn widget_w(&self) -> i32 {
            self.w
        }
        fn widget_h(&self) -> i32 {
            self.h
        }
    }

    impl ViewportPosition for TestViewportPosition {
        fn viewport_x(&self) -> i32 {
            self.x
        }
        fn viewport_y(&self) -> i32 {
            self.y
        }
        fn viewport_w(&self) -> i32 {
            self.w
        }
        fn viewport_h(&self) -> i32 {
            self.h
        }
        fn as_widget_position(&self) -> Box<dyn WidgetPosition> {
            Box::new(self.clone())
        }
    }

    #[test_log::test]
    fn test_viewport_listener_initial_callback() {
        let widget = TestWidget {
            x: 100,
            y: 100,
            w: 50,
            h: 50,
        };

        let viewport = Viewport::new(
            None,
            TestViewportPosition {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
        );

        let called = Arc::new(Mutex::new(false));
        let called_clone = Arc::clone(&called);

        let _listener = ViewportListener::new(widget, Some(viewport), move |_visible, _dist| {
            *called_clone.lock().unwrap() = true;
        });

        assert!(*called.lock().unwrap(), "Callback should be called on init");
    }

    #[test_log::test]
    fn test_viewport_listener_visibility_change_triggers_callback() {
        let widget = TestWidget {
            x: 100,
            y: 100,
            w: 50,
            h: 50,
        };

        let viewport = Viewport::new(
            None,
            TestViewportPosition {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
        );

        let call_count = Arc::new(Mutex::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let mut listener = ViewportListener::new(widget, Some(viewport), move |_visible, _dist| {
            *call_count_clone.lock().unwrap() += 1;
        });

        // Initial callback on construction
        assert_eq!(*call_count.lock().unwrap(), 1);

        // Check without change - should not trigger callback
        listener.check();
        assert_eq!(*call_count.lock().unwrap(), 1);
    }

    #[test_log::test]
    fn test_viewport_listener_no_viewport_always_visible() {
        let widget = TestWidget {
            x: 100,
            y: 100,
            w: 50,
            h: 50,
        };

        let visible_result = Arc::new(Mutex::new(false));
        let visible_result_clone = Arc::clone(&visible_result);

        let _listener = ViewportListener::new(widget, None, move |visible, dist| {
            *visible_result_clone.lock().unwrap() = visible;
            assert_eq!(dist, 0);
        });

        assert!(
            *visible_result.lock().unwrap(),
            "Widget should be visible when no viewport"
        );
    }

    #[test_log::test]
    fn test_viewport_new_with_parent() {
        let parent = Viewport::new(
            None,
            TestViewportPosition {
                x: 0,
                y: 0,
                w: 1000,
                h: 1000,
            },
        );

        let child = Viewport::new(
            Some(parent),
            TestViewportPosition {
                x: 100,
                y: 100,
                w: 600,
                h: 400,
            },
        );

        // Verify viewport was created successfully
        assert_eq!(child.x(), 100);
        assert_eq!(child.y(), 100);
        assert_eq!(child.w(), 600);
        assert_eq!(child.h(), 400);
    }

    #[test_log::test]
    fn test_viewport_is_widget_visible_calls_is_visible() {
        let viewport = Viewport::new(
            None,
            TestViewportPosition {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
        );

        let widget = TestWidget {
            x: 100,
            y: 100,
            w: 50,
            h: 50,
        };

        // Just verify the function executes without panic
        let (_visible, _dist) = viewport.is_widget_visible(&widget);
    }

    #[test_log::test]
    fn test_viewport_is_widget_visible_outside_viewport() {
        let viewport = Viewport::new(
            None,
            TestViewportPosition {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
        );

        let widget = TestWidget {
            x: 1000,
            y: 1000,
            w: 50,
            h: 50,
        };

        let (visible, dist) = viewport.is_widget_visible(&widget);

        assert!(!visible);
        assert!(dist > 0);
    }

    #[test_log::test]
    fn test_viewport_with_parent_checks_parent_visibility() {
        let parent = Viewport::new(
            None,
            TestViewportPosition {
                x: 0,
                y: 0,
                w: 1000,
                h: 1000,
            },
        );

        let child = Viewport::new(
            Some(parent),
            TestViewportPosition {
                x: 100,
                y: 100,
                w: 600,
                h: 400,
            },
        );

        let widget = TestWidget {
            x: 200,
            y: 200,
            w: 50,
            h: 50,
        };

        // Just verify the hierarchical check executes without panic
        let (_visible, _dist) = child.is_widget_visible(&widget);
    }

    #[test_log::test]
    fn test_widget_position_debug_format() {
        let widget = TestWidget {
            x: 10,
            y: 20,
            w: 30,
            h: 40,
        };

        let widget_ref: &dyn WidgetPosition = &widget;
        let debug_str = format!("{widget_ref:?}");

        assert!(debug_str.contains(&10.to_string()));
        assert!(debug_str.contains(&20.to_string()));
        assert!(debug_str.contains(&30.to_string()));
        assert!(debug_str.contains(&40.to_string()));
    }

    #[test_log::test]
    fn test_viewport_position_debug_format() {
        let vp = TestViewportPosition {
            x: 5,
            y: 15,
            w: 25,
            h: 35,
        };

        let vp_ref: &dyn ViewportPosition = &vp;
        let debug_str = format!("{vp_ref:?}");

        assert!(debug_str.contains(&5.to_string()));
        assert!(debug_str.contains(&15.to_string()));
        assert!(debug_str.contains(&25.to_string()));
        assert!(debug_str.contains(&35.to_string()));
    }

    #[test_log::test]
    fn test_viewport_with_parent_not_visible_propagates_invisibility() {
        // Grandparent viewport at origin with small size (100x100)
        let grandparent = Viewport::new(
            None,
            TestViewportPosition {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
        );

        // Parent viewport positioned OUTSIDE grandparent bounds (at 500,500)
        // This makes parent NOT visible within grandparent
        let parent = Viewport::new(
            Some(grandparent),
            TestViewportPosition {
                x: 500,
                y: 500,
                w: 200,
                h: 200,
            },
        );

        // Widget positioned within parent's bounds
        // Even though widget is within parent, the parent itself is not visible
        // in grandparent, so the widget should also be not visible
        let widget = TestWidget {
            x: 550,
            y: 550,
            w: 50,
            h: 50,
        };

        let (visible, dist) = parent.is_widget_visible(&widget);

        // Widget should NOT be visible because parent is outside grandparent's bounds
        assert!(!visible);
        // Distance should be > 0 indicating how far outside the visible area
        assert!(dist > 0);
    }

    #[test_log::test]
    fn test_viewport_listener_debug_format() {
        let widget = TestWidget {
            x: 10,
            y: 20,
            w: 30,
            h: 40,
        };

        let viewport = Viewport::new(
            None,
            TestViewportPosition {
                x: 0,
                y: 0,
                w: 800,
                h: 600,
            },
        );

        let listener = ViewportListener::new(widget, Some(viewport), |_, _| {});
        let debug_str = format!("{listener:?}");

        assert!(debug_str.contains("ViewportListener"));
        assert!(debug_str.contains("widget"));
        assert!(debug_str.contains("viewport"));
        assert!(debug_str.contains("visible"));
    }

    #[test_log::test]
    fn test_viewport_debug_format() {
        let viewport = Viewport::new(
            None,
            TestViewportPosition {
                x: 50,
                y: 60,
                w: 200,
                h: 300,
            },
        );

        let debug_str = format!("{viewport:?}");

        assert!(debug_str.contains("Viewport"));
        assert!(debug_str.contains("50"));
        assert!(debug_str.contains("60"));
        assert!(debug_str.contains("200"));
        assert!(debug_str.contains("300"));
    }

    #[test_log::test]
    fn test_viewport_debug_format_with_parent() {
        let parent = Viewport::new(
            None,
            TestViewportPosition {
                x: 0,
                y: 0,
                w: 1000,
                h: 1000,
            },
        );

        let child = Viewport::new(
            Some(parent),
            TestViewportPosition {
                x: 100,
                y: 100,
                w: 400,
                h: 300,
            },
        );

        let debug_str = format!("{child:?}");

        assert!(debug_str.contains("Viewport"));
        assert!(debug_str.contains("parent"));
    }

    #[test_log::test]
    fn test_boxed_viewport_position_debug_format() {
        let vp = TestViewportPosition {
            x: 15,
            y: 25,
            w: 35,
            h: 45,
        };

        let boxed: Box<dyn ViewportPosition + Send + Sync> = Box::new(vp);
        let debug_str = format!("{boxed:?}");

        assert!(debug_str.contains("ViewportPosition"));
        assert!(debug_str.contains("15"));
        assert!(debug_str.contains("25"));
        assert!(debug_str.contains("35"));
        assert!(debug_str.contains("45"));
    }

    #[test_log::test]
    fn test_viewport_is_widget_visible_when_not_visible_skips_parent_check() {
        // Create a grandparent viewport that should NOT be checked if child is not visible
        let grandparent = Viewport::new(
            None,
            TestViewportPosition {
                x: 0,
                y: 0,
                w: 2000,
                h: 2000,
            },
        );

        // Parent with small viewport
        let parent = Viewport::new(
            Some(grandparent),
            TestViewportPosition {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
        );

        // Widget far outside the parent viewport
        let widget = TestWidget {
            x: 500,
            y: 500,
            w: 50,
            h: 50,
        };

        let (visible, dist) = parent.is_widget_visible(&widget);

        // Widget is not visible in parent, so result should be false
        // The grandparent check should be skipped (early return in is_widget_visible)
        assert!(!visible);
        assert!(dist > 0);
    }
}
