use moosicbox_async_service::Arc;
use moosicbox_database::Database;
use moosicbox_session::events::BoxErrorSend;

use crate::WS_SERVER_HANDLE;

pub async fn init(db: Arc<Box<dyn Database>>) {
    moosicbox_session::events::on_players_updated_event({
        let db = db.clone();
        move || {
            let db = db.clone();
            async move {
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
                    .ok_or(moosicbox_ws::WebsocketSendError::Unknown(
                        "No ws server handle".into(),
                    ))
                    .map_err(|e| Box::new(e) as BoxErrorSend)?;
                moosicbox_ws::get_sessions(&**db, &handle, &context, true)
                    .await
                    .map_err(|e| Box::new(e) as BoxErrorSend)?;
                moosicbox_ws::broadcast_connections(&**db, &handle)
                    .await
                    .map_err(|e| Box::new(e) as BoxErrorSend)?;
                Ok(())
            }
        }
    })
    .await;
}
