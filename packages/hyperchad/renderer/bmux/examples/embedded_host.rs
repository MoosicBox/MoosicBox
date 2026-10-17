//! A host-owned offscreen transaction: no raw mode, input reader, or terminal writer.
mod support;

use bmux_tui::{
    buffer::Buffer,
    frame::Frame,
    geometry::{Rect, Size},
    paint::{LocalRect, PaintCx},
};
use bmux_tui_runtime::PresentReport;
use hyperchad_renderer::{Renderer, View};
use hyperchad_renderer_bmux::{BmuxRenderer, renderer::BmuxConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (actions, _action_rx) = flume::bounded(64);
    let (resizes, _resize_rx) = flume::bounded(64);
    let renderer = BmuxRenderer::new(BmuxConfig::default(), actions, resizes);
    let mut surface = renderer.clone().into_embedded()?;
    renderer
        .render(View::from(support::document(support::Workflow::Documents)))
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    // This document has no effects. A live host must execute returned commands,
    // schedule subscriptions, and deliver their messages back to the program.
    let _update = surface.drain()?;
    let ticket = surface.prepare(Size::new(80, 32))?;
    let mut buffer = Buffer::empty(Rect::new(0, 0, 100, 40));
    let mut frame = Frame::new(&mut buffer);
    let mut paint = PaintCx::new(&mut frame);
    let mut paint_result = Ok(());
    paint.with_child(10, 4, LocalRect::new(0, 0, 80, 32), |child| {
        paint_result = surface.paint(ticket, child);
    });
    paint_result?;
    // The host must submit frame.images() to its own image compositor together
    // with the cell buffer. Here the destination is an in-memory host surface.
    let _images = frame.images();
    let _committed = surface.commit(
        ticket,
        PresentReport {
            full_repaint: true,
            changed_cells: 0,
        },
    )?;
    // Stale tickets are rejected; failed output must call discard, not commit.
    assert!(surface.commit(ticket, PresentReport::default()).is_err());
    Ok(())
}
