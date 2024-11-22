#![allow(clippy::module_name_repetitions)]

use std::{
    sync::{LazyLock, RwLock},
    time::SystemTime,
};

use moosicbox_core::sqlite::models::{ApiSource, Id};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::STATE;

static DIMENSIONS: LazyLock<RwLock<(f32, f32)>> = LazyLock::new(|| RwLock::new((0.0, 0.0)));

pub fn get_dimensions() -> (f32, f32) {
    *DIMENSIONS.read().unwrap()
}

#[allow(unused)]
pub fn set_dimensions(width: f32, height: f32) {
    *DIMENSIONS.write().unwrap() = (width, height);
}

#[derive(Debug, Clone)]
pub struct CurrentTrack {
    #[allow(unused)]
    id: Id,
    #[allow(unused)]
    api_source: ApiSource,
    #[allow(unused)]
    seek: f64,
    #[allow(unused)]
    duration: f64,
    #[allow(unused)]
    time: SystemTime,
}

static CURRENT_TRACK: LazyLock<RwLock<Option<CurrentTrack>>> = LazyLock::new(|| RwLock::new(None));
static INTERVAL: LazyLock<RwLock<Option<JoinHandle<()>>>> = LazyLock::new(|| RwLock::new(None));
static CANCEL_INTERVAL: LazyLock<RwLock<CancellationToken>> =
    LazyLock::new(|| RwLock::new(CancellationToken::new()));

#[cfg(feature = "_calculated_canvas")]
#[allow(clippy::too_many_arguments)]
async fn visualization_updated(
    cursor_width: f32,
    bar_width: f32,
    gap: f32,
    visualization_width: f32,
    visualization_height: f32,
    duration: f32,
    last_update: SystemTime,
    progress_percent: f32,
    visualization: &[u8],
) {
    use moosicbox_app_native_lib::renderer::{
        canvas::{self, Pos},
        Color,
    };

    use crate::RENDERER;

    log::trace!("visualization_updated");

    let mut canvas_actions = Vec::with_capacity(visualization.len());

    canvas_actions.push(canvas::CanvasAction::Clear);

    let step_1_second = visualization_width / duration;
    let delta = SystemTime::now()
        .duration_since(last_update)
        .unwrap()
        .as_secs_f32()
        * step_1_second;
    let cursor_x = visualization_width.mul_add(progress_percent, -(cursor_width / 2.0)) + delta;

    canvas_actions.push(canvas::CanvasAction::StrokeColor(Color::from_hex("222")));

    let mut past = true;

    for (i, point) in visualization.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x = (i as f32) * (bar_width + gap);
        let height = f32::from(*point);
        let height = (height / 255.0) * visualization_height;
        let height = if height < 2.0 { 2.0 } else { height };
        let y = (visualization_height - height) / 2.0;

        if past && x >= cursor_x {
            past = false;

            canvas_actions.push(canvas::CanvasAction::StrokeColor(Color::WHITE));
        }

        canvas_actions.push(canvas::CanvasAction::FillRect(
            Pos(x, y),
            Pos(x + bar_width, y + height),
        ));
    }

    // draw cursor
    {
        let x = cursor_x;
        let y = 0.0;
        let height = visualization_height;
        canvas_actions.push(canvas::CanvasAction::StrokeColor(Color::WHITE));
        canvas_actions.push(canvas::CanvasAction::FillRect(
            Pos(x, y),
            Pos(x + cursor_width, y + height),
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

#[cfg(not(feature = "_calculated_canvas"))]
#[allow(clippy::unused_async)]
pub async fn update_visualization(
    _visualization_width: f32,
    _visualization_height: f32,
    _track: CurrentTrack,
) {
}

#[cfg(feature = "_calculated_canvas")]
#[allow(clippy::unused_async)]
pub async fn update_visualization(
    visualization_width: f32,
    visualization_height: f32,
    track: CurrentTrack,
) {
    use std::{collections::HashMap, sync::Arc};

    use tokio::sync::RwLock;

    static CURSOR_WIDTH: f32 = 3.0;
    static BAR_WIDTH: f32 = 1.0;
    static GAP: f32 = 2.0;

    static CACHE: LazyLock<RwLock<HashMap<String, Arc<[u8]>>>> =
        LazyLock::new(|| RwLock::new(HashMap::new()));

    let track_id = track.id;
    let api_source = track.api_source;
    let seek = track.seek;
    let duration = track.duration;
    let time = track.time;

    #[allow(clippy::cast_possible_truncation)]
    let progress_percent = (seek / duration) as f32;
    #[allow(clippy::cast_possible_truncation)]
    let duration = duration as f32;

    log::trace!("update_visualization: track_id={track_id} api_source={api_source} seek={seek}");

    let key = format!("{api_source}|{track_id}|{visualization_width}|{visualization_height}");

    let mut binding = CACHE.write().await;
    if let Some(data) = binding.get(&key) {
        #[cfg(feature = "_canvas")]
        visualization_updated(
            CURSOR_WIDTH,
            BAR_WIDTH,
            GAP,
            visualization_width,
            visualization_height,
            duration,
            time,
            progress_percent,
            data,
        )
        .await;
        return;
    }

    let mut headers = serde_json::Map::new();
    let profile = "master";

    headers.insert(
        "moosicbox-profile".to_string(),
        serde_json::Value::String(profile.to_string()),
    );

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let max = (visualization_width / (BAR_WIDTH + GAP)).round() as usize;

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
    visualization_updated(
        CURSOR_WIDTH,
        BAR_WIDTH,
        GAP,
        visualization_width,
        visualization_height,
        duration,
        time,
        progress_percent,
        &buf,
    )
    .await;
}

pub async fn tick_visualization() {
    let Some(current_track) = CURRENT_TRACK.read().unwrap().clone() else {
        moosicbox_assert::die_or_panic!("Current track not set");
    };
    let (visualization_width, visualization_height) = get_dimensions();

    update_visualization(visualization_width, visualization_height, current_track).await;
}

pub async fn check_visualization_update() {
    let session = STATE.get_current_session_ref().await;
    if let Some(session) = session {
        if let Some(position) = session.position {
            if let Some(track) = session.playlist.tracks.get(position as usize) {
                let track_id = track.track_id.clone();
                let duration = track.duration;
                let api_source = track.api_source;
                let seek = session.seek.unwrap_or_default();
                let playing = session.playing;
                drop(session);
                CURRENT_TRACK.write().unwrap().replace(CurrentTrack {
                    id: track_id,
                    api_source,
                    seek,
                    duration,
                    time: SystemTime::now(),
                });
                let has_interval = { INTERVAL.read().unwrap().is_some() };
                if playing && !has_interval {
                    let token = CancellationToken::new();
                    *CANCEL_INTERVAL.write().unwrap() = token.clone();
                    INTERVAL
                        .write()
                        .unwrap()
                        .replace(tokio::task::spawn(async move {
                            let mut interval =
                                tokio::time::interval(std::time::Duration::from_millis(16));

                            while !token.is_cancelled() {
                                tokio::select! {
                                    _ = interval.tick() => {}
                                    () = token.cancelled() => {
                                        break;
                                    }
                                };
                                tick_visualization().await;
                            }
                        }));
                } else if !playing && has_interval {
                    INTERVAL.write().unwrap().take();
                    CANCEL_INTERVAL.read().unwrap().cancel();
                }
                tick_visualization().await;
            }
        }
    } else {
        drop(session);
    }
}
