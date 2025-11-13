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
