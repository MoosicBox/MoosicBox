use async_trait::async_trait;
use futures_channel::mpsc::UnboundedSender;
use moosicbox_ws::api::{WebsocketSendError, WebsocketSender};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

use super::tunnel_sender::TunnelResponseMessage;

pub struct TempSender<T>
where
    T: WebsocketSender + Send + Sync,
{
    pub id: usize,
    pub request_id: usize,
    pub packet_id: u32,
    pub root_sender: T,
    pub tunnel_sender: UnboundedSender<TunnelResponseMessage>,
}

impl<T> TempSender<T>
where
    T: WebsocketSender + Send + Sync,
{
    fn send_tunnel(&self, data: &str) {
        let body: Value = serde_json::from_str(data).unwrap();
        let request_id = self.request_id;
        let packet_id = self.packet_id;
        let value = json!({"request_id": request_id, "body": body});

        self.tunnel_sender
            .unbounded_send(TunnelResponseMessage {
                request_id,
                packet_id,
                message: Message::Text(value.to_string()),
            })
            .unwrap();
    }
}

#[async_trait]
impl<T> WebsocketSender for TempSender<T>
where
    T: WebsocketSender + Send + Sync,
{
    async fn send(&self, connection_id: &str, data: &str) -> Result<(), WebsocketSendError> {
        let id = connection_id.parse::<usize>().unwrap();

        if id == self.id {
            self.send_tunnel(data);
        } else {
            self.root_sender.send(connection_id, data).await?;
        }

        Ok(())
    }

    async fn send_all(&self, data: &str) -> Result<(), WebsocketSendError> {
        self.send_tunnel(data);

        self.root_sender.send_all(data).await?;

        Ok(())
    }

    async fn send_all_except(
        &self,
        connection_id: &str,
        data: &str,
    ) -> Result<(), WebsocketSendError> {
        let id = connection_id.parse::<usize>().unwrap();

        if id != self.id {
            self.send_tunnel(data);
        }

        self.root_sender
            .send_all_except(connection_id, data)
            .await?;

        Ok(())
    }
}
