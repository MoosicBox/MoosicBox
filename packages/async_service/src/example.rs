use moosicbox_async_service::{tokio::sync::RwLock, Arc};
use strum_macros::AsRefStr;

#[derive(Debug, AsRefStr)]
pub enum Command {
    Connect {},
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
        _ctx: Arc<RwLock<Context>>,
        command: Command,
    ) -> Result<(), Self::Error> {
        log::debug!("process_command command={command}");
        match command {
            Command::Connect {} => {
                log::debug!("Received Connect command");
            }
        }
        Ok(())
    }
}

pub struct Context {}
