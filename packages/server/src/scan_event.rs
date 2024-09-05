use crate::WS_SERVER_HANDLE;

pub async fn init() {
    moosicbox_scan::event::add_progress_listener(Box::new(move |event| {
        let event = event.clone();

        Box::pin(async move {
            let api_event: Option<moosicbox_scan::event::ApiProgressEvent> = event.into();
            if let Some(api_event) = api_event {
                if let Err(err) = moosicbox_ws::send_scan_event(
                    WS_SERVER_HANDLE.read().await.as_ref().unwrap(),
                    None,
                    api_event,
                )
                .await
                {
                    log::error!("Failed to broadcast scan event: {err:?}");
                }
            }
        }) as moosicbox_scan::event::ProgressListenerRefFut
    }))
    .await;
}
