#![allow(clippy::module_name_repetitions)]

#[derive(Debug, Clone)]
pub enum CanvasAction {
    Line,
}

#[derive(Default, Debug, Clone)]
pub struct CanvasUpdate {
    pub target: String,
    pub canvas_actions: Vec<CanvasAction>,
}
