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
    parent: Option<Box<Viewport>>,
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
