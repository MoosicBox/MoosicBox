//! Embedded server infrastructure for `MoosicBox` native applications.
//!
//! This crate provides the bundled server component for Tauri-based `MoosicBox` applications,
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
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_async_service::{sync::RwLock, Arc, JoinHandle};
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
/// This module provides the service infrastructure for handling [`Command`]
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
                let result = ctx.read().await.handle_event(&event);
                if let Err(e) = result {
                    log::error!("process_command: Failed to handle event: {e:?}");
                }
            }
            Command::WaitForStartup { sender } => {
                let receiver = ctx.write().await.receiver.take();
                if let Some(receiver) = receiver {
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
                let handle = ctx.write().await.server_handle.take();
                if let Some(handle) = handle {
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
    pub fn handle_event(&self, event: &RunEvent) -> Result<(), std::io::Error> {
        if let tauri::RunEvent::ExitRequested { .. } = *event {
            self.shutdown()?;
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
