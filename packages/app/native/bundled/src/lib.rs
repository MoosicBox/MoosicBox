#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_async_service::{Arc, JoinHandle, tokio::sync::RwLock};
use moosicbox_config::AppType;
use strum_macros::AsRefStr;
use tauri::RunEvent;

#[derive(Debug, AsRefStr)]
pub enum Command {
    RunEvent {
        event: Arc<RunEvent>,
    },
    WaitForStartup {
        sender: tokio::sync::oneshot::Sender<()>,
    },
    WaitForShutdown {
        sender: tokio::sync::oneshot::Sender<()>,
    },
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

pub struct Context {
    server_handle: Option<JoinHandle<std::io::Result<()>>>,
    receiver: Option<tokio::sync::oneshot::Receiver<()>>,
}

impl Context {
    pub fn new(handle: &tokio::runtime::Handle) -> Self {
        let (sender, receiver) = tokio::sync::oneshot::channel();

        let addr = "0.0.0.0";
        let port = 8016;

        let server_handle = moosicbox_task::spawn_on(
            "moosicbox_app_tauri_bundled server",
            handle,
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

    pub fn shutdown(&self) -> Result<(), std::io::Error> {
        if let Some(handle) = &self.server_handle {
            handle.abort();
        }
        Ok(())
    }
}
