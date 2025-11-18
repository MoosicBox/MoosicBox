//! Real-time audio visualization for the native application.
//!
//! This module provides real-time waveform visualization for the currently playing track,
//! rendering animated audio waveforms on an HTML canvas. The visualization updates
//! periodically to show playback progress with a moving cursor.
//!
//! The module manages canvas dimensions, update intervals, and caches visualization data
//! for performance.

#![allow(clippy::module_name_repetitions)]

use std::{
    sync::{LazyLock, RwLock},
    time::{Duration, SystemTime},
};

use hyperchad::renderer::{
    Color,
    canvas::{self, CanvasAction, Pos},
};
use moosicbox_music_models::{ApiSource, id::Id};
use switchy::unsync::{futures::FutureExt, task::JoinHandle, util::CancellationToken};

use crate::STATE;

static INTERVAL_PERIOD: LazyLock<RwLock<Option<Duration>>> =
    LazyLock::new(|| RwLock::new(Some(Duration::from_millis(16))));
static DIMENSIONS: LazyLock<RwLock<(f32, f32)>> = LazyLock::new(|| RwLock::new((0.0, 0.0)));

/// Returns the current visualization canvas dimensions.
///
/// # Panics
///
/// * If the `DIMENSIONS` lock is poisoned
#[must_use]
pub fn get_dimensions() -> (f32, f32) {
    *DIMENSIONS.read().unwrap()
}

/// Sets the visualization canvas dimensions.
///
/// # Panics
///
/// * If the `DIMENSIONS` lock is poisoned
pub fn set_dimensions(width: f32, height: f32) {
    *DIMENSIONS.write().unwrap() = (width, height);
}

/// Sets the visualization update interval period.
///
/// # Panics
///
/// * If the `INTERVAL_PERIOD` lock is poisoned
pub fn set_interval_period(period: Duration) {
    *INTERVAL_PERIOD.write().unwrap() = Some(period);
}

/// Disables the visualization update interval.
///
/// # Panics
///
/// * If the `INTERVAL_PERIOD` lock is poisoned
pub fn disable_interval() {
    *INTERVAL_PERIOD.write().unwrap() = None;
}

/// Represents the currently playing track for visualization purposes.
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

static PREV_CURSOR_X: LazyLock<RwLock<Option<f32>>> = LazyLock::new(|| RwLock::new(None));
static CURRENT_TRACK: LazyLock<RwLock<Option<CurrentTrack>>> = LazyLock::new(|| RwLock::new(None));
static INTERVAL: LazyLock<RwLock<Option<JoinHandle<()>>>> = LazyLock::new(|| RwLock::new(None));
static CANCEL_INTERVAL: LazyLock<RwLock<CancellationToken>> =
    LazyLock::new(|| RwLock::new(CancellationToken::new()));
static CURRENT_STROKE_COLOR: LazyLock<RwLock<Color>> = LazyLock::new(|| RwLock::new(Color::BLACK));

fn stroke_color(color: Color, canvas_actions: &mut Vec<CanvasAction>) {
    let current_color = *CURRENT_STROKE_COLOR.read().unwrap();
    if current_color != color {
        let len = canvas_actions.len();
        // compress stroke color actions
        for i in 1..=len {
            if canvas_actions[len - i].is_draw_action() {
                break;
            }
            if matches!(canvas_actions[len - i], CanvasAction::StrokeColor { .. }) {
                canvas_actions.remove(len - i);
            }
        }
        *CURRENT_STROKE_COLOR.write().unwrap() = color;
        canvas_actions.push(canvas::CanvasAction::StrokeColor(color));
    }
}

#[allow(clippy::too_many_arguments)]
async fn visualization_updated(
    cursor_width: f32,
    cursor_height: f32,
    bar_width: f32,
    bar_height: f32,
    gap: f32,
    visualization_width: f32,
    visualization_height: f32,
    duration: f32,
    prev_cursor_x: Option<f32>,
    last_update: SystemTime,
    progress_percent: f32,
    visualization: &[u8],
) {
    use crate::RENDERER;

    static BUFFER_WIDTH: f32 = 2.0f32;

    log::trace!("visualization_updated");

    let mut canvas_actions = if prev_cursor_x.is_some() {
        Vec::with_capacity(1)
    } else {
        Vec::with_capacity(visualization.len())
    };

    let cursor_half_width = cursor_width / 2.0;
    let step_1_second = visualization_width / duration;
    let delta = switchy::time::now()
        .duration_since(last_update)
        .unwrap()
        .as_secs_f32()
        * step_1_second;
    let cursor_x = visualization_width.mul_add(progress_percent, -cursor_half_width) + delta;
    let bar_y_offset = (visualization_height - bar_height) / 2.0;

    let clear_buffer = cursor_half_width + BUFFER_WIDTH;
    let left_cursor = prev_cursor_x.map(|x| if x < cursor_x { x } else { cursor_x });
    let right_cursor = prev_cursor_x.map_or(cursor_x, |x| if x > cursor_x { x } else { cursor_x });
    let clear_buffer_left = left_cursor.map(|x| x - clear_buffer);
    let clear_buffer_right = right_cursor + clear_buffer;

    if let Some(left) = clear_buffer_left {
        canvas_actions.push(canvas::CanvasAction::ClearRect(
            Pos(left, 0.0),
            Pos(clear_buffer_right, visualization_height),
        ));
    } else {
        canvas_actions.push(canvas::CanvasAction::Clear);
    }

    stroke_color(Color::from_hex("222"), &mut canvas_actions);

    let mut past = true;

    for (i, point) in visualization.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let x = (i as f32) * (bar_width + gap);

        if let Some(left) = clear_buffer_left {
            if x + bar_width + gap < left {
                continue;
            }
            if x > clear_buffer_right {
                break;
            }
        }

        let height = f32::from(*point);
        let height = (height / 255.0) * bar_height;
        let height = if height < 2.0 { 2.0 } else { height };
        let y = (bar_height - height) / 2.0 + bar_y_offset;

        if past && x >= cursor_x {
            past = false;

            stroke_color(Color::WHITE, &mut canvas_actions);
        }

        canvas_actions.push(canvas::CanvasAction::FillRect(
            Pos(x, y),
            Pos(x + bar_width, y + height),
        ));
    }

    // draw cursor
    {
        let cursor_y_offset = (visualization_height - cursor_height) / 2.0;
        let x = cursor_x;
        let y = cursor_y_offset;
        let height = cursor_height;
        stroke_color(Color::WHITE, &mut canvas_actions);
        canvas_actions.push(canvas::CanvasAction::FillRect(
            Pos(x, y),
            Pos(x + cursor_width, y + height),
        ));
    }

    PREV_CURSOR_X.write().unwrap().replace(cursor_x);

    let view = canvas::CanvasUpdate {
        target: "visualization".to_string(),
        canvas_actions,
    };
    let response = RENDERER.get().unwrap().render_canvas(view).await;
    if let Err(e) = response {
        log::error!("Failed to render_canvas: {e:?}");
    }
}

async fn clear_canvas() {
    use hyperchad::renderer::canvas;

    use crate::RENDERER;

    let view = canvas::CanvasUpdate {
        target: "visualization".to_string(),
        canvas_actions: vec![canvas::CanvasAction::Clear],
    };
    let response = RENDERER.get().unwrap().render_canvas(view).await;
    if let Err(e) = response {
        log::error!("Failed to render_canvas: {e:?}");
    }
}

async fn update_visualization(
    visualization_width: f32,
    visualization_height: f32,
    track: CurrentTrack,
) {
    use std::{collections::BTreeMap, sync::Arc};

    use switchy::unsync::sync::RwLock;

    static CURSOR_WIDTH: f32 = 2.0;
    static BAR_WIDTH: f32 = 1.0;
    static GAP: f32 = 2.0;

    static CACHE: LazyLock<RwLock<BTreeMap<String, Arc<[u8]>>>> =
        LazyLock::new(|| RwLock::new(BTreeMap::new()));

    let track_id = track.id;
    let api_source = track.api_source;
    let seek = track.seek;
    let duration = track.duration;
    let last_update = track.time;
    let prev_cursor_x = *PREV_CURSOR_X.read().unwrap();
    let bar_height = visualization_height - 5.0;
    let cursor_height = visualization_height;

    #[allow(clippy::cast_possible_truncation)]
    let progress_percent = (seek / duration) as f32;
    #[allow(clippy::cast_possible_truncation)]
    let duration = duration as f32;

    log::trace!(
        "update_visualization: track_id={track_id} api_source={api_source} seek={seek} visualization_width={visualization_width} visualization_height={visualization_height}"
    );

    let key = format!("{api_source}|{track_id}|{visualization_width}|{visualization_height}");

    let mut binding = CACHE.write().await;
    if let Some(data) = binding.get(&key) {
        visualization_updated(
            CURSOR_WIDTH,
            cursor_height,
            BAR_WIDTH,
            bar_height,
            GAP,
            visualization_width,
            visualization_height,
            duration,
            prev_cursor_x,
            last_update,
            progress_percent,
            data,
        )
        .await;
        return;
    }

    clear_canvas().await;

    *PREV_CURSOR_X.write().unwrap() = None;
    let prev_cursor_x = None;

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let max = (visualization_width / (BAR_WIDTH + GAP)).round() as usize;

    let resp = STATE
        .api_proxy_get(
            format!("files/track/visualization?trackId={track_id}&source={api_source}&max={max}"),
            None,
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

    visualization_updated(
        CURSOR_WIDTH,
        cursor_height,
        BAR_WIDTH,
        bar_height,
        GAP,
        visualization_width,
        visualization_height,
        duration,
        prev_cursor_x,
        last_update,
        progress_percent,
        &buf,
    )
    .await;
}

async fn tick_visualization() {
    let Some(current_track) = CURRENT_TRACK.read().unwrap().clone() else {
        moosicbox_assert::die_or_panic!("Current track not set");
    };
    let (visualization_width, visualization_height) = get_dimensions();

    update_visualization(visualization_width, visualization_height, current_track).await;
}

/// Checks and updates the visualization based on the current playback state.
///
/// # Panics
///
/// * If the `CURRENT_TRACK` lock is poisoned
pub async fn check_visualization_update() {
    let session = STATE.get_current_session_ref().await;
    if let Some(session) = session {
        if let Some(position) = session.position
            && let Some(track) = session.playlist.tracks.get(position as usize)
        {
            let track_id = track.track_id.clone();
            let duration = track.duration;
            let api_source = track.api_source.clone();
            let seek = session.seek.unwrap_or_default();
            let playing = session.playing;
            drop(session);
            CURRENT_TRACK.write().unwrap().replace(CurrentTrack {
                id: track_id,
                api_source,
                seek,
                duration,
                time: switchy::time::now(),
            });

            if let Some(interval_period) = { *INTERVAL_PERIOD.read().unwrap() } {
                let mut interval = INTERVAL.write().unwrap();
                let mut cancel_interval = CANCEL_INTERVAL.write().unwrap();
                if playing && !interval.is_some() {
                    cancel_interval.cancel();
                    let token = CancellationToken::new();
                    *cancel_interval = token.clone();
                    drop(cancel_interval);
                    interval.replace(switchy::unsync::task::spawn(async move {
                        let mut interval = switchy::unsync::time::interval(interval_period);

                        while !token.is_cancelled() {
                            switchy::unsync::select! {
                                _ = interval.tick() => {}
                                () = token.cancelled().fuse() => {
                                    break;
                                }
                            };
                            tick_visualization().await;
                        }
                    }));
                } else {
                    if !playing && interval.is_some() {
                        interval.take();
                        cancel_interval.cancel();
                    }
                    drop(cancel_interval);
                }

                drop(interval);
            }

            tick_visualization().await;
        }
    } else {
        drop(session);
    }
}
