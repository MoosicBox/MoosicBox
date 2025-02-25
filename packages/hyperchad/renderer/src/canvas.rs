#![allow(clippy::module_name_repetitions)]

use hyperchad_color::Color;

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct Pos(pub f32, pub f32);

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub enum CanvasAction {
    StrokeSize(f32),
    StrokeColor(Color),
    Line(Pos, Pos),
    FillRect(Pos, Pos),
    Clear,
    ClearRect(Pos, Pos),
}

impl CanvasAction {
    #[must_use]
    pub const fn is_draw_action(&self) -> bool {
        !matches!(
            self,
            Self::StrokeSize(..) | Self::StrokeColor(..) | Self::Clear | Self::ClearRect(..)
        )
    }
}

#[derive(Default, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
pub struct CanvasUpdate {
    pub target: String,
    pub canvas_actions: Vec<CanvasAction>,
}
