//! Embedded server infrastructure for MoosicBox native applications.
//!
//! This crate provides the bundled server component for Tauri-based MoosicBox applications,
//! managing an embedded HTTP server that handles music streaming and API requests. The server
//! runs on `0.0.0.0:8016` and integrates with the Tauri application lifecycle.
//!
//! # Main Components
//!
//! * [`Command`] - Service commands for controlling server lifecycle and event processing
//! * [`Context`] - Application context managing the embedded server and startup synchronization
//! * [`service`] - Async service implementation for command processing
//!
//! # Example
//!
//! ```rust,no_run
//! # use moosicbox_app_native_bundled::{Context, service};
//! # use moosicbox_async_service::runtime::Handle;
//! # async fn example(runtime_handle: &Handle) {
//! // Create context and start embedded server
//! let ctx = Context::new(runtime_handle);
//!
//! // Server starts listening on 0.0.0.0:8016
//! // and processes music streaming requests
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_async_service::{Arc, JoinHandle, sync::RwLock};
use moosicbox_config::AppType;
use strum_macros::AsRefStr;
use switchy_async::sync::oneshot;
use tauri::RunEvent;

/// Commands for controlling the bundled native application service.
#[derive(Debug, AsRefStr)]
pub enum Command {
    /// Process a Tauri run event.
    RunEvent { event: Arc<RunEvent> },
    /// Wait for the application server to start up.
    WaitForStartup { sender: oneshot::Sender<()> },
    /// Wait for the application server to shut down.
    WaitForShutdown { sender: oneshot::Sender<()> },
}

impl std::fmt::Display for Command {
    /// Formats the command using its string representation.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Async service implementation for processing application commands.
///
/// This module provides the service infrastructure for handling [`Command`](super::Command)
/// instances asynchronously, managing server lifecycle and event processing.
pub mod service {
    moosicbox_async_service::async_service!(super::Command, super::Context);
}

/// Service processor implementation for the bundled native application.
///
/// Handles command processing, startup, and shutdown lifecycle events.
#[moosicbox_async_service::async_trait]
impl service::Processor for service::Service {
    type Error = service::Error;

    /// Initializes the service on startup.
    ///
    /// Currently performs no initialization and always succeeds.
    async fn on_start(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Cleans up resources on service shutdown.
    ///
    /// Currently performs no cleanup and always succeeds.
    async fn on_shutdown(_ctx: Arc<RwLock<Context>>) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Processes commands for the bundled native application service.
    ///
    /// # Errors
    ///
    /// * Returns an error if the server task panicked when waiting for shutdown (during `WaitForShutdown`)
    /// * Returns an error if the server returned an I/O error during shutdown (during `WaitForShutdown`)
    async fn process_command(
        ctx: Arc<RwLock<Context>>,
        command: Command,
    ) -> Result<(), Self::Error> {
        log::debug!("process_command: command={command}");
        match command {
            Command::RunEvent { event } => {
                log::debug!("process_command: Received RunEvent command");
                if let Err(e) = ctx.read().await.handle_event(event) {
                    log::error!("process_command: Failed to handle event: {e:?}");
                }
            }
            Command::WaitForStartup { sender } => {
                if let Some(receiver) = ctx.write().await.receiver.take() {
                    log::debug!("process_command: Waiting for startup...");
                    if let Err(e) = receiver.await {
                        log::error!(
                            "process_command: Failed to wait for on_startup response: {e:?}"
                        );
                    }
                    log::debug!("process_command: Finished waiting for startup");
                } else {
                    log::debug!("process_command: Already started up");
                }
                if let Err(e) = sender.send(()) {
                    log::error!("process_command: Failed to send WaitForStartup response: {e:?}");
                }
            }
            Command::WaitForShutdown { sender } => {
                if let Some(handle) = ctx.write().await.server_handle.take() {
                    handle.await??;
                }
                if let Err(e) = sender.send(()) {
                    log::error!("process_command: Failed to send WaitForShutdown response: {e:?}");
                }
            }
        }
        Ok(())
    }
}

/// Application context managing the embedded server and startup lifecycle.
pub struct Context {
    /// Handle to the server task, used to wait for completion or abort the server.
    server_handle: Option<JoinHandle<std::io::Result<()>>>,
    /// Oneshot receiver for server startup notification.
    receiver: Option<switchy_async::sync::oneshot::Receiver<()>>,
}

impl Context {
    /// Creates a new application context and starts the embedded server.
    ///
    /// The server listens on `0.0.0.0:8016` and signals startup completion
    /// through an internal channel.
    #[must_use]
    pub fn new(handle: &moosicbox_async_service::runtime::Handle) -> Self {
        let (sender, receiver) = switchy_async::sync::oneshot::channel();

        let addr = "0.0.0.0";
        let port = 8016;

        let server_handle = handle.spawn_with_name(
            "moosicbox_app_tauri_bundled server",
            moosicbox_server::run_basic(AppType::App, addr, port, None, move |_| {
                log::info!("App server listening on {addr}:{port}");
                if let Err(e) = sender.send(()) {
                    log::error!("Failed to send on_startup response: {e:?}");
                }
            }),
        );

        Self {
            server_handle: Some(server_handle),
            receiver: Some(receiver),
        }
    }

    /// Handles Tauri run events, triggering appropriate lifecycle actions.
    ///
    /// # Errors
    ///
    /// * Returns an error if shutting down the server fails during `ExitRequested` handling
    pub fn handle_event(&self, event: Arc<RunEvent>) -> Result<(), std::io::Error> {
        match *event {
            tauri::RunEvent::Exit => {}
            tauri::RunEvent::ExitRequested { .. } => {
                self.shutdown()?;
            }
            tauri::RunEvent::WindowEvent { .. } => {}
            tauri::RunEvent::Ready => {}
            tauri::RunEvent::Resumed => {}
            tauri::RunEvent::MainEventsCleared => {}
            _ => {}
        }
        Ok(())
    }

    /// Shuts down the embedded server by aborting its task handle.
    ///
    /// # Errors
    ///
    /// * Currently always returns `Ok(())`
    pub fn shutdown(&self) -> Result<(), std::io::Error> {
        if let Some(handle) = &self.server_handle {
            handle.abort();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchy_async::sync::oneshot;

    #[test]
    fn test_command_display_run_event() {
        let event = Arc::new(RunEvent::Ready);
        let command = Command::RunEvent { event };
        assert_eq!(command.to_string(), "RunEvent");
    }

    #[test]
    fn test_command_display_wait_for_startup() {
        let (sender, _receiver) = oneshot::channel();
        let command = Command::WaitForStartup { sender };
        assert_eq!(command.to_string(), "WaitForStartup");
    }

    #[test]
    fn test_command_display_wait_for_shutdown() {
        let (sender, _receiver) = oneshot::channel();
        let command = Command::WaitForShutdown { sender };
        assert_eq!(command.to_string(), "WaitForShutdown");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_handle_event_exit() {
        let context = create_test_context();
        let event = Arc::new(RunEvent::Exit);
        let result = context.handle_event(event);
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_handle_event_ready() {
        let context = create_test_context();
        let event = Arc::new(RunEvent::Ready);
        let result = context.handle_event(event);
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_handle_event_resumed() {
        let context = create_test_context();
        let event = Arc::new(RunEvent::Resumed);
        let result = context.handle_event(event);
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_handle_event_main_events_cleared() {
        let context = create_test_context();
        let event = Arc::new(RunEvent::MainEventsCleared);
        let result = context.handle_event(event);
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_shutdown_with_handle() {
        let context = create_test_context();
        let result = context.shutdown();
        assert!(result.is_ok());
    }

    #[test]
    fn test_shutdown_without_handle() {
        let context = Context {
            server_handle: None,
            receiver: None,
        };
        let result = context.shutdown();
        assert!(result.is_ok());
    }

    /// Helper function to create a test context with a mock server handle
    fn create_test_context() -> Context {
        let (_sender, receiver) = switchy_async::sync::oneshot::channel();
        let handle = moosicbox_async_service::runtime::Handle::current().spawn_with_name(
            "test_server",
            async move {
                switchy_async::time::sleep(std::time::Duration::from_secs(1000)).await;
                Ok(())
            },
        );
        Context {
            server_handle: Some(handle),
            receiver: Some(receiver),
        }
    }
}
