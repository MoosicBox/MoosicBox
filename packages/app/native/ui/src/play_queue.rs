//! Play queue UI components.
//!
//! This module provides UI templates for displaying the current play queue,
//! including playback history, current track, and upcoming tracks.

#![allow(clippy::module_name_repetitions)]

use hyperchad::template::{self as hyperchad_template, Containers, container};
use moosicbox_music_models::api::ApiTrack;

use crate::{public_img, state::State};

/// Renders a single track item in the play queue.
///
/// Displays track cover art, title, album, and artist with reduced opacity for historical tracks.
fn render_play_queue_item(state: &State, track: &ApiTrack, is_history: bool) -> Containers {
    let Some(connection) = &state.connection else {
        return container! {};
    };

    let album_page_url = crate::albums::album_page_url(
        &track.album_id.to_string(),
        false,
        Some(&track.api_source),
        Some(&track.track_source),
        track.sample_rate,
        track.bit_depth,
    );
    let artist_page_url = crate::artists::artist_page_url(&track.artist_id.to_string());
    container! {
        div
            direction=row
            gap=10
            opacity=[if is_history { Some(0.5) } else { None }]
        {
            div {
                @let icon_size = 50;
                anchor href=(album_page_url) width=(icon_size) height=(icon_size) {
                    (crate::albums::album_cover_img_from_track(&connection.api_url, track, icon_size))
                }
            }
            div flex=1 {
                div {
                    anchor href=(album_page_url) {
                        (track.title) " - " (track.album)
                    }
                }
                div {
                    anchor href=(artist_page_url) { (track.artist) }
                }
            }
            div align-items=end background=#000 {
                @let icon_size = 20;
                button width=(icon_size) height=(icon_size) {
                    image
                        width=(icon_size)
                        height=(icon_size)
                        src=(public_img!("cross-white.svg"));
                }
            }
        }
    }
}

/// Renders the play queue panel.
///
/// Displays the current playback queue including played history, current track,
/// and upcoming tracks with interactive controls.
#[must_use]
pub fn play_queue(state: &State) -> Containers {
    static EMPTY_QUEUE: Vec<ApiTrack> = vec![];

    let position = state.player.playback.as_ref().map_or(0, |x| x.position);

    let queue: &[ApiTrack] = state
        .player
        .playback
        .as_ref()
        .map_or(&EMPTY_QUEUE, |x| &x.tracks);

    let history = queue
        .iter()
        .enumerate()
        .filter(|(i, _)| *i < position as usize)
        .map(|(_, x)| x);

    let current = queue.get(position as usize);

    let mut future = queue
        .iter()
        .enumerate()
        .filter(|(i, _)| *i > position as usize)
        .map(|(_, x)| x)
        .peekable();

    log::debug!("state: {state:?}");

    container! {
        div
            #play-queue
            width=calc(min(500, 30%))
            height=calc(100% - 200)
            visibility=hidden
            background=#282a2b
            border-top-left-radius=10
            border-bottom-left-radius=10
            position=absolute
            bottom=170
            right=0
            fx-click-outside=fx { hide_self() }
        {
            div overflow-y=auto {
                div padding=20 {
                    h1 height=30 { "Play queue" }
                    div gap=10 {
                        @for track in history {
                            (render_play_queue_item(state, track, true))
                        }
                    }
                    @if let Some(track) = current {
                        div direction=row {
                            "Playing from: "
                            anchor href=(
                                crate::albums::album_page_url(
                                    &track.album_id.to_string(),
                                    false,
                                    Some(&track.api_source),
                                    Some(&track.track_source),
                                    track.sample_rate,
                                    track.bit_depth
                                )
                            ) {
                                (track.album)
                            }
                        }
                        (render_play_queue_item(state, track, false))
                    }
                    @if future.peek().is_some() {
                        div { "Next up:" }
                    }
                    @for track in future {
                        (render_play_queue_item(state, track, false))
                    }
                }
            }
        }
    }
}
