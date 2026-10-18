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
            assert!(buffer.get_ref().bytes.is_empty());
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
