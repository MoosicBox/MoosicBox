#![allow(clippy::module_name_repetitions)]

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use moosicbox_core::sqlite::models::{ApiSource, Id};
use tokio::sync::RwLock;

use crate::{RENDERER, STATE};

#[cfg(feature = "_canvas")]
async fn visualization_updated(
    bar_width: f32,
    gap: f32,
    visualization_height: f32,
    visualization: &[u8],
) {
    use moosicbox_app_native_lib::renderer::{
        canvas::{self, Pos},
        Color,
    };

    log::trace!("visualization_updated");

    let mut canvas_actions = Vec::with_capacity(visualization.len());

    canvas_actions.push(canvas::CanvasAction::Clear);
    canvas_actions.push(canvas::CanvasAction::StrokeSize(bar_width));
    canvas_actions.push(canvas::CanvasAction::StrokeColor(Color::WHITE));

    for (i, point) in visualization.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x = (i as f32) * (bar_width + gap);
        let height = f32::from(*point);
        let height = (height / 255.0) * visualization_height;
        let height = if height < 2.0 { 2.0 } else { height };
        let y = (visualization_height - height) / 2.0;

        canvas_actions.push(canvas::CanvasAction::FillRect(
            Pos(x, y),
            Pos(x + bar_width, y + height),
        ));
    }

    let view = canvas::CanvasUpdate {
        target: "visualization".to_string(),
        canvas_actions,
    };
    let response = RENDERER.get().unwrap().write().await.render_canvas(view);
    if let Err(e) = response {
        log::error!("Failed to render_canvas: {e:?}");
    }
}

#[allow(clippy::unused_async)]
pub async fn update_visualization(track_id: &Id, api_source: ApiSource, seek: f64) {
    static BAR_WIDTH: f32 = 2.0;
    static GAP: f32 = 2.0;
    static CACHE: LazyLock<RwLock<HashMap<String, Arc<[u8]>>>> =
        LazyLock::new(|| RwLock::new(HashMap::new()));

    log::debug!("update_visualization: track_id={track_id} api_source={api_source} seek={seek}");

    let binding = RENDERER.get().unwrap().read().await;
    let (width, height) = if let Some(visualization) = binding
        .container()
        .find_container_element_by_str_id("visualization")
    {
        (
            visualization.calculated_width.unwrap(),
            visualization.calculated_height.unwrap(),
        )
    } else {
        return;
    };
    drop(binding);

    let key = format!("{api_source}|{track_id}|{width}");

    let mut binding = CACHE.write().await;
    if let Some(data) = binding.get(&key) {
        #[cfg(feature = "_canvas")]
        visualization_updated(BAR_WIDTH, GAP, height, data).await;
        return;
    }

    let mut headers = serde_json::Map::new();
    let profile = "master";

    headers.insert(
        "moosicbox-profile".to_string(),
        serde_json::Value::String(profile.to_string()),
    );

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let max = (width / (BAR_WIDTH + GAP)).round() as usize;

    let resp = STATE
        .api_proxy_get(
            format!("files/track/visualization?trackId={track_id}&source={api_source}&max={max}"),
            Some(serde_json::Value::Object(headers)),
        )
        .await;
    let Ok(value) = resp else {
        moosicbox_assert::die_or_error!("Failed to get visualization: {:?}", resp.err().unwrap());
        return;
    };

    let buf: Result<Arc<[u8]>, _> = serde_json::from_value(value);
    let Ok(buf) = buf else {
        moosicbox_assert::die_or_error!(
            "Failed to get visualization data from response: {:?}",
            buf.err().unwrap()
        );
        return;
    };

    binding.insert(key, buf.clone());

    drop(binding);

    #[cfg(feature = "_canvas")]
    visualization_updated(BAR_WIDTH, GAP, height, &buf).await;
}

pub async fn check_visualization_update() {
    let session = STATE.get_current_session_ref().await;
    if let Some(session) = session {
        if let Some(position) = session.position {
            if let Some(track) = session.playlist.tracks.get(position as usize) {
                let track_id = track.track_id.clone();
                let api_source = track.api_source;
                let seek = session.seek.unwrap_or_default();
                drop(session);
                update_visualization(&track_id, api_source, seek).await;
            }
        }
    } else {
        drop(session);
    }
}
