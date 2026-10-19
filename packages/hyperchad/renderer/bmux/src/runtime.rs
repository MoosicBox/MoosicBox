//! Standalone terminal ownership and cancellation-safe runtime orchestration.

use std::{
    io::{self, BufWriter, Write},
    sync::{Arc, Mutex},
};

use bmux_tui::{
    crossterm::{CrosstermTerminalGuard, terminal_size},
    event::Event,
    terminal::Terminal,
};
use bmux_tui_runtime::{ManagedTerminalInput, Runtime, RuntimeError};
use hyperchad_renderer::RenderRunner;

use crate::{
    presentation::BmuxPresenter,
    program::{BmuxProgram, Message},
    renderer::{Error, Shared},
};

// Buffer protocol fragments, not whole frames. Unlike BufWriter's default Drop,
// never retry a failed frame after protocol cleanup or terminal restoration.
struct FrameWriter<W: Write> {
    inner: Option<BufWriter<W>>,
}

impl<W: Write> FrameWriter<W> {
    fn new(writer: W) -> Self {
        Self {
            inner: Some(BufWriter::with_capacity(64 * 1024, writer)),
        }
    }

    fn discard_pending(&mut self) {
        let (writer, _) = self.inner.take().expect("owned frame writer").into_parts();
        self.inner = Some(BufWriter::with_capacity(64 * 1024, writer));
    }
}

impl<W: Write> Write for FrameWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let result = self
            .inner
            .as_mut()
            .expect("owned frame writer")
            .write(bytes);
        if result.is_err() {
            self.discard_pending();
        }
        result
    }

    fn flush(&mut self) -> io::Result<()> {
        let result = self.inner.as_mut().expect("owned frame writer").flush();
        if result.is_err() {
            self.discard_pending();
        }
        result
    }
}

impl<W: Write> Drop for FrameWriter<W> {
    fn drop(&mut self) {
        if let Some(buffer) = self.inner.take() {
            // Explicit presenter flush owns publication; Drop must not publish.
            let _ = buffer.into_parts();
        }
    }
}

/// Single-use standalone runner. Terminal ownership never belongs to renderer clones.
pub struct BmuxRenderRunner {
    shared: Arc<Shared>,
    receiver: Mutex<Option<flume::Receiver<Message>>>,
}

impl BmuxRenderRunner {
    pub(crate) const fn new(shared: Arc<Shared>, receiver: flume::Receiver<Message>) -> Self {
        Self {
            shared,
            receiver: Mutex::new(Some(receiver)),
        }
    }
}

impl RenderRunner for BmuxRenderRunner {
    fn run(&mut self) -> Result<(), Box<dyn std::error::Error + Send>> {
        let receiver = self
            .receiver
            .lock()
            .map_err(|_| -> Box<dyn std::error::Error + Send> {
                Box::new(io::Error::other("bmux runner ownership poisoned"))
            })?
            .take()
            .ok_or_else(|| -> Box<dyn std::error::Error + Send> {
                Box::new(io::Error::other("bmux runner already consumed"))
            })?;
        let shared = Arc::clone(&self.shared);
        // Always cross an OS-thread boundary: App may invoke run from inside Tokio.
        // A private executor also prevents shutdown from stopping the application's runtime.
        let thread = std::thread::Builder::new()
            .name("hyperchad-bmux".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()?;
                runtime.block_on(run_terminal(shared, receiver))
            })
            .map_err(|error| -> Box<dyn std::error::Error + Send> { Box::new(error) })?;
        thread
            .join()
            .map_err(|_| -> Box<dyn std::error::Error + Send> {
                Box::new(io::Error::other("bmux terminal thread panicked"))
            })?
            .map_err(|error| -> Box<dyn std::error::Error + Send> { error })
    }
}

// Standalone ownership reserves quit chords; embedded hosts retain input policy.
const fn is_quit_event(event: &Event) -> bool {
    matches!(event, Event::Key(key)
        if key.modifiers.ctrl
            && !key.modifiers.alt
            && !key.modifiers.super_key
            && !key.modifiers.hyper
            && !key.modifiers.meta
            && matches!(key.key, bmux_keyboard::KeyCode::Char('c' | 'C' | 'q' | 'Q')))
}

async fn run_terminal(
    shared: Arc<Shared>,
    receiver: flume::Receiver<Message>,
) -> Result<(), Error> {
    // Guard remains in this stack through every success, failure and unwinding path.
    let guard = CrosstermTerminalGuard::enter(io::stdout())?;
    let size = terminal_size()?;
    let mut program = BmuxProgram::from_shared(Arc::clone(&shared));
    program.resize(size);
    let (writer, timing) = crate::presentation::TimedWriter::new(io::stdout());
    let terminal = Terminal::new(
        FrameWriter::new(writer),
        bmux_tui::geometry::Rect::new(0, 0, size.width, size.height),
    );
    let mut presenter = BmuxPresenter::new(terminal, &shared.config);
    presenter.set_writer_timing(timing);
    let (runtime, handle) = Runtime::new(program, presenter, shared.config.runtime);
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
    let mut ingress_stop = stop_rx.clone();
    let ingress_handle = handle.clone();
    let ingress = tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = ingress_stop.changed() => break,
                message = receiver.recv_async() => {
                    let Ok(message) = message else { break; };
                    tokio::select! {
                        biased;
                        _ = ingress_stop.changed() => break,
                        result = ingress_handle.send(message) => if result.is_err() { break; },
                    }
                }
            }
        }
    });
    let mut input_stop = stop_rx;
    let input = tokio::spawn(async move {
        let mut reader = ManagedTerminalInput::start(64);
        loop {
            tokio::select! {
                biased;
                _ = input_stop.changed() => break,
                event = reader.recv() => {
                    let event = match event {
                        Some(Ok(Some(event))) => event,
                        Some(Ok(None)) => continue,
                        Some(Err(error)) => {
                            let _ = handle.send(Message::InputFailed(error.to_string())).await;
                            break;
                        }
                        None => {
                            let _ = handle.send(Message::Shutdown).await;
                            break;
                        }
                    };
                    if is_quit_event(&event) {
                        shared.stopping.store(true, std::sync::atomic::Ordering::Release);
                        let _ = handle.send(Message::Quit).await;
                        break;
                    }
                    // Keep all input ordered: coalescing resize past a click changes meaning.
                    let oversized = matches!(&event, Event::Paste(text) if text.len() > shared.config.max_event_bytes);
                    if oversized { continue; }
                    tokio::select! {
                        biased;
                        _ = input_stop.changed() => break,
                        result = handle.send_terminal(event) => if result.is_err() { break; },
                    }
                }
            }
        }
        reader.shutdown().await;
    });
    let result = Box::pin(runtime.run()).await;
    let _ = stop_tx.send(true);
    let ingress_result = ingress.await;
    let input_result = input.await;
    // Runtime errors retain ownership, allowing the same cleanup as a graceful exit.
    let (mut output, primary): (_, Option<Error>) = match result {
        Ok(output) => (output, None),
        Err(
            RuntimeError::Program { error, output } | RuntimeError::Presenter { error, output },
        ) => (output, Some(error)),
    };
    let cleanup = output.presenter.cleanup();
    let leave = guard.leave();
    if let Some(error) = primary {
        if let Err(cleanup_error) = cleanup {
            log::error!("bmux image cleanup after {error}: {cleanup_error}");
        }
        if let Err(leave_error) = leave {
            log::error!("bmux terminal restoration after {error}: {leave_error}");
        }
        return Err(error);
    }
    cleanup?;
    leave?;
    ingress_result?;
    input_result?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmux_keyboard::{KeyCode, KeyStroke, Modifiers};

    /// Exercises the real runtime admission/scheduling path, unlike the direct
    /// `BmuxProgram::update` performance fixture. All input is admitted from the
    /// first presenter call, after real geometry has been committed.
    #[tokio::test]
    #[expect(
        clippy::too_many_lines,
        reason = "self-contained deterministic runtime fixture, presenter and admission assertions"
    )]
    async fn runtime_mixed_flood_preserves_click_targets_wheels_and_frame_progress() {
        use bmux_tui::{
            event::{MouseButton, MouseEvent, MouseEventKind},
            geometry::{Point, Rect, Size},
        };
        use bmux_tui_runtime::{
            LatestSendOutcome, PresentReport, Presenter, ResetReason, RuntimeConfig, RuntimeHandle,
        };
        use hyperchad_actions::{Action, ActionTrigger, ActionType};
        use hyperchad_renderer::View;
        use hyperchad_transformer::{
            Container, Number,
            models::{LayoutDirection, LayoutOverflow},
        };

        const ROUNDS: usize = 32;
        const APPLICATION_MESSAGES: usize = 64;
        const INPUT_EVENTS: usize = ROUNDS * 5;

        struct FloodPresenter {
            inner: BmuxPresenter<Vec<u8>>,
            handle: flume::Receiver<RuntimeHandle<Message>>,
            admitted: Option<RuntimeHandle<Message>>,
            terminal_progress: Vec<u64>,
            sizes: Vec<Size>,
        }

        impl Presenter<BmuxProgram> for FloodPresenter {
            type Error = Error;

            fn resize(&mut self, size: Size) {
                self.sizes.push(size);
                self.inner.resize(size);
            }

            fn reset(&mut self, reason: ResetReason) {
                self.inner.reset(reason);
            }

            fn present(&mut self, program: &mut BmuxProgram) -> Result<PresentReport, Error> {
                let report = self.inner.present(program)?;
                if self.admitted.is_none() {
                    let handle = self.handle.try_recv().expect("handle installed before run");
                    // A bounded application flood competes with the independent
                    // terminal FIFO. Neither application work nor latest resize
                    // admission may reorder clicks around individual wheels.
                    for _ in 0..APPLICATION_MESSAGES {
                        assert!(handle.try_send(Message::Background(None)).is_ok());
                    }
                    for round in 0..ROUNDS {
                        for kind in [
                            MouseEventKind::Down(MouseButton::Left),
                            MouseEventKind::Up(MouseButton::Left),
                            MouseEventKind::ScrollDown,
                            MouseEventKind::Down(MouseButton::Left),
                            MouseEventKind::Up(MouseButton::Left),
                        ] {
                            handle
                                .try_send_terminal(Event::Mouse(MouseEvent::new(
                                    kind,
                                    Point::new(1, 1),
                                )))
                                .unwrap();
                        }
                        assert_eq!(
                            handle
                                .send_latest_terminal(Event::Resize(Size::new(
                                    81 + u16::try_from(round).unwrap(),
                                    24,
                                )))
                                .unwrap(),
                            if round == 0 {
                                LatestSendOutcome::Inserted
                            } else {
                                LatestSendOutcome::Replaced
                            }
                        );
                    }
                    assert!(
                        handle
                            .send_latest_terminal(Event::Mouse(MouseEvent::new(
                                MouseEventKind::ScrollDown,
                                Point::new(1, 1),
                            )))
                            .is_err(),
                        "wheel input must not be admitted to the coalescing lane"
                    );
                    self.admitted = Some(handle);
                }
                self.terminal_progress
                    .push(self.admitted.as_ref().unwrap().stats().terminal_processed);
                Ok(report)
            }
        }

        let config = crate::renderer::BmuxConfig::default();
        let (action_tx, action_rx) = flume::unbounded();
        let (resize_tx, _resize_rx) = flume::unbounded();
        let mut program = BmuxProgram::new(config.clone(), action_tx, resize_tx);
        program.resize(Size::new(80, 24));
        program
            .apply_view(
                View::from(Container {
                    str_id: Some("scroll".into()),
                    direction: LayoutDirection::Column,
                    overflow_y: LayoutOverflow::Scroll,
                    width: Some(Number::Integer(640)),
                    height: Some(Number::Integer(384)),
                    children: (0..256)
                        .map(|row| Container {
                            str_id: Some(format!("row-{row}")),
                            width: Some(Number::Integer(640)),
                            height: Some(Number::Integer(16)),
                            actions: vec![Action {
                                trigger: ActionTrigger::Click,
                                effect: ActionType::Custom {
                                    action: format!("row-{row}"),
                                }
                                .into(),
                            }],
                            ..Container::default()
                        })
                        .collect(),
                    ..Container::default()
                }),
                None,
            )
            .unwrap();
        let (handle_tx, handle_rx) = flume::bounded(1);
        let presenter = FloodPresenter {
            inner: BmuxPresenter::new(Terminal::new(Vec::new(), Rect::new(0, 0, 80, 24)), &config),
            handle: handle_rx,
            admitted: None,
            terminal_progress: Vec::new(),
            sizes: Vec::new(),
        };
        let (runtime, handle) = Runtime::new(
            program,
            presenter,
            RuntimeConfig {
                reliable_capacity: APPLICATION_MESSAGES + 8,
                terminal_capacity: INPUT_EVENTS,
                messages_per_turn: 1,
                frame_interval: None,
                ..RuntimeConfig::default()
            },
        );
        handle_tx.send(handle.clone()).unwrap();
        // Start through a ready turn rather than the runtime's blocking receive
        // branch, so admission counters cover the entire subsequent flood.
        assert!(handle.try_send(Message::Background(None)).is_ok());
        let observed = Arc::new(Mutex::new(Vec::new()));
        let observed_actions = observed.clone();
        let diagnostics = handle.clone();
        let observe = async move {
            let mut actions = Vec::new();
            for _ in 0..ROUNDS * 2 {
                let action = action_rx.recv_async().await.unwrap().0;
                observed_actions.lock().unwrap().push(action.clone());
                actions.push(action);
            }
            // Synchronize on effects, not a sleep or guessed runtime duration.
            // Graceful shutdown lets the last custom completion settle.
            handle.send(Message::Shutdown).await.unwrap();
            (actions, action_rx)
        };
        // Timeout is solely a deadlock guard: correctness/progress is checked
        // below in events and scheduler turns, never milliseconds.
        let (output, (actions, action_rx)) = Box::pin(tokio::time::timeout(
            std::time::Duration::from_secs(30),
            async { tokio::join!(Box::pin(runtime.run()), observe) },
        ))
        .await
        .unwrap_or_else(|_| {
            panic!(
                "runtime flood stalled: {:?}; actions={:?}",
                diagnostics.stats(),
                observed.lock().unwrap()
            )
        });
        let output = output.unwrap_or_else(|error| match error {
            RuntimeError::Program { error, .. } | RuntimeError::Presenter { error, .. } => {
                panic!("headless runtime failed: {error}")
            }
        });
        let expected: Vec<_> = (0..ROUNDS)
            .flat_map(|round| {
                [
                    format!("row-{}", 1 + round * 3),
                    format!("row-{}", 4 + round * 3),
                ]
            })
            .collect();
        assert_eq!(
            actions, expected,
            "clicks must straddle every individual wheel in FIFO order"
        );
        assert!(
            action_rx.is_empty(),
            "no duplicate clicks may remain after shutdown"
        );
        assert_eq!(
            output.stats.terminal_processed,
            u64::try_from(INPUT_EVENTS + 1).unwrap()
        );
        assert_eq!(
            output.stats.latest_replaced,
            u64::try_from(ROUNDS - 1).unwrap()
        );
        assert_eq!(output.stats.terminal_rejected, 0);
        assert_eq!(output.stats.reliable_rejected, 0);
        assert_eq!(output.stats.latest_rejected, 0);
        assert_eq!(output.stats.terminal_high_water, INPUT_EVENTS);
        assert!(
            output.stats.reliable_processed >= u64::try_from(APPLICATION_MESSAGES + 1).unwrap()
        );
        assert!(output.stats.scheduler_budget_exhausted > 0);
        assert_eq!(output.presenter.sizes.last(), Some(&Size::new(112, 24)));
        let progress = &output.presenter.terminal_progress;
        assert_eq!(progress.first(), Some(&0));
        assert_eq!(progress.last(), Some(&output.stats.terminal_processed));
        assert!(
            progress
                .windows(2)
                .all(|pair| pair[1] >= pair[0] && pair[1] - pair[0] <= 1),
            "a one-message turn must present before admitting the next terminal event"
        );
        assert!(output.stats.frames_presented >= u64::try_from(INPUT_EVENTS + 1).unwrap());
    }

    #[derive(Default)]
    struct OutputProbe {
        bytes: Vec<u8>,
        writes: usize,
        flushes: usize,
        fail_after: Option<usize>,
        fail_flush: bool,
    }

    impl Write for OutputProbe {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            let count = self.fail_after.map_or(bytes.len(), |limit| {
                limit.saturating_sub(self.bytes.len()).min(bytes.len())
            });
            if count == 0 {
                return Err(io::Error::other("test partial write"));
            }
            self.bytes.extend_from_slice(&bytes[..count]);
            Ok(count)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            if self.fail_flush {
                return Err(io::Error::other("test flush"));
            }
            Ok(())
        }
    }

    #[test]
    fn frame_buffer_preserves_bytes_flush_boundary_and_bounded_capacity() {
        let mut probe = OutputProbe::default();
        {
            let mut output = FrameWriter::new(&mut probe);
            for _ in 0..2000 {
                output.write_all(b"\x1b[1;2Hpixel").unwrap();
            }
            let buffer = output.inner.as_ref().unwrap();
            assert_eq!(buffer.get_ref().bytes.len(), 0);
            assert_eq!(buffer.capacity(), 64 * 1024);
            output.flush().unwrap();
            let inner = output.inner.as_ref().unwrap().get_ref();
            assert_eq!(inner.bytes, b"\x1b[1;2Hpixel".repeat(2000));
            assert_eq!(inner.writes, 1);
            assert_eq!(inner.flushes, 1);
            output.write_all(&vec![b'x'; 128 * 1024]).unwrap();
            output.flush().unwrap();
        }
        assert_eq!(
            probe.bytes.len(),
            b"\x1b[1;2Hpixel".len() * 2000 + 128 * 1024
        );
    }

    #[test]
    fn failed_buffer_is_not_replayed_during_cleanup_or_drop() {
        let mut probe = OutputProbe {
            fail_after: Some(3),
            ..OutputProbe::default()
        };
        {
            let mut output = FrameWriter::new(&mut probe);
            output.write_all(b"failed-frame").unwrap();
            assert!(output.flush().is_err());
            output.inner.as_mut().unwrap().get_mut().fail_after = None;
            output.write_all(b"cleanup").unwrap();
            output.flush().unwrap();
            // Even an unflushed tail must never escape after terminal restoration.
            output.write_all(b"discard-on-drop").unwrap();
        }
        assert_eq!(probe.bytes, b"faicleanup");
    }

    #[test]
    fn underlying_flush_failure_propagates_without_duplicate_output() {
        let mut probe = OutputProbe {
            fail_flush: true,
            ..OutputProbe::default()
        };
        {
            let mut output = FrameWriter::new(&mut probe);
            output.write_all(b"frame").unwrap();
            assert!(output.flush().is_err());
            output.inner.as_mut().unwrap().get_mut().fail_flush = false;
            output.write_all(b"cleanup").unwrap();
            output.flush().unwrap();
        }
        assert_eq!(probe.bytes, b"framecleanup");
    }

    #[test]
    fn quit_chords_are_reserved_only_with_control() {
        for character in ['c', 'C', 'q', 'Q'] {
            let key = KeyStroke::simple(KeyCode::Char(character));
            assert!(!is_quit_event(&Event::Key(key)));
            let mut key = key;
            key.modifiers = Modifiers {
                ctrl: true,
                ..Modifiers::NONE
            };
            assert!(is_quit_event(&Event::Key(key)));
            key.modifiers.alt = true;
            assert!(!is_quit_event(&Event::Key(key)));
        }
        assert!(!is_quit_event(&Event::Key(KeyStroke::simple(
            KeyCode::Escape
        ))));
        assert!(!is_quit_event(&Event::Paste("\u{3}\u{11}".into())));
    }
}
