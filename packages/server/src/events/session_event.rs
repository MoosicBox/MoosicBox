//! Session and player event handling and WebSocket broadcasting.
//!
//! This module initializes event listeners for player registration/deregistration and session
//! updates, broadcasting changes to connected WebSocket clients.

use moosicbox_session::events::BoxErrorSend;
use switchy_database::profiles::PROFILES;

use crate::{CONFIG_DB, WS_SERVER_HANDLE};

/// Initializes session and player event listeners.
///
/// Sets up an event handler that broadcasts session and player updates to all connected WebSocket
/// clients whenever players are registered, deregistered, or their state changes.
pub async fn init() {
    moosicbox_session::events::on_players_updated_event({
        move || async move {
            log::debug!("on_players_updated_event: Players updated");
            let connection_id = "self";
            let context = moosicbox_ws::WebsocketContext {
                connection_id: connection_id.to_string(),
                ..Default::default()
            };
            let handle = WS_SERVER_HANDLE
                .read()
                .await
                .clone()
                .ok_or_else(|| {
                    moosicbox_ws::WebsocketSendError::Unknown("No ws server handle".into())
                })
                .map_err(|e| Box::new(e) as BoxErrorSend)?;
            for profile in PROFILES.names() {
                if let Some(db) = PROFILES.get(&profile) {
                    moosicbox_ws::broadcast_sessions(&db, &handle, &context, true)
                        .await
                        .map_err(|e| Box::new(e) as BoxErrorSend)?;
                } else {
                    log::error!("Failed to get database for profile '{profile}'");
                }
            }
            let config_db = { CONFIG_DB.read().unwrap().clone().unwrap() };
            moosicbox_ws::broadcast_connections(&config_db, &handle)
                .await
                .map_err(|e| Box::new(e) as BoxErrorSend)?;
            Ok(())
        }
    })
    .await;
}
