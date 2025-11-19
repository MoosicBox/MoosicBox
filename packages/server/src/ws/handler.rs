//! WebSocket connection handler implementation.
//!
//! This module provides the main WebSocket message handling loop including heartbeat monitoring,
//! message processing, and graceful connection cleanup.

use std::time::Duration;

use actix_ws::Message;
use futures_util::{
    StreamExt as _,
    future::{Either, select},
};
use tokio::{pin, sync::mpsc, time::interval};

use crate::ws::{ConnId, server::WsServerHandle};

/// How often heartbeat pings are sent to WebSocket clients.
///
/// The server sends a ping every 5 seconds to verify the connection is still alive.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// How long to wait for a client response before timing out the connection.
///
/// If no pong or message is received within 10 seconds, the connection is considered dead.
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);

/// Handles a WebSocket connection for a single client.
///
/// This function manages the full lifecycle of a WebSocket connection including:
/// * Registering the connection with the WebSocket server
/// * Processing incoming messages (text, binary, ping/pong)
/// * Sending outgoing messages from the server
/// * Monitoring connection health via heartbeat mechanism
/// * Gracefully closing the connection on timeout or client disconnect
///
/// The function runs until the client disconnects or times out, then performs cleanup.
#[cfg_attr(feature = "profiling", profiling::function)]
#[allow(clippy::future_not_send)]
pub async fn handle_ws(
    ws_server: WsServerHandle,
    profile: String,
    mut session: actix_ws::Session,
    mut msg_stream: actix_ws::MessageStream,
) {
    log::debug!("connected");

    let mut name = None;
    let mut last_heartbeat = switchy_time::instant_now();
    let mut interval = interval(HEARTBEAT_INTERVAL);

    let (conn_tx, mut conn_rx) = mpsc::unbounded_channel();

    let conn_id = ws_server.connect(profile, conn_tx).await;

    let close_reason = loop {
        #[cfg(feature = "profiling")]
        profiling::function_scope!("loop");

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
                    last_heartbeat = switchy_time::instant_now();
                    session.pong(&bytes).await.unwrap();
                }

                Message::Pong(_) => {
                    last_heartbeat = switchy_time::instant_now();
                }

                Message::Text(text) => {
                    process_text_msg(&ws_server, &text, conn_id, &mut name).await;
                }

                Message::Binary(bytes) => match String::from_utf8(bytes.to_vec()) {
                    Ok(text) => {
                        process_text_msg(&ws_server, &text, conn_id, &mut name).await;
                    }
                    Err(e) => {
                        log::warn!("unexpected binary message: {e:?}");
                    }
                },

                Message::Close(reason) => break reason,

                _ => {
                    break None;
                }
            },

            // client WebSocket stream error
            Either::Left((Either::Left((Some(Err(err)), _)), _)) => {
                log::error!("{err}");
                break None;
            }

            // client WebSocket stream ended
            Either::Left((Either::Left((None, _)), _)) => break None,

            // ws messages received from other room participants
            Either::Left((Either::Right((Some(ws_msg), _)), _)) => {
                if let Err(err) = session.text(ws_msg).await {
                    log::error!("Failed to send text message: {err:?}");
                }
            }

            // all connection's message senders were dropped
            Either::Left((Either::Right((None, _)), _)) => unreachable!(
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

    ws_server.disconnect(conn_id).await;

    // attempt to close connection gracefully
    let _ = session.close(close_reason).await;
}

/// Processes a text message received from a WebSocket client.
///
/// This function trims the message, optionally prefixes it with the client's name if set,
/// and forwards it to the WebSocket server for processing and potential broadcast.
async fn process_text_msg(
    ws_server: &WsServerHandle,
    text: &str,
    conn: ConnId,
    name: &mut Option<String>,
) {
    // strip leading and trailing whitespace (spaces, newlines, etc.)
    let msg = text.trim();

    // prefix message with our name, if assigned
    let msg = name
        .as_mut()
        .map_or_else(|| msg.to_owned(), |name| format!("{name}: {msg}"));

    ws_server.send_message(conn, msg).await;
}
