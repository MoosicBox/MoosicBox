use std::net::SocketAddr;

use mdns_sd::{ServiceDaemon, ServiceEvent};
use moosicbox_async_service::{Arc, CancellationToken, JoinError, JoinHandle, sync::RwLock};
use strum_macros::AsRefStr;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct MoosicBox {
    pub id: String,
    pub name: String,
    pub host: SocketAddr,
    pub dns: String,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    MdnsSd(#[from] mdns_sd::Error),
    #[error(transparent)]
    Send(#[from] kanal::SendError),
    #[error(transparent)]
    Join(#[from] JoinError),
}

#[derive(Debug, AsRefStr)]
pub enum Command {}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

pub struct Context {
    token: CancellationToken,
    handle: Option<JoinHandle<Result<(), Error>>>,
    sender: kanal::AsyncSender<MoosicBox>,
}

impl Context {
    #[must_use]
    pub fn new(sender: kanal::AsyncSender<MoosicBox>) -> Self {
        Self {
            token: CancellationToken::new(),
            handle: None,
            sender,
        }
    }
}

pub mod service {
    moosicbox_async_service::async_service!(super::Command, super::Context, super::Error);
}

#[moosicbox_async_service::async_trait]
impl service::Processor for service::Service {
    type Error = service::Error;

    async fn on_start(&mut self) -> Result<(), Self::Error> {
        let mut ctx = self.ctx.write().await;

        let tx = ctx.sender.clone();
        let token = ctx.token.clone();

        ctx.handle
            .replace(switchy_async::runtime::Handle::current().spawn_with_name(
                "switchy_mdns scanner",
                async move {
                    let mdns = ServiceDaemon::new()?;
                    let service_type = "_moosicboxserver._tcp.local.";
                    let receiver = mdns.browse(service_type)?;

                    log::debug!("mdns scanner: Browsing for {service_type} services...");

                    while let Ok(Some(event)) = {
                        switchy_async::select! {
                            event = receiver.recv_async() => event.map(Some),
                            () = token.cancelled() => Ok(None)
                        }
                    } {
                        if let ServiceEvent::ServiceResolved(info) = event {
                            log::debug!(
                                "mdns scanner: Found server instance: {}",
                                info.get_fullname()
                            );

                            for addr in info.get_addresses().iter().filter(|x| x.is_ipv4()).copied()
                            {
                                let socket_addr = SocketAddr::new(addr, info.get_port());
                                log::debug!("mdns scanner: Server address: {addr}");
                                let dns = info.get_fullname().to_string();

                                let server = MoosicBox {
                                    id: dns.split_once('.').expect("Invalid dns").0.to_string(),
                                    name: info.get_hostname().to_string(),
                                    host: socket_addr,
                                    dns,
                                };

                                moosicbox_assert::die_or_propagate!(tx.send(server).await);
                            }
                        }
                    }

                    Ok::<_, Error>(())
                },
            ));

        drop(ctx);

        Ok(())
    }

    async fn on_shutdown(ctx: Arc<RwLock<Context>>) -> Result<(), Self::Error> {
        let handle = &mut ctx.write().await.handle;

        if let Some(handle) = handle {
            ctx.read().await.token.cancel();
            handle.await??;
        }

        Ok(())
    }

    async fn process_command(
        _ctx: Arc<RwLock<Context>>,
        command: Command,
    ) -> Result<(), Self::Error> {
        log::debug!("mdns scanner: process_command command={command}");
        Ok(())
    }
}
