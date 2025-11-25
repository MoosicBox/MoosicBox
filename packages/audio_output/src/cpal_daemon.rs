//! CPAL Stream Daemon - manages !Send CPAL streams in dedicated threads
//!
//! This solves the macOS !Send issue by keeping the CPAL stream in its own thread
//! and providing a Send+Sync handle for external control.

use std::sync::Arc;

use crate::resource_daemon::{DaemonState, QuitSignal, ResourceDaemon};

// Import CPAL traits for stream control methods
use ::cpal::traits::StreamTrait;

/// Commands that can be sent to control the CPAL stream
#[derive(Debug, Clone)]
pub enum StreamCommand {
    /// Pause the stream
    Pause,
    /// Resume playback
    Resume,
    /// Reset the stream (pause it)
    Reset,
    /// Set the volume to the specified level (0.0 to 1.0)
    SetVolume(f64),
}

/// Response from stream command execution
#[derive(Debug, Clone)]
pub enum StreamResponse {
    /// Command executed successfully
    Success,
    /// Command execution failed with an error message
    Error(String),
}

/// Error type for stream daemon operations
#[derive(Debug, Clone)]
pub enum StreamDaemonError {
    /// Stream creation failed with the given error message
    StreamCreationFailed(String),
    /// Stream operation failed with the given error message
    StreamOperationFailed(String),
    /// The daemon has stopped and is no longer accepting commands
    DaemonStopped,
}

/// A Send+Sync handle for controlling a CPAL stream that lives in a dedicated thread
#[derive(Debug)]
pub struct CpalStreamDaemon {
    daemon: ResourceDaemon<(), StreamDaemonError>,
    // Quit channel sender for immediate shutdown
    shutdown_sender: Option<flume::Sender<()>>,
}

/// Handle for controlling the CPAL stream from external threads.
///
/// This handle provides a thread-safe way to send commands to a CPAL stream
/// that lives in a dedicated daemon thread, solving the `!Send` issue on macOS.
#[derive(Debug, Clone)]
pub struct StreamHandle {
    command_sender: flume::Sender<(StreamCommand, flume::Sender<StreamResponse>)>,
}

impl StreamHandle {
    /// Pauses the audio stream.
    ///
    /// # Errors
    ///
    /// * If the stream fails to pause
    pub async fn pause(&self) -> Result<(), StreamDaemonError> {
        log::debug!("StreamHandle: sending pause command");
        self.send_command(StreamCommand::Pause).await
    }

    /// Resumes the audio stream.
    ///
    /// # Errors
    ///
    /// * If the stream fails to resume
    pub async fn resume(&self) -> Result<(), StreamDaemonError> {
        log::debug!("StreamHandle: sending resume command");
        self.send_command(StreamCommand::Resume).await
    }

    /// Resets the audio stream by pausing it.
    ///
    /// # Errors
    ///
    /// * If the stream fails to reset
    pub async fn reset(&self) -> Result<(), StreamDaemonError> {
        log::debug!("StreamHandle: sending reset command");
        self.send_command(StreamCommand::Reset).await
    }

    /// Sets the volume level (0.0 to 1.0).
    ///
    /// This is handled by the volume atomic, not the stream directly.
    ///
    /// # Errors
    ///
    /// * If the stream fails to set the volume
    pub async fn set_volume(&self, volume: f64) -> Result<(), StreamDaemonError> {
        self.send_command(StreamCommand::SetVolume(volume)).await
    }

    async fn send_command(&self, command: StreamCommand) -> Result<(), StreamDaemonError> {
        let (response_tx, response_rx) = flume::unbounded();

        self.command_sender
            .send_async((command, response_tx))
            .await
            .map_err(|_| StreamDaemonError::DaemonStopped)?;

        match response_rx.recv_async().await {
            Ok(StreamResponse::Success) => Ok(()),
            Ok(StreamResponse::Error(err)) => Err(StreamDaemonError::StreamOperationFailed(err)),
            Err(_) => Err(StreamDaemonError::DaemonStopped),
        }
    }
}

impl CpalStreamDaemon {
    /// Create a new CPAL stream daemon
    ///
    /// The `stream_factory` function will be called in the daemon thread to create the stream.
    /// The `volume_atomic` will be used for volume control.
    ///
    /// # Errors
    ///
    /// * If the stream creation fails
    pub fn new<F>(
        stream_factory: F,
        volume_atomic: Arc<std::sync::RwLock<Arc<atomic_float::AtomicF64>>>,
    ) -> Result<(Self, StreamHandle), StreamDaemonError>
    where
        F: FnOnce() -> Result<::cpal::Stream, String> + Send + 'static,
    {
        let (command_tx, command_rx) = flume::unbounded();

        // Create a separate quit channel for immediate shutdown
        let (quit_tx, quit_rx) = flume::unbounded::<()>();

        let daemon = ResourceDaemon::new(move |quit_signal: QuitSignal<StreamDaemonError>| {
            log::debug!("CPAL stream daemon: starting daemon thread");

            // Create the stream in the daemon thread
            let stream = stream_factory().map_err(|e| {
                log::error!("CPAL stream daemon: stream creation failed: {e}");
                StreamDaemonError::StreamCreationFailed(e)
            })?;

            log::debug!("CPAL stream daemon: stream created successfully, starting playback");

            // Start the stream immediately
            if let Err(e) = stream.play() {
                log::error!("CPAL stream daemon: failed to start stream playback: {e:?}");
                return Err(StreamDaemonError::StreamCreationFailed(format!(
                    "Failed to start stream: {e:?}"
                )));
            }

            log::debug!("CPAL stream daemon: stream playback started");

            // Start the command processing loop
            Self::run_command_loop(&stream, &command_rx, &quit_rx, &volume_atomic, &quit_signal);

            Ok(())
        });

        let handle = StreamHandle {
            command_sender: command_tx,
        };

        let stream_daemon = Self {
            daemon,
            shutdown_sender: Some(quit_tx),
        };

        Ok((stream_daemon, handle))
    }

    /// Get the current state of the daemon
    #[must_use]
    pub fn state(&self) -> DaemonState<StreamDaemonError> {
        self.daemon.state()
    }

    /// Stop the daemon
    pub fn quit(&mut self, reason: StreamDaemonError) {
        // Send quit signal for immediate shutdown
        log::debug!("CpalStreamDaemon: quit called, sending quit signal");
        if let Some(quit_sender) = self.shutdown_sender.take()
            && let Err(e) = quit_sender.send(())
        {
            log::debug!("CpalStreamDaemon: failed to send quit signal: {e}");
        }
        self.daemon.quit(reason);
    }

    fn run_command_loop(
        stream: &::cpal::Stream,
        command_rx: &flume::Receiver<(StreamCommand, flume::Sender<StreamResponse>)>,
        quit_rx: &flume::Receiver<()>,
        volume_atomic: &Arc<std::sync::RwLock<Arc<atomic_float::AtomicF64>>>,
        quit_signal: &QuitSignal<StreamDaemonError>,
    ) {
        log::debug!("CPAL stream daemon: starting command loop");

        loop {
            // Use Selector to listen to both command and quit channels
            // Return true from callbacks to indicate we should exit
            let should_exit = flume::Selector::new()
                .recv(command_rx, |result| {
                    if let Ok((command, response_tx)) = result {
                        log::trace!("CPAL stream daemon: processing command: {command:?}");

                        let response = match command {
                            StreamCommand::Pause => match stream.pause() {
                                Ok(()) => {
                                    log::debug!("CPAL stream daemon: stream paused");
                                    StreamResponse::Success
                                }
                                Err(e) => {
                                    log::error!(
                                        "CPAL stream daemon: failed to pause stream: {e:?}"
                                    );
                                    StreamResponse::Error(format!("Failed to pause stream: {e:?}"))
                                }
                            },
                            StreamCommand::Resume => match stream.play() {
                                Ok(()) => {
                                    log::debug!("CPAL stream daemon: stream resumed");
                                    StreamResponse::Success
                                }
                                Err(e) => {
                                    log::error!(
                                        "CPAL stream daemon: failed to resume stream: {e:?}"
                                    );
                                    StreamResponse::Error(format!("Failed to resume stream: {e:?}"))
                                }
                            },
                            StreamCommand::Reset => match stream.pause() {
                                Ok(()) => {
                                    log::debug!("CPAL stream daemon: stream reset (paused)");
                                    StreamResponse::Success
                                }
                                Err(e) => {
                                    log::error!(
                                        "CPAL stream daemon: failed to reset stream: {e:?}"
                                    );
                                    StreamResponse::Error(format!("Failed to reset stream: {e:?}"))
                                }
                            },
                            StreamCommand::SetVolume(volume) => {
                                volume_atomic
                                    .read()
                                    .unwrap()
                                    .store(volume, std::sync::atomic::Ordering::SeqCst);
                                log::debug!("CPAL stream daemon: volume set to {volume}");
                                StreamResponse::Success
                            }
                        };

                        // Send response back
                        if let Err(e) = response_tx.send(response) {
                            log::warn!("CPAL stream daemon: failed to send response: {e}");
                            // If we can't send responses, the receiver is probably gone
                            quit_signal.dispatch(StreamDaemonError::DaemonStopped);
                            return true; // Exit
                        }
                        false // Continue
                    } else {
                        log::debug!(
                            "CPAL stream daemon: command channel closed, exiting command loop"
                        );
                        true // Exit
                    }
                })
                .recv(quit_rx, |_result| true)
                .wait();

            // Check if we should exit based on callback return values
            if should_exit {
                break;
            }
        }

        log::debug!("CPAL stream daemon: command loop ended - daemon thread shutting down");
    }
}

impl Drop for CpalStreamDaemon {
    fn drop(&mut self) {
        // Send quit signal for immediate shutdown
        log::debug!("CpalStreamDaemon: Drop called, sending quit signal for immediate shutdown");
        if let Some(quit_sender) = self.shutdown_sender.take() {
            if let Err(e) = quit_sender.send(()) {
                log::debug!(
                    "CpalStreamDaemon: failed to send quit signal (daemon may already be stopped): {e}"
                );
            } else {
                log::debug!("CpalStreamDaemon: quit signal sent successfully");
            }
        }
    }
}

// The daemon itself is Send+Sync because the !Send stream is owned by the daemon thread
unsafe impl Send for CpalStreamDaemon {}
unsafe impl Sync for CpalStreamDaemon {}
