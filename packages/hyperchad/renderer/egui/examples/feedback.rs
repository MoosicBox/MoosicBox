//! Native production feedback fixture. Capture is opt-in via HYPERCHAD_FEEDBACK_OUTPUT.
//! HYPERCHAD_FEEDBACK_READY selects a readiness marker. HYPERCHAD_FEEDBACK_BASELINE
//! enables strict comparison; HYPERCHAD_FEEDBACK_ARTIFACTS selects comparison output.

#[path = "../tests/feedback/driver.rs"]
mod driver;
#[path = "../tests/feedback/fixture.rs"]
mod fixture;
#[path = "../tests/feedback/screenshot.rs"]
mod screenshot;

use hyperchad_renderer_egui::{
    eframe,
    feedback::{CaptureOptions, FeedbackApp},
};
use std::sync::{Arc, Mutex};

type Outcome = Arc<Mutex<Option<Result<std::path::PathBuf, String>>>>;

struct Native {
    app: FeedbackApp<driver::FixtureCalc>,
    outcome: Outcome,
}

impl eframe::App for Native {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        self.app.frame(ctx);
        if let Some(result) = self.app.capture_result() {
            *self.outcome.lock().unwrap() = Some(result.clone());
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::var_os("HYPERCHAD_EGUI_SCREENSHOT")
        .or_else(|| std::env::var_os("HYPERCHAD_FEEDBACK_OUTPUT"))
        .map(std::path::PathBuf::from);
    let capture = output.map(|output| CaptureOptions {
        output,
        ready: std::env::var_os("HYPERCHAD_EGUI_READY")
            .or_else(|| std::env::var_os("HYPERCHAD_FEEDBACK_READY"))
            .map(Into::into),
        required_ids: [
            fixture::BUTTON_ID,
            fixture::INPUT_ID,
            fixture::CHECKBOX_ID,
            fixture::LINK_ID,
        ]
        .map(str::to_owned)
        .into(),
        max_frames: 120,
        close: true,
    });
    let requested = capture.is_some();
    let mut view = fixture::view();
    if let Some(variant) = std::env::var_os("HYPERCHAD_FEEDBACK_VARIANT") {
        if variant != "alternate" {
            return Err(std::io::Error::other("unknown fixture variant").into());
        }
        fixture::set_background(&mut view, hyperchad_color::Color::from_hex("#702020"));
    }
    let outcome: Outcome = Arc::default();
    let result = outcome.clone();
    eframe::run_native(
        "HyperChad feedback",
        eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([fixture::WIDTH, fixture::HEIGHT])
                .with_resizable(false),
            ..Default::default()
        },
        Box::new(move |_cc| {
            let mut app = FeedbackApp::new(view, driver::FixtureCalc::default());
            if let Some(capture) = capture {
                app = app.with_capture(capture);
            }
            Ok(Box::new(Native { app, outcome }))
        }),
    )
    .map_err(|error| std::io::Error::other(error.to_string()))?;
    if requested {
        let path = result
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| std::io::Error::other("window closed before capture completed"))?
            .map_err(std::io::Error::other)?;
        eprintln!("feedback screenshot: {}", path.display());
        if let Some(baseline) = std::env::var_os("HYPERCHAD_FEEDBACK_BASELINE") {
            let actual = image::open(&path)?.into_rgba8();
            let artifacts = std::env::var_os("HYPERCHAD_FEEDBACK_ARTIFACTS")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    path.parent()
                        .unwrap_or_else(|| std::path::Path::new("."))
                        .join("comparison")
                });
            screenshot::compare(&actual, std::path::Path::new(&baseline), &artifacts, 0)?;
        }
    }
    Ok(())
}
