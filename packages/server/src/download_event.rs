use moosicbox_async_service::Arc;

use crate::WS_SERVER_HANDLE;

pub async fn init() {
    let bytes_throttle = Arc::new(std::sync::Mutex::new(throttle::Throttle::new(
        std::time::Duration::from_millis(200),
        1,
    )));
    let bytes_buf = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    moosicbox_downloader::api::add_progress_listener_to_download_queue(Box::new(move |event| {
        let bytes_throttle = bytes_throttle.clone();
        let bytes_buf = bytes_buf.clone();
        let event = event.clone();
        Box::pin(async move {
            let event = if let moosicbox_downloader::queue::ProgressEvent::BytesRead {
                task,
                read,
                total,
            } = event
            {
                if bytes_throttle.lock().unwrap().accept().is_err() {
                    bytes_buf.fetch_add(read, std::sync::atomic::Ordering::SeqCst);
                    return;
                }

                let bytes = bytes_buf.load(std::sync::atomic::Ordering::SeqCst);
                bytes_buf.store(0, std::sync::atomic::Ordering::SeqCst);
                moosicbox_downloader::queue::ProgressEvent::BytesRead {
                    task,
                    read: read + bytes,
                    total,
                }
            } else {
                event.clone()
            };

            let api_event: moosicbox_downloader::api::models::ApiProgressEvent = event.into();

            let handle = { WS_SERVER_HANDLE.read().await.clone().unwrap() };

            if let Err(err) = moosicbox_ws::send_download_event(&handle, None, api_event).await {
                log::error!("Failed to broadcast download event: {err:?}");
            }
        }) as moosicbox_downloader::queue::ProgressListenerRefFut
    }))
    .await;
}
