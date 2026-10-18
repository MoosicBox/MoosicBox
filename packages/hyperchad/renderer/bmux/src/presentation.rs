//! Text/image presentation and host-owned embedding transactions.

use std::{
    io::{self, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use bmux_tui::{
    event::Event,
    geometry::Size,
    paint::{LocalRect, PaintCx},
    terminal::Terminal,
};
use bmux_tui_runtime::{PresentReport, Presenter, Program, ResetReason, RuntimeEvent, Update};

use crate::{
    program::{BmuxProgram, Message},
    renderer::{BmuxRenderer, Error},
};

type TimingHandle = Arc<Mutex<Option<WriterTiming>>>;

#[derive(Default)]
pub(crate) struct WriterTiming {
    bytes: usize,
    writes: usize,
    flushes: usize,
    write_time: Duration,
    max_write: Duration,
    flush_time: Duration,
}

/// Transparent writer instrumentation, active only during a debug presentation.
pub(crate) struct TimedWriter<W> {
    inner: W,
    timing: TimingHandle,
}
impl<W> TimedWriter<W> {
    pub(crate) fn new(inner: W) -> (Self, TimingHandle) {
        let timing = Arc::new(Mutex::new(None));
        (
            Self {
                inner,
                timing: Arc::clone(&timing),
            },
            timing,
        )
    }
}
impl<W: Write> Write for TimedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if !log::log_enabled!(log::Level::Debug) {
            return self.inner.write(bytes);
        }
        let start = Instant::now();
        let result = self.inner.write(bytes);
        let elapsed = start.elapsed();
        if let Ok(mut timing) = self.timing.lock()
            && let Some(timing) = timing.as_mut()
        {
            timing.writes += 1;
            timing.bytes = timing
                .bytes
                .saturating_add(result.as_ref().copied().unwrap_or(0));
            timing.write_time += elapsed;
            timing.max_write = timing.max_write.max(elapsed);
        }
        result
    }
    fn flush(&mut self) -> io::Result<()> {
        if !log::log_enabled!(log::Level::Debug) {
            return self.inner.flush();
        }
        let start = Instant::now();
        let result = self.inner.flush();
        let elapsed = start.elapsed();
        if let Ok(mut timing) = self.timing.lock()
            && let Some(timing) = timing.as_mut()
        {
            timing.flushes += 1;
            timing.flush_time += elapsed;
        }
        result
    }
}

/// Presenter which prepares layout before any output and commits it only after flush.
pub struct BmuxPresenter<W: Write> {
    timing: Option<TimingHandle>,
    #[cfg(feature = "images")]
    graphics_available: bool,
    #[cfg(feature = "images")]
    inner: bmux_tui_runtime::ImageTerminalPresenter<W, fn(&mut BmuxProgram, &mut PaintCx<'_, '_>)>,
    #[cfg(not(feature = "images"))]
    inner: bmux_tui_runtime::TerminalPresenter<W, fn(&mut BmuxProgram, &mut PaintCx<'_, '_>)>,
}

impl<W: Write> BmuxPresenter<W> {
    #[must_use]
    pub fn new(terminal: Terminal<W>, policy: &crate::renderer::BmuxConfig) -> Self {
        #[cfg(feature = "images")]
        {
            let mut capabilities = bmux_image::host_caps::detect_from_env();
            let selected = std::env::var("HYPERCHAD_BMUX_IMAGE_PROTOCOL").ok();
            let protocol = match selected.as_deref() {
                Some("kitty") => crate::renderer::ImageProtocol::Kitty,
                Some("sixel") => crate::renderer::ImageProtocol::Sixel,
                Some("iterm2") => crate::renderer::ImageProtocol::Iterm2,
                Some("none" | "disabled") => crate::renderer::ImageProtocol::Disabled,
                _ => policy.image_protocol,
            };
            if protocol != crate::renderer::ImageProtocol::Auto {
                capabilities.kitty_graphics = protocol == crate::renderer::ImageProtocol::Kitty;
                capabilities.sixel = protocol == crate::renderer::ImageProtocol::Sixel;
                capabilities.iterm2_inline = protocol == crate::renderer::ImageProtocol::Iterm2;
            }
            let config = bmux_image::config::ImageConfig {
                enabled: policy.graphics_enabled
                    && protocol != crate::renderer::ImageProtocol::Disabled,
                max_image_bytes: policy.graphics.max_resource_bytes,
                max_images_per_pane: policy.graphics.max_entries,
                ..Default::default()
            };
            Self {
                timing: None,
                graphics_available: config.enabled && capabilities.any_supported(),
                inner: bmux_tui_runtime::ImageTerminalPresenter::new(
                    terminal,
                    paint_program as fn(&mut BmuxProgram, &mut PaintCx<'_, '_>),
                    capabilities,
                    config,
                ),
            }
        }
        #[cfg(not(feature = "images"))]
        {
            let _ = policy;
            Self {
                timing: None,
                inner: bmux_tui_runtime::TerminalPresenter::new(
                    terminal,
                    paint_program as fn(&mut BmuxProgram, &mut PaintCx<'_, '_>),
                ),
            }
        }
    }

    pub(crate) fn set_writer_timing(&mut self, timing: TimingHandle) {
        self.timing = Some(timing);
    }

    /// Release protocol resources before returning the terminal to its owner.
    ///
    /// # Errors
    /// * Returns an error if terminal cleanup or flushing fails.
    pub fn cleanup(&mut self) -> Result<(), Error> {
        #[cfg(feature = "images")]
        self.inner.cleanup_images()?;
        Ok(())
    }
}

impl<W: Write> Presenter<BmuxProgram> for BmuxPresenter<W> {
    type Error = Error;

    fn resize(&mut self, size: Size) {
        self.inner.resize(size);
    }
    fn reset(&mut self, reason: ResetReason) {
        self.inner.reset(reason);
    }

    fn present(&mut self, program: &mut BmuxProgram) -> Result<PresentReport, Error> {
        #[cfg(feature = "images")]
        program.set_graphics_available(self.graphics_available);
        #[cfg(not(feature = "images"))]
        program.set_graphics_available(false);
        program.prepare()?;
        let measured = log::log_enabled!(log::Level::Debug) && self.timing.is_some();
        if measured
            && let Some(timing) = &self.timing
            && let Ok(mut timing) = timing.lock()
        {
            *timing = Some(WriterTiming::default());
        }
        let start = measured.then(Instant::now);
        let result = self.inner.present(program);
        if let Some(start) = start {
            let elapsed = start.elapsed();
            if let Some(timing) = &self.timing
                && let Ok(mut timing) = timing.lock()
                && let Some(timing) = timing.take()
            {
                log::debug!(
                    "bmux presenter output present_us={} write_us={} max_write_us={} flush_us={} bytes={} writes={} flushes={} success={}",
                    elapsed.as_micros(),
                    timing.write_time.as_micros(),
                    timing.max_write.as_micros(),
                    timing.flush_time.as_micros(),
                    timing.bytes,
                    timing.writes,
                    timing.flushes,
                    result.is_ok()
                );
            }
        }
        match result {
            Ok(report) => {
                program.commit_prepared();
                Ok(report)
            }
            Err(error) => {
                program.discard_prepared();
                Err(Box::new(error))
            }
        }
    }
}

fn paint_program(program: &mut BmuxProgram, cx: &mut PaintCx<'_, '_>) {
    program.paint(cx);
}

/// A host-driven interactive surface. It never owns a writer or reads terminal input.
/// The host must commit only after its enclosing output transaction succeeds.
pub struct EmbeddedSurface {
    pub program: BmuxProgram,
    receiver: flume::Receiver<Message>,
    next_ticket: u64,
    prepared_ticket: Option<u64>,
}

impl BmuxRenderer {
    /// Transfer the sole document owner to an embedding host.
    ///
    /// # Errors
    /// * Returns an error if a runner or another embedded owner already exists.
    pub fn into_embedded(self) -> Result<EmbeddedSurface, Error> {
        let receiver = self.take_receiver()?;
        Ok(EmbeddedSurface {
            program: BmuxProgram::from_shared(Arc::clone(&self.shared)),
            receiver,
            next_ticket: 0,
            prepared_ticket: None,
        })
    }
}

impl EmbeddedSurface {
    /// Apply at most one bounded runtime turn. The host schedules returned commands/subscriptions.
    ///
    /// # Errors
    /// * Returns document/action errors without committing candidate presentation state.
    pub fn drain(&mut self) -> Result<Update<Message>, Error> {
        self.discard();
        let mut update = Update::none();
        let start = std::time::Instant::now();
        for _ in 0..self.program.config().runtime.messages_per_turn.max(1) {
            let Ok(message) = self.receiver.try_recv() else {
                break;
            };
            update = update.merge(self.program.update(RuntimeEvent::Message(message))?);
            if start.elapsed() >= self.program.config().runtime.processing_time_per_turn {
                break;
            }
        }
        Ok(update)
    }

    /// Route local-coordinate input against the last successfully committed layout.
    ///
    /// # Errors
    /// * Returns an error when the input effect fails validation.
    pub fn event(&mut self, event: Event) -> Result<Update<Message>, Error> {
        self.discard();
        self.program.update(RuntimeEvent::Terminal(event))
    }

    /// Prepare a bounded scene for the supplied local viewport.
    ///
    /// # Errors
    /// * Returns a layout/resource error, or ticket exhaustion.
    pub fn prepare(&mut self, size: Size) -> Result<u64, Error> {
        self.program.resize(size);
        self.program.prepare()?;
        self.next_ticket = self
            .next_ticket
            .checked_add(1)
            .ok_or_else(|| io::Error::other("embedded presentation ticket exhausted"))?;
        self.prepared_ticket = Some(self.next_ticket);
        Ok(self.next_ticket)
    }

    /// Paint into the host's already translated/clipped context. The host compositor owns images.
    ///
    /// # Errors
    /// * Returns an error for an expired preparation ticket.
    pub fn paint(&mut self, ticket: u64, cx: &mut PaintCx<'_, '_>) -> Result<(), Error> {
        self.check(ticket)?;
        self.program.paint(cx);
        Ok(())
    }

    /// Publish input geometry after the host flush succeeds.
    ///
    /// # Errors
    /// * Returns an error for an expired preparation ticket.
    pub fn commit(&mut self, ticket: u64, report: PresentReport) -> Result<Update<Message>, Error> {
        self.check(ticket)?;
        self.program.commit_prepared();
        self.prepared_ticket = None;
        Ok(self.program.presentation_committed(report))
    }

    /// Discard a candidate after failed or cancelled host output.
    pub fn discard(&mut self) {
        self.prepared_ticket = None;
        self.program.discard_prepared();
    }

    fn check(&self, ticket: u64) -> Result<(), Error> {
        if self.prepared_ticket == Some(ticket) {
            Ok(())
        } else {
            Err(io::Error::other("stale embedded presentation ticket").into())
        }
    }
}

/// Paint only the authoritative layout's preformatted text; no geometry is recomputed.
pub(crate) fn paint_text(
    tree: &crate::layout::LayoutTree,
    controls: &crate::controls::Controls,
    cx: &mut PaintCx<'_, '_>,
) {
    for node in &tree.nodes {
        if !node.visible {
            continue;
        }
        let clip = node.text_clip.terminal();
        cx.with_child(
            0,
            0,
            LocalRect::new(
                i32::from(clip.x),
                i64::from(clip.y),
                clip.width,
                clip.height,
            ),
            |cx| {
                let bounds = node.bounds;
                if node.style.style.bg.is_some() {
                    cx.fill(
                        LocalRect::new(
                            i32::try_from(bounds.x).unwrap_or(if bounds.x < 0 {
                                i32::MIN
                            } else {
                                i32::MAX
                            }),
                            bounds.y,
                            u16::try_from(bounds.width).unwrap_or(u16::MAX),
                            u16::try_from(bounds.height).unwrap_or(u16::MAX),
                        ),
                        " ",
                        node.style.style,
                    );
                }
                for fragment in &node.text {
                    if fragment.visible {
                        cx.write_line(
                            LocalRect::new(
                                i32::try_from(fragment.bounds.x).unwrap_or(
                                    if fragment.bounds.x < 0 {
                                        i32::MIN
                                    } else {
                                        i32::MAX
                                    },
                                ),
                                fragment.bounds.y,
                                u16::try_from(fragment.bounds.width).unwrap_or(u16::MAX),
                                1,
                            ),
                            &bmux_tui::text::Line::from_spans(vec![bmux_tui::text::Span::styled(
                                fragment.text.clone(),
                                fragment
                                    .sources
                                    .iter()
                                    .fold(fragment.style, |style, source| {
                                        controls.inline_style(&source.path, style)
                                    }),
                            )]),
                        );
                    }
                }
            },
        );
    }
}

#[cfg(test)]
mod writer_tests {
    use super::*;

    #[derive(Default)]
    struct PartialWriter {
        bytes: Vec<u8>,
        fail: bool,
        flushes: usize,
    }
    impl Write for PartialWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if self.fail {
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "test write"));
            }
            let count = bytes.len().min(2);
            self.bytes.extend_from_slice(&bytes[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if self.fail {
                return Err(io::Error::other("test flush"));
            }
            Ok(())
        }
    }
    #[test]
    fn timed_writer_preserves_partial_writes_and_write_all() {
        let (mut writer, _) = TimedWriter::new(PartialWriter::default());
        assert_eq!(writer.write(b"abcd").unwrap(), 2);
        writer.write_all(b"efghi").unwrap();
        writer.flush().unwrap();
        assert_eq!(writer.inner.bytes, b"abefghi");
        assert_eq!(writer.inner.flushes, 1);
    }
    #[test]
    fn timed_writer_preserves_write_and_flush_errors() {
        let (mut writer, _) = TimedWriter::new(PartialWriter {
            fail: true,
            ..Default::default()
        });
        let error = writer.write(b"payload").unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(error.to_string(), "test write");
        let error = writer.flush().unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(error.to_string(), "test flush");
        assert_eq!(writer.inner.bytes.len(), 0);
        assert_eq!(writer.inner.flushes, 1);
    }
}
