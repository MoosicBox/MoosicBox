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

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::Stream;
    use moosicbox_channel_utils::futures_channel::unbounded;
    use std::{
        pin::Pin,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Poll},
    };
    use tokio_tungstenite::tungstenite::Message;

    /// Tracks call counts for `MockWebsocketSender`
    #[derive(Clone, Default)]
    struct CallTracker {
        send: Arc<AtomicUsize>,
        send_all: Arc<AtomicUsize>,
        send_all_except: Arc<AtomicUsize>,
        ping: Arc<AtomicUsize>,
    }

    /// Mock websocket sender that tracks which methods were called
    struct MockWebsocketSender {
        tracker: CallTracker,
    }

    impl MockWebsocketSender {
        fn new() -> Self {
            Self {
                tracker: CallTracker::default(),
            }
        }
    }

    #[async_trait]
    impl WebsocketSender for MockWebsocketSender {
        async fn send(&self, _connection_id: &str, _data: &str) -> Result<(), WebsocketSendError> {
            self.tracker.send.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn send_all(&self, _data: &str) -> Result<(), WebsocketSendError> {
            self.tracker.send_all.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn send_all_except(
            &self,
            _connection_id: &str,
            _data: &str,
        ) -> Result<(), WebsocketSendError> {
            self.tracker.send_all_except.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn ping(&self) -> Result<(), WebsocketSendError> {
            self.tracker.ping.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    type TestReceiver =
        moosicbox_channel_utils::futures_channel::PrioritizedReceiver<TunnelResponseMessage>;

    struct TestSetup {
        sender: TunnelWebsocketSender<MockWebsocketSender>,
        rx: TestReceiver,
        tracker: CallTracker,
    }

    fn create_test_sender(id: u64, propagate_id: u64) -> TestSetup {
        let (tx, rx) = unbounded();
        let mock_sender = MockWebsocketSender::new();
        let tracker = mock_sender.tracker.clone();

        let sender = TunnelWebsocketSender {
            id,
            propagate_id,
            request_id: 12345,
            packet_id: 1,
            root_sender: mock_sender,
            tunnel_sender: tx,
            profile: Some("test".to_string()),
        };

        TestSetup {
            sender,
            rx,
            tracker,
        }
    }

    /// Polls the receiver once and returns the message if ready
    fn poll_receiver(
        rx: &mut moosicbox_channel_utils::futures_channel::PrioritizedReceiver<
            TunnelResponseMessage,
        >,
    ) -> Poll<Option<TunnelResponseMessage>> {
        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);
        Pin::new(rx).poll_next(&mut context)
    }

    #[test_log::test(tokio::test)]
    async fn test_send_routes_to_tunnel_when_id_matches() {
        let tunnel_id = 100;
        let propagate_id = 200;
        let TestSetup {
            sender,
            mut rx,
            tracker,
        } = create_test_sender(tunnel_id, propagate_id);

        // Send to connection ID that matches tunnel ID - should route to tunnel
        let result = sender
            .send(&tunnel_id.to_string(), r#"{"test": "data"}"#)
            .await;
        assert!(result.is_ok());

        // Root sender should NOT be called
        assert_eq!(tracker.send.load(Ordering::SeqCst), 0);

        // Tunnel should receive the message
        let poll_result = poll_receiver(&mut rx);
        match poll_result {
            Poll::Ready(Some(TunnelResponseMessage::Packet(packet))) => {
                assert_eq!(packet.request_id, 12345);
                assert_eq!(packet.packet_id, 1);
                assert!(!packet.broadcast);
                assert!(packet.except_id.is_none());
                assert_eq!(packet.only_id, Some(propagate_id));
                match packet.message {
                    Message::Text(text) => {
                        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                        assert_eq!(value["request_id"], 12345);
                        assert_eq!(value["body"]["test"], "data");
                    }
                    _ => panic!("Expected text message"),
                }
            }
            _ => panic!("Expected Packet message"),
        }
    }

    #[test_log::test(tokio::test)]
    async fn test_send_routes_to_root_when_id_does_not_match() {
        let tunnel_id = 100;
        let propagate_id = 200;
        let other_id = 300;
        let TestSetup {
            sender,
            mut rx,
            tracker,
        } = create_test_sender(tunnel_id, propagate_id);

        // Send to connection ID that does NOT match tunnel ID - should route to root
        let result = sender
            .send(&other_id.to_string(), r#"{"test": "data"}"#)
            .await;
        assert!(result.is_ok());

        // Root sender SHOULD be called
        assert_eq!(tracker.send.load(Ordering::SeqCst), 1);

        // Tunnel should NOT receive any message
        let poll_result = poll_receiver(&mut rx);
        assert!(matches!(poll_result, Poll::Pending));
    }

    #[test_log::test(tokio::test)]
    async fn test_send_all_routes_to_both_tunnel_and_root() {
        let tunnel_id = 100;
        let propagate_id = 200;
        let TestSetup {
            sender,
            mut rx,
            tracker,
        } = create_test_sender(tunnel_id, propagate_id);

        let result = sender.send_all(r#"{"broadcast": true}"#).await;
        assert!(result.is_ok());

        // Root sender SHOULD be called
        assert_eq!(tracker.send_all.load(Ordering::SeqCst), 1);

        // Tunnel should also receive the message
        let poll_result = poll_receiver(&mut rx);
        match poll_result {
            Poll::Ready(Some(TunnelResponseMessage::Packet(packet))) => {
                assert!(packet.broadcast);
                assert!(packet.except_id.is_none());
                assert!(packet.only_id.is_none());
            }
            _ => panic!("Expected Packet message"),
        }
    }

    #[test_log::test(tokio::test)]
    async fn test_send_all_except_routes_to_tunnel_when_not_propagate_id() {
        let tunnel_id = 100;
        let propagate_id = 200;
        let other_id = 300;
        let TestSetup {
            sender,
            mut rx,
            tracker,
        } = create_test_sender(tunnel_id, propagate_id);

        // Exclude a different ID - should send to tunnel
        let result = sender
            .send_all_except(&other_id.to_string(), r#"{"except": true}"#)
            .await;
        assert!(result.is_ok());

        // Root sender SHOULD be called
        assert_eq!(tracker.send_all_except.load(Ordering::SeqCst), 1);

        // Tunnel should receive the message with propagate_id excluded
        let poll_result = poll_receiver(&mut rx);
        match poll_result {
            Poll::Ready(Some(TunnelResponseMessage::Packet(packet))) => {
                assert!(packet.broadcast);
                assert_eq!(packet.except_id, Some(propagate_id));
                assert!(packet.only_id.is_none());
            }
            _ => panic!("Expected Packet message"),
        }
    }

    #[test_log::test(tokio::test)]
    async fn test_send_all_except_skips_tunnel_when_propagate_id() {
        let tunnel_id = 100;
        let propagate_id = 200;
        let TestSetup {
            sender,
            mut rx,
            tracker,
        } = create_test_sender(tunnel_id, propagate_id);

        // Exclude the propagate_id - should NOT send to tunnel
        let result = sender
            .send_all_except(&propagate_id.to_string(), r#"{"except": true}"#)
            .await;
        assert!(result.is_ok());

        // Root sender SHOULD still be called
        assert_eq!(tracker.send_all_except.load(Ordering::SeqCst), 1);

        // Tunnel should NOT receive any message
        let poll_result = poll_receiver(&mut rx);
        assert!(matches!(poll_result, Poll::Pending));
    }

    #[test_log::test(tokio::test)]
    async fn test_ping_delegates_to_root_sender() {
        let tunnel_id = 100;
        let propagate_id = 200;
        let TestSetup {
            sender, tracker, ..
        } = create_test_sender(tunnel_id, propagate_id);

        let result = sender.ping().await;
        assert!(result.is_ok());

        // Root sender SHOULD be called
        assert_eq!(tracker.ping.load(Ordering::SeqCst), 1);
    }
}
