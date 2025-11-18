//! Canvas drawing operations and update events.
//!
//! This module provides types for performing 2D canvas drawing operations, including
//! lines, rectangles, and clearing operations. Canvas updates can be sent to renderers
//! to update specific canvas elements.
//!
//! # Examples
//!
//! Creating a canvas update with drawing operations:
//!
//! ```rust
//! # #[cfg(feature = "canvas")]
//! # {
//! use hyperchad_renderer::canvas::{CanvasUpdate, CanvasAction, Pos};
//! use hyperchad_renderer::Color;
//!
//! let update = CanvasUpdate {
//!     target: "my-canvas".to_string(),
//!     canvas_actions: vec![
//!         CanvasAction::StrokeColor(Color { r: 255, g: 0, b: 0, a: None }),
//!         CanvasAction::StrokeSize(2.0),
//!         CanvasAction::Line(Pos(0.0, 0.0), Pos(100.0, 100.0)),
//!     ],
//! };
//! # }
//! ```

#![allow(clippy::module_name_repetitions)]

use hyperchad_color::Color;

/// 2D position with x and y coordinates
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Pos(
    /// X coordinate
    pub f32,
    /// Y coordinate
    pub f32,
);

/// Actions that can be performed on a canvas
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub enum CanvasAction {
    /// Set the stroke width
    StrokeSize(f32),
    /// Set the stroke color
    StrokeColor(Color),
    /// Draw a line from first position to second position
    Line(Pos, Pos),
    /// Fill a rectangle from first position to second position
    FillRect(Pos, Pos),
    /// Clear the entire canvas
    Clear,
    /// Clear a rectangular area from first position to second position
    ClearRect(Pos, Pos),
}

impl CanvasAction {
    /// Returns whether this action performs drawing (as opposed to configuration or clearing).
    ///
    /// Drawing actions include lines and filled rectangles, while configuration actions
    /// like setting stroke size/color and clearing operations return `false`.
    #[must_use]
    pub const fn is_draw_action(&self) -> bool {
        !matches!(
            self,
            Self::StrokeSize(..) | Self::StrokeColor(..) | Self::Clear | Self::ClearRect(..)
        )
    }
}

/// Update to apply to a canvas element
#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct CanvasUpdate {
    /// Target canvas element identifier
    pub target: String,
    /// Actions to perform on the canvas
    pub canvas_actions: Vec<CanvasAction>,
}
