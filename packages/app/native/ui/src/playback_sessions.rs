#![allow(clippy::module_name_repetitions)]

use hyperchad::transformer_models::AlignItems;
use maud::{Markup, html};
use moosicbox_session_models::ApiSession;

use crate::PLAYBACK_SESSIONS_CONTENT_ID;

#[must_use]
pub fn playback_sessions(sessions: &[ApiSession]) -> Markup {
    html! {
        div id=(PLAYBACK_SESSIONS_CONTENT_ID) {
            @for session in sessions {
                @let future_tracks = session.playlist.tracks.iter().skip(session.position.unwrap_or(0) as usize);
                div {
                    h1 { (session.name) }
                    div {
                        @for track in future_tracks {
                            div sx-dir="row" {
                                @let icon_size = 50;
                                @let album_page_url = crate::albums::album_page_url(
                                    &track.album_id.to_string(),
                                    false,
                                    Some(track.api_source),
                                    Some(track.track_source),
                                    track.sample_rate,
                                    track.bit_depth,
                                );
                                a
                                    href=(album_page_url)
                                    sx-align-items=(AlignItems::Center)
                                    sx-width=(icon_size)
                                    sx-height=(icon_size)
                                {
                                    (crate::albums::album_cover_img_from_track(track, icon_size))
                                }
                                (track.title)
                            }
                        }
                    }
                }
            }
        }
    }
}
