use moosicbox_async_service::Arc;
use moosicbox_audio_zone::events::BoxErrorSend;
use moosicbox_database::Database;

use crate::WS_SERVER_HANDLE;

pub async fn init(db: Arc<Box<dyn Database>>) {
    moosicbox_audio_zone::events::on_audio_zones_updated_event({
        let db = db.clone();
        move || {
            let db = db.clone();
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
                    .ok_or(moosicbox_ws::WebsocketSendError::Unknown(
                        "No ws server handle".into(),
                    ))
                    .map_err(|e| Box::new(e) as BoxErrorSend)?;
                moosicbox_ws::broadcast_audio_zones(&**db, &handle, &context, true)
                    .await
                    .map_err(|e| Box::new(e) as BoxErrorSend)?;
                Ok(())
            }
        }
    })
    .await;
}