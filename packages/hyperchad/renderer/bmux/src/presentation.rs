//! Text/image presentation and host-owned embedding transactions.

use std::{
    io::{self, Write},
    sync::Arc,
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

/// Presenter which prepares layout before any output and commits it only after flush.
pub struct BmuxPresenter<W: Write> {
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
                inner: bmux_tui_runtime::TerminalPresenter::new(
                    terminal,
                    paint_program as fn(&mut BmuxProgram, &mut PaintCx<'_, '_>),
                ),
            }
        }
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
        match self.inner.present(program) {
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
