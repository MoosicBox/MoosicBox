#![allow(clippy::module_name_repetitions)]

use hyperchad::{
    actions::{ActionType, logic::get_visibility_self},
    transformer_models::{AlignItems, LayoutDirection, Visibility},
};
use maud::{Markup, html};
use moosicbox_music_models::api::ApiTrack;

use crate::{public_img, state::State};

fn render_play_queue_item(state: &State, track: &ApiTrack, is_history: bool) -> Markup {
    let Some(connection) = &state.connection else {
        return html! {};
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
    html! {
        div
            sx-dir=(LayoutDirection::Row)
            sx-gap=(10)
            sx-opacity=[if is_history { Some(0.5) } else { None }]
        {
            div {
                @let icon_size = 50;
                a href=(album_page_url) sx-width=(icon_size) sx-height=(icon_size) {
                    (crate::albums::album_cover_img_from_track(&connection.api_url, track, icon_size))
                }
            }
            div flex=(1) {
                div {
                    a href=(album_page_url) {
                        (track.title) " - " (track.album)
                    }
                }
                div {
                    a href=(artist_page_url) { (track.artist) }
                }
            }
            div sx-align-items=(AlignItems::End) sx-background="#000" {
                @let icon_size = 20;
                button sx-width=(icon_size) sx-height=(icon_size) {
                    img
                        sx-width=(icon_size)
                        sx-height=(icon_size)
                        src=(public_img!("cross-white.svg"));
                }
            }
        }
    }
}

#[must_use]
pub fn play_queue(state: &State) -> Markup {
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

    html! {
        div
            id="play-queue"
            sx-width="calc(min(500, 30%))"
            sx-height="calc(100% - 200)"
            sx-visibility="hidden"
            sx-background="#282a2b"
            sx-border-top-left-radius=(10)
            sx-border-bottom-left-radius=(10)
            sx-position="absolute"
            sx-bottom=(170)
            sx-right=(0)
            fx-click-outside=(
                get_visibility_self()
                    .eq(Visibility::Visible)
                    .then(ActionType::hide_self())
            )
        {
            div sx-overflow-y="auto" {
                div sx-padding=(20) {
                    h1 sx-height=(30) { "Play queue" }
                    div sx-gap=(10) {
                        @for track in history {
                            (render_play_queue_item(state, track, true))
                        }
                    }
                    @if let Some(track) = current {
                        div sx-dir="row" {
                            "Playing from: "
                            a href=(
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
