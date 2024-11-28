use moosicbox_audio_zone::events::BoxErrorSend;
use moosicbox_database::{config::ConfigDatabase, profiles::PROFILES};

use crate::WS_SERVER_HANDLE;

pub async fn init(config_db: &ConfigDatabase) {
    let config_db = config_db.to_owned();
    moosicbox_audio_zone::events::on_audio_zones_updated_event({
        let config_db = config_db.clone();
        move || {
            let config_db = config_db.clone();
            async move {
                log::debug!("on_audio_zones_updated_event: Audio zones updated");
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
                    if let Some(library_db) = PROFILES.get(&profile) {
                        moosicbox_ws::broadcast_audio_zones(
                            &config_db,
                            &library_db,
                            &handle,
                            &context,
                            true,
                        )
                        .await
                        .map_err(|e| Box::new(e) as BoxErrorSend)?;
                    } else {
                        log::error!("Failed to get database for profile '{profile}'");
                    }
                }
                Ok(())
            }
        }
    })
    .await;
}
