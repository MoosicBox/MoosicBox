#![allow(clippy::module_name_repetitions)]

use maud::{html, Markup};
use moosicbox_session_models::ApiSession;

#[must_use]
pub fn playback_sessions(sessions: &[ApiSession]) -> Markup {
    html! {
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
                            a href=(album_page_url) sx-width=(icon_size) sx-height=(icon_size) {
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
