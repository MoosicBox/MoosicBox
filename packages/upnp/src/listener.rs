use std::{collections::HashMap, fmt::Display, pin::Pin, sync::Arc, time::Duration};

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
    SubscribeMediaInfo {
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: MediaInfoSubscriptionAction,
    },
    UnsubscribeMediaInfo {
        instance_id: u32,
        udn: String,
        service_id: String,
    },
    SubscribePositionInfo {
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: PositionInfoSubscriptionAction,
    },
    UnsubscribePositionInfo {
        instance_id: u32,
        udn: String,
        service_id: String,
    },
}

impl Display for UpnpCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpnpCommand::SubscribeMediaInfo { .. } => f.write_str("SubscribeMediaInfo"),
            UpnpCommand::UnsubscribeMediaInfo { .. } => f.write_str("UnsubscribeMediaInfo"),
            UpnpCommand::SubscribePositionInfo { .. } => f.write_str("SubscribePositionInfo"),
            UpnpCommand::UnsubscribePositionInfo { .. } => f.write_str("UnsubscribePositionInfo"),
        }
    }
}

pub struct UpnpListener {
    sender: Sender<UpnpCommand>,
    receiver: Receiver<UpnpCommand>,
    #[allow(clippy::type_complexity)]
    status_join_handles: HashMap<String, JoinHandle<Result<(), ListenerError>>>,
    token: CancellationToken,
    status_tokens: HashMap<String, CancellationToken>,
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
        interval: Duration,
        key: String,
        action: SubscriptionAction,
    ) -> Result<(), ListenerError> {
        let token = self.token.clone();
        let status_token = CancellationToken::new();
        self.status_tokens.insert(key.clone(), status_token.clone());
        self.status_join_handles.insert(
            key.clone(),
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(interval);

                while tokio::select!(
                    () = token.cancelled() => {
                        log::debug!("UpnpListener was cancelled");
                        Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Cancelled"))
                    }
                    () = status_token.cancelled() => {
                        log::debug!("Subscription was cancelled for key={key}");
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

    async fn unsubscribe(&mut self, key: String) -> Result<(), ListenerError> {
        log::debug!("Unsubscribing key={key}");
        if let Some(token) = self.status_tokens.remove(&key) {
            token.cancel();
            if let Some(handle) = self.status_join_handles.remove(&key) {
                handle.await??;
            } else {
                log::debug!("No status_join_handle with key={key}");
            }
        } else {
            log::debug!("No token with key={key}");
        }

        Ok(())
    }

    async fn process_command(&mut self, command: UpnpCommand) -> Result<(), ListenerError> {
        log::debug!("process_command command={command}");
        match command {
            UpnpCommand::SubscribeMediaInfo {
                interval,
                instance_id,
                udn,
                service_id,
                action,
            } => {
                let action = Arc::new(action);
                let key = format!("MediaInfo:{instance_id}:{udn}:{service_id}");
                self.subscribe(
                    interval,
                    key,
                    Box::new(move || {
                        let action = action.clone();
                        let udn = udn.clone();
                        let service_id = service_id.clone();
                        Box::pin(async move {
                            if let Some(device) = super::get_device(&udn) {
                                if let Some(service) = super::get_service(&udn, &service_id) {
                                    match super::get_media_info(&service, device.url(), instance_id)
                                        .await
                                    {
                                        Ok(info) => {
                                            action(info).await;
                                        }
                                        Err(e) => {
                                            log::error!("Failed to get_media_info: {e:?}");
                                        }
                                    }
                                } else {
                                    log::debug!(
                                        "No service with device_udn={udn} service_id={service_id}"
                                    );
                                }
                            } else {
                                log::debug!("No device with udn={udn}");
                            }
                        })
                    }),
                )
                .await?;
            }
            UpnpCommand::UnsubscribeMediaInfo {
                instance_id,
                udn,
                service_id,
            } => {
                let key = format!("MediaInfo:{instance_id}:{udn}:{service_id}");
                self.unsubscribe(key).await?;
            }
            UpnpCommand::SubscribePositionInfo {
                interval,
                instance_id,
                udn,
                service_id,
                action,
            } => {
                let action = Arc::new(action);
                let key = format!("PositionInfo:{instance_id}:{udn}:{service_id}");
                self.subscribe(
                    interval,
                    key,
                    Box::new(move || {
                        let action = action.clone();
                        let udn = udn.clone();
                        let service_id = service_id.clone();
                        Box::pin(async move {
                            if let Some(device) = super::get_device(&udn) {
                                if let Some(service) = super::get_service(&udn, &service_id) {
                                    match super::get_position_info(
                                        &service,
                                        device.url(),
                                        instance_id,
                                    )
                                    .await
                                    {
                                        Ok(info) => {
                                            action(info).await;
                                        }
                                        Err(e) => {
                                            log::error!("Failed to get_position_info: {e:?}");
                                        }
                                    }
                                } else {
                                    log::debug!(
                                        "No service with device_udn={udn} service_id={service_id}"
                                    );
                                }
                            } else {
                                log::debug!("No device with udn={udn}");
                            }
                        })
                    }),
                )
                .await?;
            }
            UpnpCommand::UnsubscribePositionInfo {
                instance_id,
                udn,
                service_id,
            } => {
                let key = format!("PositionInfo:{instance_id}:{udn}:{service_id}");
                self.unsubscribe(key).await?;
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

type SubscriptionAction = Box<dyn (Fn() -> Pin<Box<dyn Future<Output = ()> + Send>>) + Send>;
pub type MediaInfoSubscriptionAction = Box<
    dyn (Fn(HashMap<String, String>) -> Pin<Box<dyn Future<Output = ()> + Send>>) + Send + Sync,
>;
pub type PositionInfoSubscriptionAction = Box<
    dyn (Fn(HashMap<String, String>) -> Pin<Box<dyn Future<Output = ()> + Send>>) + Send + Sync,
>;

pub trait UpnpCommander {
    type Error;

    fn subscribe_media_info(
        &self,
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: MediaInfoSubscriptionAction,
    ) -> Result<(), Self::Error>;
    fn unsubscribe_media_info(
        &self,
        instance_id: u32,
        udn: String,
        service_id: String,
    ) -> Result<(), Self::Error>;
    fn subscribe_position_info(
        &self,
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        func: PositionInfoSubscriptionAction,
    ) -> Result<(), Self::Error>;
    fn unsubscribe_position_info(
        &self,
        instance_id: u32,
        udn: String,
        service_id: String,
    ) -> Result<(), Self::Error>;
    fn shutdown(&self) -> Result<(), Self::Error>;
}

#[derive(Clone)]
pub struct UpnpListenerHandle {
    sender: Sender<UpnpCommand>,
    token: CancellationToken,
}

impl UpnpCommander for UpnpListener {
    type Error = SendError<UpnpCommand>;

    fn subscribe_media_info(
        &self,
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: MediaInfoSubscriptionAction,
    ) -> Result<(), Self::Error> {
        self.sender.send(UpnpCommand::SubscribeMediaInfo {
            interval,
            instance_id,
            udn,
            service_id,
            action,
        })
    }

    fn unsubscribe_media_info(
        &self,
        instance_id: u32,
        udn: String,
        service_id: String,
    ) -> Result<(), Self::Error> {
        self.sender.send(UpnpCommand::UnsubscribeMediaInfo {
            instance_id,
            udn,
            service_id,
        })
    }

    fn subscribe_position_info(
        &self,
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: PositionInfoSubscriptionAction,
    ) -> Result<(), Self::Error> {
        self.sender.send(UpnpCommand::SubscribePositionInfo {
            interval,
            instance_id,
            udn,
            service_id,
            action,
        })
    }

    fn unsubscribe_position_info(
        &self,
        instance_id: u32,
        udn: String,
        service_id: String,
    ) -> Result<(), Self::Error> {
        self.sender.send(UpnpCommand::UnsubscribePositionInfo {
            instance_id,
            udn,
            service_id,
        })
    }

    fn shutdown(&self) -> Result<(), Self::Error> {
        log::debug!("Shutting down UpnpListener");
        self.token.cancel();
        Ok(())
    }
}

impl UpnpCommander for UpnpListenerHandle {
    type Error = SendError<UpnpCommand>;

    fn subscribe_media_info(
        &self,
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: MediaInfoSubscriptionAction,
    ) -> Result<(), Self::Error> {
        self.sender.send(UpnpCommand::SubscribeMediaInfo {
            interval,
            instance_id,
            udn,
            service_id,
            action,
        })
    }

    fn unsubscribe_media_info(
        &self,
        instance_id: u32,
        udn: String,
        service_id: String,
    ) -> Result<(), Self::Error> {
        self.sender.send(UpnpCommand::UnsubscribeMediaInfo {
            instance_id,
            udn,
            service_id,
        })
    }

    fn subscribe_position_info(
        &self,
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: PositionInfoSubscriptionAction,
    ) -> Result<(), Self::Error> {
        self.sender.send(UpnpCommand::SubscribePositionInfo {
            interval,
            instance_id,
            udn,
            service_id,
            action,
        })
    }

    fn unsubscribe_position_info(
        &self,
        instance_id: u32,
        udn: String,
        service_id: String,
    ) -> Result<(), Self::Error> {
        self.sender.send(UpnpCommand::UnsubscribePositionInfo {
            instance_id,
            udn,
            service_id,
        })
    }

    fn shutdown(&self) -> Result<(), Self::Error> {
        log::debug!("Shutting down UpnpListener");
        self.token.cancel();
        Ok(())
    }
}
