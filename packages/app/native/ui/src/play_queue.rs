#![allow(clippy::module_name_repetitions)]

use hyperchad::{
    actions::{self as hyperchad_actions, ActionType, logic::get_visibility_self},
    template2::{self as hyperchad_template2, Containers, container},
    transformer::models::{AlignItems, LayoutDirection, Visibility},
};
use moosicbox_music_models::api::ApiTrack;

use crate::{public_img, state::State};

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
        Div
            direction=(LayoutDirection::Row)
            gap=(10)
            opacity=[if is_history { Some(0.5) } else { None }]
        {
            Div {
                @let icon_size = 50;
                Anchor href=(album_page_url) width=(icon_size) height=(icon_size) {
                    (crate::albums::album_cover_img_from_track(&connection.api_url, track, icon_size))
                }
            }
            Div flex=(1) {
                Div {
                    Anchor href=(album_page_url) {
                        (track.title) " - " (track.album)
                    }
                }
                Div {
                    Anchor href=(artist_page_url) { (track.artist) }
                }
            }
            Div align-items=(AlignItems::End) background="#000" {
                @let icon_size = 20;
                Button width=(icon_size) height=(icon_size) {
                    Image
                        width=(icon_size)
                        height=(icon_size)
                        src=(public_img!("cross-white.svg"));
                }
            }
        }
    }
}

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
        Div
            id="play-queue"
            width="calc(min(500, 30%))"
            height="calc(100% - 200)"
            visibility="hidden"
            background="#282a2b"
            border-top-left-radius=(10)
            border-bottom-left-radius=(10)
            position="absolute"
            bottom=(170)
            right=(0)
            fx-click-outside=(
                get_visibility_self()
                    .eq(Visibility::Visible)
                    .then(ActionType::hide_self())
            )
        {
            Div overflow-y="auto" {
                Div padding=(20) {
                    H1 height=(30) { "Play queue" }
                    Div gap=(10) {
                        @for track in history {
                            (render_play_queue_item(state, track, true))
                        }
                    }
                    @if let Some(track) = current {
                        Div direction="row" {
                            "Playing from: "
                            Anchor href=(
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
                        Div { "Next up:" }
                    }
                    @for track in future {
                        (render_play_queue_item(state, track, false))
                    }
                }
            }
        }
    }
}
