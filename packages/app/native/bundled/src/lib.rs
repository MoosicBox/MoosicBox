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
//! let ctx = Context::new(runtime_handle).expect("Failed to initialize bundled server");
//!
//! // Server starts listening on 0.0.0.0:8016
//! // and processes music streaming requests
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::{Arc as StdArc, Mutex};

use moosicbox_async_service::{Arc, JoinHandle, sync::RwLock};
use moosicbox_config::AppType;
use strum_macros::AsRefStr;
use switchy_async::sync::oneshot;
use tauri::RunEvent;

/// Error returned when the bundled server cannot reach readiness.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
#[error("Bundled server failed to start: {message}")]
pub struct StartupError {
    message: String,
}

impl StartupError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

async fn receive_startup(
    receiver: switchy_async::sync::oneshot::Receiver<Result<ReadyServer, StartupError>>,
) -> Result<ReadyServer, StartupError> {
    receiver.await.unwrap_or_else(|error| {
        Err(StartupError::new(format!(
            "startup channel closed before readiness: {error}"
        )))
    })
}

/// Authoritative result of successfully starting the bundled server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyServer {
    /// Loopback HTTP endpoint selected for this server instance.
    pub endpoint: String,
}

/// Commands for controlling the bundled native application service.
#[derive(Debug, AsRefStr)]
pub enum Command {
    /// Process a Tauri run event.
    RunEvent { event: Arc<RunEvent> },
    /// Wait for the application server to start up.
    WaitForStartup {
        sender: oneshot::Sender<Result<ReadyServer, StartupError>>,
    },
    /// Wait for the application server to shut down.
    WaitForShutdown { sender: oneshot::Sender<()> },
}

impl std::fmt::Display for Command {
    /// Formats the command using its string representation.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

pub mod service {
    //! Async service implementation for processing application commands.
    //!
    //! This module provides the service infrastructure for handling [`Command`]
    //! instances asynchronously, managing server lifecycle and event processing.

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
                    let result = receive_startup(receiver).await;
                    *ctx.read().await.ready.lock().unwrap() = Some(result);
                    log::debug!("process_command: Finished waiting for startup");
                } else {
                    log::debug!("process_command: Already finished startup");
                }
                let ready = ctx
                    .read()
                    .await
                    .ready
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| Err(StartupError::new("startup has not completed")));
                if let Err(e) = sender.send(ready) {
                    log::error!("process_command: Failed to send WaitForStartup response: {e:?}");
                }
            }
            Command::WaitForShutdown { sender } => {
                let handle = ctx.read().await.server_handle.lock().unwrap().take();
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
    server_handle: StdArc<Mutex<Option<JoinHandle<std::io::Result<()>>>>>,
    /// Oneshot receiver for server startup notification.
    receiver: Option<switchy_async::sync::oneshot::Receiver<Result<ReadyServer, StartupError>>>,
    /// Last authoritative startup result.
    ready: StdArc<Mutex<Option<Result<ReadyServer, StartupError>>>>,
}

impl Context {
    /// Creates a new application context and starts the embedded server.
    ///
    /// The server reserves an OS-assigned loopback port and reports the
    /// authoritative endpoint through its startup channel.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use moosicbox_app_native_bundled::Context;
    ///
    /// # fn example(handle: &moosicbox_async_service::runtime::Handle) {
    /// let _ctx = Context::new(handle).expect("Failed to initialize bundled server");
    /// # }
    /// ```
    #[must_use]
    pub fn new(handle: &moosicbox_async_service::runtime::Handle) -> Result<Self, StartupError> {
        let (sender, receiver) = switchy_async::sync::oneshot::channel();

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .map_err(|error| StartupError::new(error.to_string()))?;
        let port = listener
            .local_addr()
            .map_err(|error| StartupError::new(error.to_string()))?
            .port();
        let ready = ReadyServer {
            endpoint: format!("http://127.0.0.1:{port}"),
        };
        let startup_ready = ready.clone();

        let server_handle = handle.spawn_with_name(
            "moosicbox_app_native_bundled server",
            moosicbox_server::run_basic_with_listener(
                AppType::App,
                "127.0.0.1",
                port,
                None,
                Some(listener),
                move |_| {
                    log::info!("App server listening at {}", startup_ready.endpoint);
                    if let Err(e) = sender.send(Ok(startup_ready)) {
                        log::error!("Failed to send on_startup response: {e:?}");
                    }
                },
            ),
        );

        Ok(Self {
            server_handle: StdArc::new(Mutex::new(Some(server_handle))),
            receiver: Some(receiver),
            ready: StdArc::new(Mutex::new(None)),
        })
    }

    /// Handles Tauri run events, triggering appropriate lifecycle actions.
    ///
    /// # Errors
    ///
    /// * Returns an error if shutting down the server fails during `ExitRequested` handling
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use moosicbox_app_native_bundled::Context;
    /// use tauri::RunEvent;
    ///
    /// # fn example(ctx: &Context, event: &RunEvent) -> Result<(), std::io::Error> {
    /// ctx.handle_event(event)?;
    /// # Ok(())
    /// # }
    /// ```
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
        if let Some(handle) = self.server_handle.lock().unwrap().as_ref() {
            handle.abort();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[switchy_async::test]
    async fn startup_channel_closure_is_an_error() {
        let (sender, receiver) = switchy_async::sync::oneshot::channel();
        drop(sender);

        let error = receive_startup(receiver).await.unwrap_err();

        assert!(error.to_string().contains("startup channel closed"));
    }

    #[switchy_async::test]
    async fn startup_channel_preserves_server_failure() {
        let (sender, receiver) = switchy_async::sync::oneshot::channel();
        sender
            .send(Err(StartupError::new("migration failed")))
            .unwrap();

        let error = receive_startup(receiver).await.unwrap_err();

        assert_eq!(
            error.to_string(),
            "Bundled server failed to start: migration failed"
        );
    }
}
