use std::time::{Duration, Instant};

use actix_ws::Message;
use futures_util::{
    future::{select, Either},
    StreamExt as _,
};
use log::{debug, error};
use moosicbox_tunnel::tunnel::TunnelWsResponse;
use serde_json::Value;
use tokio::{pin, sync::mpsc, time::interval};

use crate::ws::server::ChatServerHandle;

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// Echo text & binary messages received from the client, respond to ping messages, and monitor
/// connection health to detect network issues and free up resources.
pub async fn chat_ws(
    chat_server: ChatServerHandle,
    mut session: actix_ws::Session,
    mut msg_stream: actix_ws::MessageStream,
    client_id: String,
    sender: bool,
) {
    log::info!("Connected");

    let mut last_heartbeat = Instant::now();
    let mut interval = interval(HEARTBEAT_INTERVAL);

    let (conn_tx, mut conn_rx) = mpsc::unbounded_channel();

    // unwrap: chat server is not dropped before the HTTP server
    let conn_id = chat_server.connect(&client_id, sender, conn_tx).await;

    log::info!("Connection id: {conn_id}");

    let close_reason = loop {
        // most of the futures we process need to be stack-pinned to work with select()

        let tick = interval.tick();
        pin!(tick);

        let msg_rx = conn_rx.recv();
        pin!(msg_rx);

        // TODO: nested select is pretty gross for readability on the match
        let messages = select(msg_stream.next(), msg_rx);
        pin!(messages);

        match select(messages, tick).await {
            // commands & messages received from client
            Either::Left((Either::Left((Some(Ok(msg)), _)), _)) => match msg {
                Message::Ping(bytes) => {
                    last_heartbeat = Instant::now();
                    session.pong(&bytes).await.unwrap();
                }

                Message::Pong(_) => {
                    last_heartbeat = Instant::now();
                }

                Message::Text(text) => {
                    last_heartbeat = Instant::now();
                    let text: &str = text.as_ref();
                    if let Ok(response) = text.try_into() {
                        chat_server.response(response);
                    } else if sender {
                        debug!("Propagating ws response");
                        let value: Value = serde_json::from_str(text).unwrap();
                        if let Err(err) = chat_server
                            .ws_response(TunnelWsResponse {
                                request_id: serde_json::from_value(
                                    value.get("request_id").unwrap().clone(),
                                )
                                .unwrap(),
                                body: value.get("body").unwrap().clone(),
                            })
                            .await
                        {
                            error!("Failed to propagate ws response from tunnel_server: {err:?}");
                        }
                    } else if let Err(err) = chat_server.ws_request(conn_id, &client_id, text).await
                    {
                        error!("Failed to propagate ws request from tunnel_server: {err:?}");
                    }
                }

                Message::Binary(bytes) => {
                    last_heartbeat = Instant::now();

                    chat_server.response(bytes.into());
                }

                Message::Close(reason) => break reason,

                _ => {
                    break None;
                }
            },

            // client WebSocket stream error
            Either::Left((Either::Left((Some(Err(err)), _)), _)) => {
                log::error!("{}", err);
                break None;
            }

            // client WebSocket stream ended
            Either::Left((Either::Left((None, _)), _)) => break None,

            // chat messages received from other room participants
            Either::Left((Either::Right((Some(chat_msg), _)), _)) => {
                session.text(chat_msg).await.unwrap();
            }

            // all connection's message senders were dropped
            Either::Left((Either::Right((None, _)), _)) => unreachable!(
                "all connection message senders were dropped; chat server may have panicked"
            ),

            // heartbeat internal tick
            Either::Right((_inst, _)) => {
                // if no heartbeat ping/pong received recently, close the connection
                if Instant::now().duration_since(last_heartbeat) > CLIENT_TIMEOUT {
                    log::info!(
                        "client has not sent heartbeat in over {CLIENT_TIMEOUT:?}; disconnecting"
                    );
                    break None;
                }

                // send heartbeat ping
                let _ = session.ping(b"").await;
            }
        };
    };

    chat_server.disconnect(conn_id);

    // attempt to close connection gracefully
    let _ = session.close(close_reason).await;
}
