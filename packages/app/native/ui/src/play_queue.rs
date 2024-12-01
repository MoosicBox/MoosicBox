#![allow(clippy::module_name_repetitions)]

use maud::{html, Markup};
use moosicbox_core::sqlite::models::ApiTrack;

use crate::{public_img, state::State};

#[must_use]
pub fn play_queue(state: &State) -> Markup {
    static EMPTY_QUEUE: Vec<ApiTrack> = vec![];

    let queue: &[ApiTrack] = state
        .player
        .playback
        .as_ref()
        .map_or(&EMPTY_QUEUE, |x| &x.tracks);

    html! {
        div sx-width="calc(min(500, 30%))" sx-height="calc(100% - 200)" {
            @for track in queue {
                div sx-dir="row" {
                    div {
                        (crate::albums::album_cover_img_from_track(track, 50))
                    }
                    div {
                        div {
                            (format!("{} - {}", track.title, track.album))
                        }
                        div {
                            (track.artist)
                        }
                    }
                    div {
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
    }
}
