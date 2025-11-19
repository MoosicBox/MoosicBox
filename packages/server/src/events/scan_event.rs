//! Library scan progress event handling and WebSocket broadcasting.
//!
//! This module initializes event listeners for music library scan progress and broadcasts scan
//! events to connected WebSocket clients. Events are throttled to avoid excessive updates.

use moosicbox_async_service::Arc;
use moosicbox_scan::event::{
    ApiProgressEvent, ProgressEvent, ProgressListenerRefFut, add_progress_listener,
};

use crate::WS_SERVER_HANDLE;

/// Initializes library scan progress event listeners.
///
/// Sets up throttled event handlers that broadcast scan progress (item count updates, items scanned,
/// scan completion) to connected WebSocket clients. Events are throttled to a maximum of one update
/// per 200ms to prevent excessive message traffic.
#[cfg_attr(feature = "profiling", profiling::function)]
pub async fn init() {
    let scan_throttle = Arc::new(std::sync::Mutex::new(throttle::Throttle::new(
        std::time::Duration::from_millis(200),
        1,
    )));

    add_progress_listener(Box::new(move |event| {
        let scan_throttle = scan_throttle.clone();
        let event = event.clone();

        Box::pin(async move {
            match &event {
                ProgressEvent::ScanCountUpdated { .. } | ProgressEvent::ItemScanned { .. } => {
                    if scan_throttle.lock().unwrap().accept().is_err() {
                        return;
                    }
                    let api_event: Option<ApiProgressEvent> = event.into();
                    if let Some(api_event) = api_event {
                        let handle = { WS_SERVER_HANDLE.read().await.clone().unwrap() };

                        if let Err(err) =
                            moosicbox_ws::send_scan_event(&handle, None, api_event).await
                        {
                            log::error!("Failed to broadcast scan event: {err:?}");
                        }
                    }
                }
                ProgressEvent::ScanFinished { .. } => {
                    let api_event: Option<ApiProgressEvent> = event.into();
                    if let Some(api_event) = api_event {
                        let handle = { WS_SERVER_HANDLE.read().await.clone().unwrap() };

                        if let Err(err) =
                            moosicbox_ws::send_scan_event(&handle, None, api_event).await
                        {
                            log::error!("Failed to broadcast scan event: {err:?}");
                        }
                    }
                }
                ProgressEvent::State { .. } => {}
            }
        }) as ProgressListenerRefFut
    }))
    .await;
}
