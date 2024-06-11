use std::{collections::HashMap, fmt::Display, pin::Pin, time::Duration};

use flume::{unbounded, Receiver, SendError, Sender};
use futures::Future;
use thiserror::Error;
use tokio::task::{JoinError, JoinHandle};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Error)]
pub enum ListenerError {
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error(transparent)]
    Rupnp(#[from] rupnp::Error),
}

pub enum UpnpCommand {
    Subscribe {
        instance_id: u32,
        action: SubscriptionAction,
    },
    Unsubscribe {
        instance_id: u32,
    },
}

impl Display for UpnpCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpnpCommand::Subscribe { .. } => f.write_str("Subscribe"),
            UpnpCommand::Unsubscribe { .. } => f.write_str("Unsubscribe"),
        }
    }
}

pub struct UpnpListener {
    sender: Sender<UpnpCommand>,
    receiver: Receiver<UpnpCommand>,
    #[allow(clippy::type_complexity)]
    status_join_handles: HashMap<u32, JoinHandle<Result<(), ListenerError>>>,
    token: CancellationToken,
    status_tokens: HashMap<u32, CancellationToken>,
}

impl UpnpListener {
    pub fn new() -> Self {
        let (tx, rx) = unbounded();
        Self {
            sender: tx,
            receiver: rx,
            status_join_handles: HashMap::new(),
            token: CancellationToken::new(),
            status_tokens: HashMap::new(),
        }
    }

    pub fn start(mut self) -> JoinHandle<Result<(), ListenerError>> {
        tokio::spawn(async move {
            while let Ok(Ok(cmd)) = tokio::select!(
                () = self.token.cancelled() => {
                    log::debug!("UpnpListener was cancelled");
                    Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Cancelled"))
                }
                cmd = self.receiver.recv_async() => { Ok(cmd) }
            ) {
                log::trace!("Received UpnpListener command");
                self.process_command(cmd).await?;
            }

            for (_, handle) in self.status_join_handles.drain() {
                handle.await??;
            }

            log::debug!("Stopped UpnpListener");

            Ok(())
        })
    }

    pub fn handle(&self) -> UpnpListenerHandle {
        UpnpListenerHandle {
            sender: self.sender.clone(),
            token: self.token.clone(),
        }
    }

    async fn subscribe(
        &mut self,
        instance_id: u32,
        action: SubscriptionAction,
    ) -> Result<(), ListenerError> {
        let token = self.token.clone();
        let status_token = CancellationToken::new();
        self.status_tokens.insert(instance_id, status_token.clone());
        self.status_join_handles.insert(
            instance_id,
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_millis(200));

                while tokio::select!(
                    () = token.cancelled() => {
                        log::debug!("UpnpListener was cancelled");
                        Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Cancelled"))
                    }
                    () = status_token.cancelled() => {
                        log::debug!("Subscription was cancelled for instance_id={instance_id}");
                        Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Cancelled"))
                    }
                    _ = interval.tick() => { Ok(()) }
                )
                .is_ok()
                {
                    log::trace!("Tick");
                    action().await;
                }

                Ok(())
            }),
        );

        Ok(())
    }

    async fn unsubscribe(&mut self, instance_id: u32) -> Result<(), ListenerError> {
        if let Some(token) = self.status_tokens.remove(&instance_id) {
            token.cancel();
            if let Some(handle) = self.status_join_handles.remove(&instance_id) {
                handle.await??;
            }
        }

        Ok(())
    }

    async fn process_command(&mut self, command: UpnpCommand) -> Result<(), ListenerError> {
        log::debug!("process_command command={command}");
        match command {
            UpnpCommand::Subscribe {
                instance_id,
                action,
            } => {
                self.subscribe(instance_id, action).await?;
            }
            UpnpCommand::Unsubscribe { instance_id } => {
                self.unsubscribe(instance_id).await?;
            }
        }

        Ok(())
    }
}

impl Default for UpnpListener {
    fn default() -> Self {
        Self::new()
    }
}

pub type SubscriptionAction = Box<dyn (Fn() -> Pin<Box<dyn Future<Output = ()> + Send>>) + Send>;

pub trait UpnpCommander {
    type Error;

    fn subscribe(&self, instance_id: u32, func: SubscriptionAction) -> Result<(), Self::Error>;
    fn unsubscribe(&self, instance_id: u32) -> Result<(), Self::Error>;
    fn shutdown(&self) -> Result<(), Self::Error>;
}

pub struct UpnpListenerHandle {
    sender: Sender<UpnpCommand>,
    token: CancellationToken,
}

impl UpnpCommander for UpnpListener {
    type Error = SendError<UpnpCommand>;

    fn subscribe(&self, instance_id: u32, action: SubscriptionAction) -> Result<(), Self::Error> {
        self.sender.send(UpnpCommand::Subscribe {
            instance_id,
            action,
        })
    }

    fn unsubscribe(&self, instance_id: u32) -> Result<(), Self::Error> {
        self.sender.send(UpnpCommand::Unsubscribe { instance_id })
    }

    fn shutdown(&self) -> Result<(), Self::Error> {
        log::debug!("Shutting down UpnpListener");
        self.token.cancel();
        Ok(())
    }
}

impl UpnpCommander for UpnpListenerHandle {
    type Error = SendError<UpnpCommand>;

    fn subscribe(&self, instance_id: u32, action: SubscriptionAction) -> Result<(), Self::Error> {
        self.sender.send(UpnpCommand::Subscribe {
            instance_id,
            action,
        })
    }

    fn unsubscribe(&self, instance_id: u32) -> Result<(), Self::Error> {
        self.sender.send(UpnpCommand::Unsubscribe { instance_id })
    }

    fn shutdown(&self) -> Result<(), Self::Error> {
        log::debug!("Shutting down UpnpListener");
        self.token.cancel();
        Ok(())
    }
}
