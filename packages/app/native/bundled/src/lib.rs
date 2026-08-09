//! Embedded server infrastructure for `MoosicBox` native applications.
//!
//! This crate provides the bundled server component for Tauri-based `MoosicBox` applications,
//! managing an embedded HTTP server that handles music streaming and API requests. The server
//! binds an OS-assigned loopback port and integrates with the Tauri application lifecycle.
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
//! // Server reserves an OS-assigned loopback port and reports it through readiness.
//! // The application activates that runtime endpoint after startup completes.
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_async_service::{Arc, sync::RwLock};
use moosicbox_config::AppType;
pub use moosicbox_server::{
    BundledReadyServer as ReadyServer, BundledStartupError as StartupError,
};
use strum_macros::AsRefStr;
use switchy_async::sync::oneshot;
use tauri::RunEvent;

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
    async fn on_start(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Cancels and settles the bundled server if the command service stops first.
    async fn on_shutdown(ctx: Arc<RwLock<Context>>) -> Result<(), Self::Error> {
        let handle = ctx.read().await.server_handle.clone();
        handle.abort();
        if let Err(error) = handle.wait().await
            && error.kind() != std::io::ErrorKind::Interrupted
        {
            return Err(error.into());
        }
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
                let result = ctx.write().await.startup.wait().await;
                if let Err(e) = sender.send(result) {
                    log::error!("process_command: Failed to send WaitForStartup response: {e:?}");
                }
            }
            Command::WaitForShutdown { sender } => {
                let handle = ctx.read().await.server_handle.clone();
                handle.wait().await?;
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
    server_handle: moosicbox_server::BundledServerTask,
    /// Authoritative startup coordinator.
    startup: moosicbox_server::BundledStartup,
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
    /// # Errors
    ///
    /// * If a loopback listener cannot be reserved
    /// * If the selected listener address cannot be read
    pub fn new(handle: &moosicbox_async_service::runtime::Handle) -> Result<Self, StartupError> {
        let (startup, startup_sender) = moosicbox_server::BundledStartup::pending();
        let (listener, ready) = moosicbox_server::bind_bundled_listener()?;
        let port = listener
            .local_addr()
            .map_err(|error| StartupError::new(error.to_string()))?
            .port();
        let startup_ready = ready;
        let failure_sender = startup_sender.clone();

        let server_handle =
            handle.spawn_with_name("moosicbox_app_native_bundled server", async move {
                let result = moosicbox_server::run_basic_with_listener(
                    AppType::App,
                    "127.0.0.1",
                    port,
                    None,
                    Some(listener),
                    move |_| {
                        log::info!("App server listening at {}", startup_ready.endpoint);
                        startup_sender.ready(startup_ready);
                    },
                )
                .await;
                if let Err(error) = &result {
                    failure_sender.failed(error.to_string());
                }
                result
            });

        Ok(Self {
            server_handle: moosicbox_server::BundledServerTask::new(server_handle),
            startup,
        })
    }

    /// Returns shared ownership of the bundled server task.
    #[must_use]
    pub fn server_task(&self) -> moosicbox_server::BundledServerTask {
        self.server_handle.clone()
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
        self.server_handle.abort();
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    #[switchy_async::test]
    async fn startup_channel_closure_is_an_error() {
        let (mut startup, sender) = moosicbox_server::BundledStartup::pending();
        drop(sender);

        let error = startup.wait().await.unwrap_err();

        assert!(error.to_string().contains("startup channel closed"));
    }

    #[switchy_async::test]
    async fn startup_channel_preserves_server_failure() {
        let (mut startup, sender) = moosicbox_server::BundledStartup::pending();
        sender.failed("migration failed");

        let error = startup.wait().await.unwrap_err();

        assert_eq!(
            error.to_string(),
            "Bundled server failed to start: migration failed"
        );
    }
}
