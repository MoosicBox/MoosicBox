//! Repeatable input driver around the production context-only frame seam.

use eframe::egui;
#[cfg(test)]
use hyperchad_renderer_egui::feedback::FeedbackApp;
use hyperchad_renderer_egui::{font_metrics::EguiFontMetrics, layout::EguiCalc};
use hyperchad_transformer::{
    Container,
    layout::{
        Calc,
        calc::{Calculator, CalculatorDefaults},
    },
};

#[derive(Clone, Default)]
pub struct FixtureCalc(Option<egui::Context>);

impl EguiCalc for FixtureCalc {
    fn with_context(self, context: egui::Context) -> Self {
        Self(Some(context))
    }
}

impl Calc for FixtureCalc {
    fn calc(&self, container: &mut Container) -> bool {
        Calculator::new(
            EguiFontMetrics::new(
                self.0
                    .clone()
                    .expect("production must initialize calculator context"),
            ),
            CalculatorDefaults {
                font_size: 14.0,
                font_margin_top: 0.0,
                font_margin_bottom: 0.0,
                h1_font_size: 28.0,
                h1_font_margin_top: 0.0,
                h1_font_margin_bottom: 0.0,
                h2_font_size: 24.0,
                h2_font_margin_top: 0.0,
                h2_font_margin_bottom: 0.0,
                h3_font_size: 20.0,
                h3_font_margin_top: 0.0,
                h3_font_margin_bottom: 0.0,
                h4_font_size: 18.0,
                h4_font_margin_top: 0.0,
                h4_font_margin_bottom: 0.0,
                h5_font_size: 16.0,
                h5_font_margin_top: 0.0,
                h5_font_margin_bottom: 0.0,
                h6_font_size: 14.0,
                h6_font_margin_top: 0.0,
                h6_font_margin_bottom: 0.0,
            },
        )
        .calc(container)
    }
}

#[cfg(test)]
pub struct Driver {
    pub app: FeedbackApp<FixtureCalc>,
    pub ctx: egui::Context,
    frame: u32,
}

#[cfg(test)]
impl Driver {
    pub fn new(view: Container) -> Self {
        Self {
            app: FeedbackApp::new(view, FixtureCalc::default()),
            ctx: egui::Context::default(),
            frame: 0,
        }
    }

    pub fn step(&mut self, events: Vec<egui::Event>) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(super::fixture::WIDTH, super::fixture::HEIGHT),
            )),
            time: Some(f64::from(self.frame) / 60.0),
            predicted_dt: 1.0 / 60.0,
            events,
            focused: true,
            ..Default::default()
        };
        self.frame += 1;
        self.ctx.run(input, |ctx| self.app.frame(ctx))
    }

    pub fn settle(&mut self) {
        let required = [
            super::fixture::BUTTON_ID,
            super::fixture::INPUT_ID,
            super::fixture::CHECKBOX_ID,
            super::fixture::LINK_ID,
        ]
        .map(str::to_owned);
        for _ in 0..12 {
            self.step(vec![]);
            if self.app.ready(&self.ctx, &required) {
                return;
            }
        }
        panic!(
            "production fixture failed readiness: {:?}",
            self.app.geometry()
        );
    }

    pub fn click(&mut self, id: &str) {
        let rect = self
            .app
            .geometry()
            .get(id)
            .copied()
            .unwrap_or_else(|| panic!("missing semantic geometry {id}"));
        assert!(rect.is_finite() && rect.is_positive());
        let pos = rect.center();
        self.step(vec![egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            self.step(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }]);
        }
    }
}
