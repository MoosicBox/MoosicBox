//! Tauri bundled application service for `MoosicBox`.
//!
//! This crate provides the bundled Tauri application that runs the `MoosicBox`
//! server embedded within a desktop application. It manages the lifecycle of
//! the embedded server, including startup, shutdown, and event handling.
//!
//! # Example
//!
//! ```rust,no_run
//! use moosicbox_app_tauri_bundled::{Context, service};
//! use moosicbox_async_service::{Arc, sync::RwLock};
//!
//! # fn main() {
//! let runtime_handle = moosicbox_async_service::runtime::Handle::current();
//! let ctx = Context::new(&runtime_handle);
//! let service = service::Service::new(ctx);
//! let _handle = service.start();
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_async_service::{Arc, JoinHandle, sync::RwLock};
use moosicbox_config::AppType;
use strum_macros::AsRefStr;
use switchy_async::sync::oneshot;
use tauri::RunEvent;

/// Commands for the Tauri bundled app service.
#[derive(Debug, AsRefStr)]
pub enum Command {
    /// Process a Tauri run event.
    RunEvent { event: Arc<RunEvent> },
    /// Wait for the server to complete startup.
    WaitForStartup { sender: oneshot::Sender<()> },
    /// Wait for the server to complete shutdown.
    WaitForShutdown { sender: oneshot::Sender<()> },
}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Async service for managing the Tauri bundled app lifecycle.
pub mod service {
    moosicbox_async_service::async_service!(super::Command, super::Context);
}

#[moosicbox_async_service::async_trait]
impl service::Processor for service::Service {
    type Error = service::Error;

    async fn on_start(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn on_shutdown(_ctx: Arc<RwLock<Context>>) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn process_command(
        ctx: Arc<RwLock<Context>>,
        command: Command,
    ) -> Result<(), Self::Error> {
        log::debug!("process_command: command={command}");
        match command {
            Command::RunEvent { event } => {
                log::debug!("process_command: Received RunEvent command");
                let response = ctx.read().await.handle_event(&event);
                if let Err(e) = response {
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

/// Context for the Tauri bundled app service.
pub struct Context {
    server_handle: Option<JoinHandle<std::io::Result<()>>>,
    receiver: Option<switchy_async::sync::oneshot::Receiver<()>>,
}

impl Context {
    /// Creates a new `Context` and starts the embedded server.
    ///
    /// # Panics
    ///
    /// * If the default download path cannot be determined
    /// * If the download directory cannot be created
    /// * If the download path contains invalid UTF-8
    /// * If database profiles cannot be accessed
    /// * If scan paths cannot be added to the database
    #[must_use]
    pub fn new(handle: &moosicbox_async_service::runtime::Handle) -> Self {
        let downloads_path = moosicbox_downloader::get_default_download_path().unwrap();
        std::fs::create_dir_all(&downloads_path).unwrap();

        let (sender, receiver) = switchy_async::sync::oneshot::channel();

        let addr = "0.0.0.0";
        let port = 8016;

        let server_handle = handle.spawn_with_name(
            "moosicbox_app_tauri_bundled server",
            moosicbox_server::run_basic(AppType::App, addr, port, None, move |_| {
                switchy_async::runtime::Handle::current().spawn_with_name(
                    "moosicbox_app_tauri_bundled: create_download_location",
                    async move {
                        let downloads_path_str = downloads_path.to_str().unwrap();

                        for profile in switchy_database::profiles::PROFILES.names() {
                            let db = switchy_database::profiles::PROFILES.get(&profile).unwrap();
                            moosicbox_scan::db::add_scan_path(&db, downloads_path_str)
                                .await
                                .unwrap();
                        }

                        moosicbox_profiles::events::on_profiles_updated_event(
                            move |added, _removed| {
                                let added = added.to_vec();
                                let downloads_path = downloads_path.clone();

                                Box::pin(async move {
                                    let downloads_path_str = downloads_path.to_str().unwrap();

                                    for profile in &added {
                                        let db = switchy_database::profiles::PROFILES
                                            .get(profile)
                                            .unwrap();
                                        moosicbox_scan::db::add_scan_path(&db, downloads_path_str)
                                            .await
                                            .unwrap();
                                    }

                                    Ok(())
                                })
                            },
                        )
                        .await;
                    },
                );

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

    /// Handles a Tauri run event, initiating shutdown on exit requests.
    ///
    /// # Errors
    ///
    /// * If an IO error occurs
    pub fn handle_event(&self, event: &Arc<RunEvent>) -> Result<(), std::io::Error> {
        if let tauri::RunEvent::ExitRequested { .. } = **event {
            self.shutdown()?;
        }
        Ok(())
    }

    /// Shuts down the embedded server.
    ///
    /// # Errors
    ///
    /// * This function currently never returns an error
    pub fn shutdown(&self) -> Result<(), std::io::Error> {
        if let Some(handle) = &self.server_handle {
            handle.abort();
        }
        Ok(())
    }
}
