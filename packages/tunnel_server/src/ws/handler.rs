//! WebSocket connection handler for tunnel clients.
//!
//! This module implements the main WebSocket message loop that handles client
//! connections. It manages heartbeats, message routing (text/binary), and graceful
//! connection shutdown. The handler processes incoming WebSocket frames and routes
//! tunnel responses back through the server.

#![allow(clippy::future_not_send)]

use std::time::Duration;

use actix_ws::Message;
use futures_util::{
    StreamExt as _,
    future::{Either, select},
};
use moosicbox_tunnel::TunnelWsResponse;
use switchy_async::sync::mpsc;
use tokio::{pin, time::interval};

use super::server::service::CommanderError;

/// How often heartbeat pings are sent
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// How long before lack of client response causes a timeout
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// Handle WebSocket connection lifecycle and message routing.
///
/// This function manages the WebSocket connection for a tunnel client. It handles
/// incoming messages (ping/pong, text, binary), routes tunnel responses, maintains
/// connection health via heartbeats, and cleans up resources on disconnection.
///
/// The function runs until the connection is closed by either the client or server,
/// or until a heartbeat timeout occurs.
///
/// # Errors
///
/// * Returns [`CommanderError`] if communication with the WebSocket server fails.
///
/// # Panics
///
/// * Panics if sending a pong response fails.
/// * Panics if parsing binary tunnel response data fails.
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
pub async fn handle_ws(
    ws_server: super::server::service::Handle,
    mut session: actix_ws::Session,
    mut msg_stream: actix_ws::MessageStream,
    client_id: String,
    sender: bool,
    profile: Option<String>,
) -> Result<(), CommanderError> {
    log::debug!("Connected");

    let mut last_heartbeat = switchy_time::instant_now();
    let mut interval = interval(HEARTBEAT_INTERVAL);

    let (conn_tx, mut conn_rx) = mpsc::unbounded();

    // unwrap: ws server is not dropped before the HTTP server
    let conn_id = ws_server.connect(&client_id, sender, conn_tx).await?;

    log::debug!("Connection id: {conn_id}");

    let close_reason = loop {
        // most of the futures we process need to be stack-pinned to work with select()

        let tick = interval.tick();
        pin!(tick);

        let msg_rx = conn_rx.recv_async();
        pin!(msg_rx);

        // TODO: nested select is pretty gross for readability on the match
        let messages = select(msg_stream.next(), msg_rx);
        pin!(messages);

        match select(messages, tick).await {
            // commands & messages received from client
            Either::Left((Either::Left((Some(Ok(msg)), _)), _)) => match msg {
                Message::Ping(bytes) => {
                    log::trace!("Received ping");
                    last_heartbeat = switchy_time::instant_now();
                    session.pong(&bytes).await.unwrap();
                }

                Message::Pong(_) => {
                    last_heartbeat = switchy_time::instant_now();
                }

                Message::Text(text) => {
                    last_heartbeat = switchy_time::instant_now();
                    let text: &str = text.as_ref();

                    #[cfg(feature = "base64")]
                    let finished = if let Ok(response) = text.try_into() {
                        ws_server.response(conn_id, response).await;
                        true
                    } else {
                        false
                    };

                    #[cfg(not(feature = "base64"))]
                    let finished = false;

                    if !finished {
                        if sender {
                            if let Ok(response) = serde_json::from_str::<TunnelWsResponse>(text) {
                                if response.request_id == 0 {
                                    log::debug!("Propagating ws message {text}");
                                    if let Err(err) = ws_server.ws_message(response).await {
                                        log::error!(
                                            "Failed to propagate ws message from tunnel_server: {err:?}"
                                        );
                                    }
                                } else {
                                    log::debug!("Propagating ws response");
                                    if let Err(err) = ws_server.ws_response(response).await {
                                        log::error!(
                                            "Failed to propagate ws response from tunnel_server: {err:?}"
                                        );
                                    }
                                }
                            } else {
                                log::error!("Invalid TunnelWsResponse: {text}");
                            }
                        } else if let Err(err) = ws_server
                            .ws_request(conn_id, &client_id, profile.clone(), text)
                            .await
                        {
                            log::error!(
                                "Failed to propagate ws request from tunnel_server: {err:?}"
                            );
                        }
                    }
                }

                Message::Binary(bytes) => {
                    last_heartbeat = switchy_time::instant_now();

                    ws_server.response(conn_id, bytes.try_into().unwrap()).await;
                }

                Message::Close(reason) => break reason,

                _ => {
                    break None;
                }
            },

            // client WebSocket stream error
            Either::Left((Either::Left((Some(Err(err)), _)), _)) => {
                log::error!("WebSocket stream error: {err}");
                break None;
            }

            // client WebSocket stream ended
            Either::Left((Either::Left((None, _)), _)) => {
                log::debug!("WebSocket stream ended");
                break None;
            }

            // ws messages received from other room participants
            Either::Left((Either::Right((Ok(ws_msg), _)), _)) => {
                if let Err(err) = session.text(ws_msg).await {
                    log::error!(
                        "Failed to send text message to conn_id='{conn_id}' client_id='{client_id}': {err:?}"
                    );
                }
            }

            // all connection's message senders were dropped
            Either::Left((Either::Right((Err(_), _)), _)) => unreachable!(
                "all connection message senders were dropped; ws server may have panicked"
            ),

            // heartbeat internal tick
            Either::Right((_inst, _)) => {
                // if no heartbeat ping/pong received recently, close the connection
                if switchy_time::instant_now().duration_since(last_heartbeat) > CLIENT_TIMEOUT {
                    log::info!(
                        "client has not sent heartbeat in over {CLIENT_TIMEOUT:?}; disconnecting"
                    );
                    break None;
                }

                // send heartbeat ping
                let _ = session.ping(b"").await;
            }
        }
    };

    log::debug!("handle_ws: disconnecting connection");
    ws_server.disconnect(conn_id).await;

    // attempt to close connection gracefully
    log::debug!("handle_ws: closing connection");
    let _ = session.close(close_reason).await;

    Ok(())
}
