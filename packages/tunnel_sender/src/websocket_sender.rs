//! WebSocket sender implementation for routing messages through tunnel connections.
//!
//! This module provides [`TunnelWebsocketSender`] which manages message propagation
//! across both local and tunnel WebSocket connections with connection filtering support.

#![allow(clippy::module_name_repetitions)]

use async_trait::async_trait;
use futures_channel::mpsc::TrySendError;
use moosicbox_channel_utils::{MoosicBoxSender as _, futures_channel::PrioritizedSender};
use moosicbox_ws::{WebsocketSendError, WebsocketSender};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;

use crate::sender::{TunnelResponseMessage, TunnelResponsePacket};

/// Websocket sender that routes messages through both local and tunnel connections.
///
/// Manages message propagation and connection filtering for tunnel websocket operations.
pub struct TunnelWebsocketSender<T>
where
    T: WebsocketSender + Send + Sync,
{
    /// The unique identifier for this tunnel sender instance.
    pub id: u64,
    /// The connection ID to propagate messages to through the tunnel.
    pub propagate_id: u64,
    /// The tunnel request identifier associated with this sender.
    pub request_id: u64,
    /// The packet sequence number for this sender's messages.
    pub packet_id: u32,
    /// The underlying local WebSocket sender.
    pub root_sender: T,
    /// The channel sender for tunnel response messages.
    pub tunnel_sender: PrioritizedSender<TunnelResponseMessage>,
    /// Optional profile identifier for connection context.
    pub profile: Option<String>,
}

impl<T> TunnelWebsocketSender<T>
where
    T: WebsocketSender + Send + Sync,
{
    /// Sends a message through the tunnel with connection filtering options.
    ///
    /// # Errors
    ///
    /// * If the tunnel sender channel is full or disconnected
    ///
    /// # Panics
    ///
    /// * If `data` is not valid JSON
    fn send_tunnel(
        &self,
        data: &str,
        broadcast: bool,
        except_id: Option<u64>,
        only_id: Option<u64>,
    ) -> Result<(), TrySendError<TunnelResponseMessage>> {
        let body: Value = serde_json::from_str(data).unwrap();
        let request_id = self.request_id;
        let packet_id = self.packet_id;
        let value = json!({"request_id": request_id, "body": body});

        self.tunnel_sender
            .send(TunnelResponseMessage::Packet(TunnelResponsePacket {
                request_id,
                packet_id,
                broadcast,
                except_id,
                only_id,
                message: Message::Text(value.to_string().into()),
            }))
    }
}

#[async_trait]
impl<T> WebsocketSender for TunnelWebsocketSender<T>
where
    T: WebsocketSender + Send + Sync,
{
    /// Sends a message to a specific connection, routing through tunnel if needed.
    ///
    /// # Errors
    ///
    /// * If the underlying root sender fails to send the message
    ///
    /// # Panics
    ///
    /// * If `connection_id` cannot be parsed as a `u64`
    async fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        let id = connection_id.parse::<u64>().unwrap();

        if id == self.id {
            if self
                .send_tunnel(data, false, None, Some(self.propagate_id))
                .is_err()
            {
                log::error!("Failed to send tunnel message");
            }
        } else {
            self.root_sender.send(connection_id, data).await?;
        }

        Ok(())
    }

    /// Sends a message to all connections, including both local and tunnel connections.
    ///
    /// # Errors
    ///
    /// * If the underlying root sender fails to send the message
    async fn send_all(&self, data: &str) -> Result<(), WebsocketSendError> {
        if self.send_tunnel(data, true, None, None).is_err() {
            log::error!("Failed to send tunnel message");
        }

        self.root_sender.send_all(data).await?;

        Ok(())
    }

    /// Sends a message to all connections except the specified one.
    ///
    /// # Errors
    ///
    /// * If the underlying root sender fails to send the message
    ///
    /// # Panics
    ///
    /// * If `connection_id` cannot be parsed as a `u64`
    async fn send_all_except(
        &self,
        connection_id: &str,
        data: &str,
    ) -> Result<(), WebsocketSendError> {
        let id = connection_id.parse::<u64>().unwrap();

        if id != self.propagate_id
            && self
                .send_tunnel(data, true, Some(self.propagate_id), None)
                .is_err()
        {
            log::error!("Failed to send tunnel message");
        }

        self.root_sender
            .send_all_except(connection_id, data)
            .await?;

        Ok(())
    }

    /// Sends a ping control message to the underlying connection.
    ///
    /// # Errors
    ///
    /// * If the underlying root sender fails to send the ping
    async fn ping(&self) -> Result<(), WebsocketSendError> {
        self.root_sender.ping().await
    }
}
