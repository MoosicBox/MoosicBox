#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_async_service::{Arc, JoinHandle, sync::RwLock};
use moosicbox_config::AppType;
use strum_macros::AsRefStr;
use switchy_async::sync::oneshot;
use tauri::RunEvent;

#[derive(Debug, AsRefStr)]
pub enum Command {
    RunEvent { event: Arc<RunEvent> },
    WaitForStartup { sender: oneshot::Sender<()> },
    WaitForShutdown { sender: oneshot::Sender<()> },
}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

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

pub struct Context {
    server_handle: Option<JoinHandle<std::io::Result<()>>>,
    receiver: Option<switchy_async::sync::oneshot::Receiver<()>>,
}

impl Context {
    /// # Panics
    ///
    /// * If fails to get the `LibraryDatabase`
    #[must_use]
    pub fn new(handle: &moosicbox_async_service::runtime::Handle) -> Self {
        let downloads_path = moosicbox_downloader::get_default_download_path().unwrap();
        std::fs::create_dir_all(&downloads_path).unwrap();

        let (sender, receiver) = switchy_async::sync::oneshot::channel();

        let addr = "0.0.0.0";
        let port = 8016;

        let server_handle = moosicbox_task::spawn_on(
            "moosicbox_app_tauri_bundled server",
            handle,
            moosicbox_server::run_basic(AppType::App, addr, port, None, move |_| {
                moosicbox_task::spawn(
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

    /// # Errors
    ///
    /// * If an IO error occurs
    pub fn handle_event(&self, event: &Arc<RunEvent>) -> Result<(), std::io::Error> {
        if let tauri::RunEvent::ExitRequested { .. } = **event {
            self.shutdown()?;
        }
        Ok(())
    }

    /// # Errors
    ///
    /// * None
    pub fn shutdown(&self) -> Result<(), std::io::Error> {
        if let Some(handle) = &self.server_handle {
            handle.abort();
        }
        Ok(())
    }
}
