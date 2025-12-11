//! Playback session management UI components.
//!
//! This module provides UI templates for displaying and managing playback sessions
//! and their associated playlists.

#![allow(clippy::module_name_repetitions)]

#[allow(unused_imports)]
use hyperchad::template as hyperchad_template;
use hyperchad::template::{Containers, container};
use moosicbox_session_models::ApiSession;

use crate::PLAYBACK_SESSIONS_CONTENT_ID;

/// Renders the playback sessions list content.
///
/// Displays all active playback sessions with their current and upcoming tracks.
#[must_use]
pub fn playback_sessions(host: &str, sessions: &[ApiSession]) -> Containers {
    container! {
        div id=(PLAYBACK_SESSIONS_CONTENT_ID) {
            @for session in sessions {
                @let future_tracks = session.playlist.tracks.iter().skip(session.position.unwrap_or(0) as usize);
                div {
                    h1 { (session.name) }
                    div {
                        @for track in future_tracks {
                            div direction=row {
                                @let icon_size = 50;
                                @let album_page_url = crate::albums::album_page_url(
                                    &track.album_id.to_string(),
                                    false,
                                    Some(&track.api_source),
                                    Some(&track.track_source),
                                    track.sample_rate,
                                    track.bit_depth,
                                );
                                anchor
                                    href=(album_page_url)
                                    align-items=center
                                    width=(icon_size)
                                    height=(icon_size)
                                {
                                    (crate::albums::album_cover_img_from_track(host, track, icon_size))
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
