//! Standalone terminal ownership and cancellation-safe runtime orchestration.

use std::{
    io,
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

async fn run_terminal(
    shared: Arc<Shared>,
    receiver: flume::Receiver<Message>,
) -> Result<(), Error> {
    // Guard remains in this stack through every success, failure and unwinding path.
    let guard = CrosstermTerminalGuard::enter(io::stdout())?;
    let size = terminal_size()?;
    let mut program = BmuxProgram::from_shared(Arc::clone(&shared));
    program.resize(size);
    let terminal = Terminal::new(
        io::stdout(),
        bmux_tui::geometry::Rect::new(0, 0, size.width, size.height),
    );
    let presenter = BmuxPresenter::new(terminal, &shared.config);
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
