//! `MoosicBox` native UI component library.
//!
//! This crate provides UI components and templates for rendering the `MoosicBox`
//! music player interface using the hyperchad templating system.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::branches_sharing_code)]

pub mod albums;
pub mod artists;
pub mod audio_zones;
pub mod downloads;
pub mod formatting;
pub mod play_queue;
pub mod playback_sessions;
pub mod search;
pub mod settings;
pub mod state;

use albums::album_cover_img_from_album;
use formatting::TimeFormat;
use hyperchad::{
    actions::ActionType,
    template::{self as hyperchad_template, Containers, IntoActionEffect, container},
};
use moosicbox_music_models::{
    API_SOURCES, AlbumSort, ApiSource, TrackApiSource, api::ApiTrack, id::Id,
};
use moosicbox_session_models::{ApiSession, ApiUpdateSession};
use play_queue::play_queue;
use search::search;
use serde::{Deserialize, Serialize};
use state::State;

/// Height of the visualization canvas in pixels.
pub const VIZ_HEIGHT: u16 = 35;
/// Padding around the visualization canvas in pixels.
pub const VIZ_PADDING: u16 = 5;
/// Border size of the footer in pixels.
pub const FOOTER_BORDER_SIZE: u16 = 3;
/// Total height of the footer including visualization and padding.
pub const FOOTER_HEIGHT: u16 = 100 + VIZ_HEIGHT + VIZ_PADDING * 2 + FOOTER_BORDER_SIZE;
/// Size of icons in the footer in pixels.
pub const FOOTER_ICON_SIZE: u16 = 25;
/// Size of the current album artwork in the player in pixels.
pub const CURRENT_ALBUM_SIZE: u16 = 70;

/// Dark background color used in the UI.
pub const DARK_BACKGROUND: &str = "#080a0b";
/// Standard background color used in the UI.
pub const BACKGROUND: &str = "#181a1b";

/// Constructs a path to a public image asset.
///
/// # Examples
///
/// ```
/// # use moosicbox_app_native_ui::public_img;
/// let icon_path = public_img!("icon128.png");
/// assert_eq!(icon_path, "/public/img/icon128.png");
/// ```
#[macro_export]
macro_rules! public_img {
    ($path:expr $(,)?) => {
        concat!("/public/img/", $path)
    };
}

/// Custom UI actions that can be triggered in the application.
///
/// Actions are serialized to JSON and sent to the frontend for execution.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    /// Requests the visualization canvas to refresh with current dimensions.
    RefreshVisualization,
    /// Toggles between play and pause states.
    TogglePlayback,
    /// Skips to the previous track in the playlist.
    PreviousTrack,
    /// Skips to the next track in the playlist.
    NextTrack,
    /// Sets the playback volume.
    SetVolume,
    /// Seeks to a specific position in the current track as a percentage.
    SeekCurrentTrackPercent,
    /// Filters the album list by source and sort order.
    FilterAlbums {
        /// The sources to include in the filtered results.
        filtered_sources: Vec<TrackApiSource>,
        /// The sort order to apply.
        sort: AlbumSort,
    },
    /// Plays an album from the beginning.
    PlayAlbum {
        /// The album identifier.
        album_id: Id,
        /// The API source for the album.
        api_source: ApiSource,
        /// The specific version source to use.
        version_source: Option<TrackApiSource>,
        /// The desired sample rate in Hz.
        sample_rate: Option<u32>,
        /// The desired bit depth.
        bit_depth: Option<u8>,
    },
    /// Adds an album to the end of the current queue.
    AddAlbumToQueue {
        /// The album identifier.
        album_id: Id,
        /// The API source for the album.
        api_source: ApiSource,
        /// The specific version source to use.
        version_source: Option<TrackApiSource>,
        /// The desired sample rate in Hz.
        sample_rate: Option<u32>,
        /// The desired bit depth.
        bit_depth: Option<u8>,
    },
    /// Plays an album starting from a specific track.
    PlayAlbumStartingAtTrackId {
        /// The album identifier.
        album_id: Id,
        /// The track to start playback from.
        start_track_id: Id,
        /// The API source for the album.
        api_source: ApiSource,
        /// The specific version source to use.
        version_source: Option<TrackApiSource>,
        /// The desired sample rate in Hz.
        sample_rate: Option<u32>,
        /// The desired bit depth.
        bit_depth: Option<u8>,
    },
    /// Plays a list of specific tracks.
    PlayTracks {
        /// The track identifiers to play.
        track_ids: Vec<Id>,
        /// The API source for the tracks.
        api_source: ApiSource,
    },
}

impl IntoActionEffect for Action {
    fn into_action_effect(self) -> hyperchad::actions::ActionEffect {
        ActionType::Custom {
            action: self.to_string(),
        }
        .into()
    }
}

impl From<Action> for hyperchad::actions::Action {
    fn from(value: Action) -> Self {
        ActionType::Custom {
            action: value.to_string(),
        }
        .into()
    }
}

impl From<Action> for ActionType {
    fn from(value: Action) -> Self {
        Self::Custom {
            action: value.to_string(),
        }
    }
}

impl std::fmt::Display for Action {
    /// # Panics
    ///
    /// * Panics if the action cannot be serialized to JSON
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

impl TryFrom<String> for Action {
    type Error = serde_json::Error;

    /// # Errors
    ///
    /// * Returns an error if the string is not valid JSON or does not match the `Action` schema
    fn try_from(value: String) -> Result<Self, Self::Error> {
        serde_json::from_str(&value)
    }
}

impl TryFrom<&String> for Action {
    type Error = serde_json::Error;

    /// # Errors
    ///
    /// * Returns an error if the string is not valid JSON or does not match the `Action` schema
    fn try_from(value: &String) -> Result<Self, Self::Error> {
        serde_json::from_str(value)
    }
}

impl<'a> TryFrom<&'a str> for Action {
    type Error = serde_json::Error;

    /// # Errors
    ///
    /// * Returns an error if the string is not valid JSON or does not match the `Action` schema
    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        serde_json::from_str(value)
    }
}

/// Renders the sidebar navigation menu.
///
/// Includes the application logo, settings link, and navigation links to main sections.
#[must_use]
pub fn sidebar_navigation() -> Containers {
    container! {
        aside width=calc(max(240, min(280, 15%))) background=(DARK_BACKGROUND) {
            div .navigation-bar padding=20 {
                @let size = 36;
                div .navigation-bar-header direction=row align-items=center height=(size) {
                    @let icon_size = 36;
                    anchor href="/" direction=row height=(icon_size) {
                        image
                            width=(icon_size)
                            height=(icon_size)
                            src=(public_img!("icon128.png"));

                        h1 .navigation-bar-header-home-link-text font-size=20 {
                            "MoosicBox"
                        }
                    }
                    @let size = 22;
                    div direction=row justify-content=end align-items=center height=(size) {
                        anchor href="/settings" direction=row width=(size + 10) {
                            image
                                width=(size)
                                height=(size)
                                src=(public_img!("settings-gear-white.svg"));
                        }
                        div width=(size + 10) {
                            image
                                width=(size)
                                height=(size)
                                src=(public_img!("chevron-left-white.svg"));
                        }
                    }
                }
                ul {
                    li {
                        anchor href="/" {
                            "Home"
                        }
                    }
                    li {
                        anchor href="/downloads" {
                            "Downloads"
                        }
                    }
                }
                h1 .my-collection-header font-size=20 {
                    "My Collection"
                }
                ul {
                    li {
                        anchor href="/albums" {
                            "Albums"
                        }
                    }
                    li {
                        anchor href="/artists" {
                            "Artists"
                        }
                    }
                }
            }
        }
    }
}

/// Renders the player control panel.
///
/// Includes visualization canvas, playback controls, current track information, and volume controls.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn player(state: &State) -> Containers {
    container! {
        div height=(FOOTER_HEIGHT) border-top=((FOOTER_BORDER_SIZE, "#222")) {
            div height=(VIZ_HEIGHT) padding-y=(VIZ_PADDING) direction=row {
                canvas
                    #visualization
                    cursor=pointer
                    width=100%
                    height=(VIZ_HEIGHT)
                    fx-click=fx { invoke(Action::SeekCurrentTrackPercent, get_mouse_x_self() / get_width_px_self()) }
                    fx-resize=fx { invoke(Action::RefreshVisualization, get_width_px_self()) }
                    fx-immediate=fx { invoke(Action::RefreshVisualization, get_width_px_self()) }
                {}
            }
            div height=100 direction=row {
                div flex=1 justify-content=center {
                    (player_current_album_from_state(state, 70))
                }
                div flex=2 align-items=center justify-content=center {
                    @let button_size = 40;
                    @let progress_size = 20;
                    div height=(button_size) direction=row justify-content=center {
                        button
                            width=(button_size)
                            height=(button_size)
                            margin-x=5
                            direction=row
                            justify-content=center
                            align-items=center
                            background=(BACKGROUND)
                            border-radius=100%
                            fx-click=(Action::PreviousTrack)
                        {
                            @let icon_size = 18;
                            image
                                width=(icon_size)
                                height=(icon_size)
                                src=(public_img!("previous-button-white.svg"));
                        }
                        (player_play_button_from_state(state))
                        button
                            width=(button_size)
                            height=(button_size)
                            margin-x=5
                            direction=row
                            justify-content=center
                            align-items=center
                            background=(BACKGROUND)
                            border-radius=100%
                            fx-click=(Action::NextTrack)
                        {
                            @let icon_size = 18;
                            image
                                width=(icon_size)
                                height=(icon_size)
                                src=(public_img!("next-button-white.svg"));
                        }
                    }
                    div height=(progress_size) margin-top=10 {
                        (player_current_progress_from_state(state))
                    }
                }
                div flex=1 direction=row justify-content=end align-items=center padding-right=20 {
                    (volume(state, FOOTER_ICON_SIZE))
                    button
                        width=(FOOTER_ICON_SIZE)
                        height=(FOOTER_ICON_SIZE)
                        margin-left=10
                        fx-click=fx { element(AUDIO_ZONES_ID).toggle_visibility() }
                    {
                        image
                            width=(FOOTER_ICON_SIZE)
                            height=(FOOTER_ICON_SIZE)
                            src=(public_img!("speaker-white.svg"));
                    }
                    button
                        width=(FOOTER_ICON_SIZE)
                        height=(FOOTER_ICON_SIZE)
                        margin-left=10
                        fx-click=fx { element(PLAYBACK_SESSIONS_ID).toggle_visibility() }
                    {
                        image
                            width=(FOOTER_ICON_SIZE)
                            height=(FOOTER_ICON_SIZE)
                            src=(public_img!("sessions-white.svg"));
                    }
                    button
                        fx-click=fx { element(PLAY_QUEUE_ID).toggle_visibility() }
                        width=(FOOTER_ICON_SIZE)
                        height=(FOOTER_ICON_SIZE)
                        margin-left=10
                    {
                        image
                            width=(FOOTER_ICON_SIZE)
                            height=(FOOTER_ICON_SIZE)
                            src=(public_img!("playlist-white.svg"));
                    }
                }
            }
        }
    }
}

/// DOM element ID for the volume slider container.
pub const VOLUME_SLIDER_CONTAINER_ID: &str = "volume-slider-container";
/// DOM element ID for the volume slider.
pub const VOLUME_SLIDER_ID: &str = "volume-slider";
/// DOM element ID for the volume slider value container.
pub const VOLUME_SLIDER_VALUE_CONTAINER_ID: &str = "volume-slider-value-container";
/// DOM element ID for the volume slider value display.
pub const VOLUME_SLIDER_VALUE_ID: &str = "volume-slider-value";

fn volume(state: &State, size: u16) -> Containers {
    let volume_percent = state.player.playback.as_ref().map_or(1.0, |x| x.volume);
    container! {
        div
            id=(VOLUME_SLIDER_CONTAINER_ID)
            width=(size)
            height=(size)
            position=relative
            fx-hover=fx { element(VOLUME_SLIDER_ID).show().delay_off(400) }
        {
            button {
                image
                    width=(size)
                    height=(size)
                    src=(public_img!("audio-white.svg"));
            }
            (volume_slider(size, volume_percent))
        }
    }
}

fn volume_slider(size: u16, volume_percent: f64) -> Containers {
    container! {
        div
            id=(VOLUME_SLIDER_ID)
            visibility=hidden
            width=30
            height=130
            padding-y=15
            position=absolute
            bottom=(size)
            left=0
            margin-y=5
            align-items=center
            justify-content=center
            border-radius=30
            background=(BACKGROUND)
            cursor=pointer
            fx-mouse-down=fx {
                let element = element(VOLUME_SLIDER_VALUE_CONTAINER_ID);
                invoke(
                    Action::SetVolume,
                    ((element.get_height_px() - element.get_mouse_y()) / element.get_height_px())
                        .clamp(0.0, 1.0)
                ).throttle(30)
            }
            fx-hover=fx { show_self().delay_off(400) }
        {
            div
                id=(VOLUME_SLIDER_VALUE_CONTAINER_ID)
                position=relative
                width=3
                height=100%
                border-radius=30
                background=#444
            {
                (volume_slider_value(size, volume_percent))
            }
        }
    }
}

fn volume_slider_value(size: u16, volume_percent: f64) -> Containers {
    container! {
        div
            id=(VOLUME_SLIDER_VALUE_ID)
            position=absolute
            bottom=0
            left=0
            width=100%
            height=(volume_percent * 100.0)%
            border-radius=30
            background=#fff
        {
            div position=relative {
                @let slider_top_width = f32::from(size) / 2.5;
                div
                    position=absolute
                    top=0
                    left=calc(50% - slider_top_width / 2.0)
                    width=(slider_top_width)
                    height=3
                    border-radius=30
                    background=#fff
                {}
            }
        }
    }
}

fn player_play_button(playing: bool) -> Containers {
    container! {
        @let button_size = 40;
        button
            #player-play-button
            width=(button_size)
            height=(button_size)
            margin-x=5
            direction=row
            justify-content=center
            align-items=center
            background=(BACKGROUND)
            border-radius=100%
            fx-click=(Action::TogglePlayback)
        {
            @let icon_size = 16;
            image
                width=(icon_size)
                height=(icon_size)
                src=(
                    if playing {
                        public_img!("pause-button-white.svg")
                    } else {
                        public_img!("play-button-white.svg")
                    }
                );
        }
    }
}

fn player_play_button_from_state(state: &State) -> Containers {
    state.player.playback.as_ref().map_or_else(
        || player_play_button(false),
        |playback| player_play_button(playback.playing),
    )
}

fn player_current_album(host: &str, track: &ApiTrack, size: u16) -> Containers {
    container! {
        div #player-current-playing direction=row align-items=center {
            div width=(size) padding-x=20 align-items=center justify-content=center {
                anchor href=(format!("/albums?albumId={}&source={}", track.album_id, track.api_source)) width=(size) height=(size) {
                    (album_cover_img_from_album(host, &track.into(), size))
                }
            }
            div justify-content=center gap=3 {
                div {
                    anchor href=(format!("/albums?albumId={}&source={}", track.album_id, track.api_source)) { (track.title) }
                }
                div {
                    anchor href=(format!("/artists?artistId={}&source={}", track.artist_id, track.api_source)) { (track.artist) }
                }
                div direction=row {
                    "Playing from: " anchor href=(format!("/albums?albumId={}&source={}", track.album_id, track.api_source)) { (track.album) }
                }
            }
        }
    }
}

fn player_current_album_from_state(state: &State, size: u16) -> Containers {
    if let Some(connection) = &state.connection
        && let Some(playback) = &state.player.playback
    {
        let track: Option<&ApiTrack> = playback.tracks.get(playback.position as usize);

        if let Some(track) = track {
            return player_current_album(&connection.api_url, track, size);
        }
    }

    container! {
        div #player-current-playing direction=row {}
    }
}

fn player_current_progress(progress: f64, duration: f64) -> Containers {
    container! {
        div #player-current-progress {
            (progress.into_formatted()) " // " (duration.into_formatted())
        }
    }
}

fn player_current_progress_from_state(state: &State) -> Containers {
    if let Some(playback) = &state.player.playback {
        let track: Option<&ApiTrack> = playback.tracks.get(playback.position as usize);

        if let Some(track) = track {
            return player_current_progress(playback.seek, track.duration);
        }
    }

    container! {
        div #player-current-progress {}
    }
}

/// Renders UI updates when a playback session changes.
///
/// Returns partial DOM updates for the current playing track, play/pause button,
/// progress indicator, and volume slider based on what changed in the session.
#[must_use]
pub fn session_updated(
    state: &State,
    update: &ApiUpdateSession,
    session: &ApiSession,
) -> Containers {
    let mut partials = vec![];

    if update.position.is_some() || update.playlist.is_some() {
        log::debug!("session_updated: position or playlist updated");
        let track: Option<&ApiTrack> = session
            .playlist
            .tracks
            .get(session.position.unwrap_or(0) as usize);

        if let Some(connection) = &state.connection
            && let Some(track) = track
        {
            log::debug!("session_updated: rendering current playing");
            partials.extend(player_current_album(
                &connection.api_url,
                track,
                CURRENT_ALBUM_SIZE,
            ));
        }

        partials.extend(play_queue(state));
    }
    if let Some(playing) = update.playing {
        log::debug!("session_updated: rendering play button");
        partials.extend(player_play_button(playing));
    }
    if let Some(seek) = update.seek {
        let track: Option<&ApiTrack> = session
            .playlist
            .tracks
            .get(session.position.unwrap_or(0) as usize);

        if let Some(track) = track {
            log::debug!("session_updated: rendering current progress");
            partials.extend(player_current_progress(seek, track.duration));
        }
    }
    if let Some(volume) = update.volume {
        log::debug!("session_updated: rendering volume");
        partials.extend(volume_slider_value(FOOTER_ICON_SIZE, volume));
    }

    partials
}

/// Renders the footer section containing the player controls.
#[must_use]
pub fn footer(state: &State) -> Containers {
    container! {
        footer height=(FOOTER_HEIGHT) background=(DARK_BACKGROUND) {
            (player(state))
        }
    }
}

/// Renders the main content area wrapper.
///
/// Wraps the provided slot content in a scrollable main element.
#[must_use]
pub fn main(slot: &Containers) -> Containers {
    container! {
        main .main-content overflow-y=auto flex-grow=1 {
            (slot)
        }
    }
}

/// Renders the home page.
#[must_use]
pub fn home(state: &State) -> Containers {
    page(
        state,
        &container! {
            "home"
        },
    )
}

/// Renders the downloads page.
#[must_use]
pub fn downloads(state: &State) -> Containers {
    page(
        state,
        &container! {
            "downloads"
        },
    )
}

/// Renders a complete page with the application layout.
///
/// Wraps the provided content in the full page structure including navigation,
/// footer, and global UI elements.
///
/// # Panics
///
/// * If the `API_SOURCES` `RwLock` is poisoned
#[must_use]
pub fn page(state: &State, slot: &Containers) -> Containers {
    let api_sources = API_SOURCES
        .read()
        .unwrap()
        .iter()
        .cloned()
        .collect::<Vec<_>>();

    container! {
        div
            #root
            .dark
            width=100%
            height=100%
            position=relative
            color=#fff
        {
            section
                .navigation-bar-and-main-content
                direction=row
                height=calc(100% - FOOTER_HEIGHT)
            {
                (sidebar_navigation())
                (main(&slot))
            }
            (footer(state))
            (play_queue(state))
            (audio_zones())
            (playback_sessions())
            (search(state, &api_sources, false, false))
        }
    }
}

/// DOM element ID for the audio zones modal.
pub static AUDIO_ZONES_ID: &str = "audio-zones";
/// DOM element ID for the audio zones modal content.
pub static AUDIO_ZONES_CONTENT_ID: &str = "audio-zones-content";

/// Renders the audio zones modal dialog.
///
/// Displays a modal that loads audio zone content via `HyperChad`.
#[must_use]
pub fn audio_zones() -> Containers {
    modal(
        AUDIO_ZONES_ID,
        &container! {
            h1 { "Audio Zones" }
            button { "New" }
        },
        &container! {
            div hx-get="/audio-zones" hx-trigger="load" {
                "Loading..."
            }
        },
    )
}

/// DOM element ID for the playback sessions modal.
pub static PLAYBACK_SESSIONS_ID: &str = "playback-sessions";
/// DOM element ID for the playback sessions modal content.
pub static PLAYBACK_SESSIONS_CONTENT_ID: &str = "playback-sessions-content";
/// DOM element ID for the play queue panel.
pub static PLAY_QUEUE_ID: &str = "play-queue";

/// Renders the playback sessions modal dialog.
///
/// Displays a modal that loads playback session content via `HyperChad`.
#[must_use]
pub fn playback_sessions() -> Containers {
    modal(
        PLAYBACK_SESSIONS_ID,
        &container! {
            h1 { "Playback Sessions" }
            button { "New" }
        },
        &container! {
            div hx-get="/playback-sessions" hx-trigger="load" {
                "Loading..."
            }
        },
    )
}

/// Renders a reusable modal dialog component.
///
/// Creates a centered modal overlay with a close button and click-outside-to-close behavior.
#[must_use]
pub fn modal(id: &str, header: &Containers, content: &Containers) -> Containers {
    container! {
        div
            id=(id)
            visibility=hidden
            direction=row
            position=fixed
            width=100%
            height=100%
            align-items=center
        {
            div
                flex=1
                background=(DARK_BACKGROUND)
                margin-x=vw20
                min-height=calc(min(vh(90), 300))
                max-height=vh90
                border-radius=15
                fx-click-outside=fx { hide(id) }
                overflow-y=auto
            {
                div
                    direction=row
                    background=(DARK_BACKGROUND)
                    padding-x=30
                    padding-top=20
                    border-top-radius=15
                    justify-content=space-between
                    position=sticky
                    top=0
                {
                    div direction=row { (header) }
                    div direction=row justify-content=end {
                        @let icon_size = 20;
                        button
                            width=(icon_size)
                            height=(icon_size)
                            fx-click=fx { element(id).toggle_visibility() }
                        {
                            image
                                width=(icon_size)
                                height=(icon_size)
                                src=(public_img!("cross-white.svg"));
                        }
                    }
                }
                div
                    padding-x=30
                    padding-bottom=20
                {
                    (content)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_action_display_refresh_visualization() {
        let action = Action::RefreshVisualization;
        let json = action.to_string();
        assert!(json.contains(r#""type":"RefreshVisualization"#));
    }

    #[test]
    fn test_action_display_toggle_playback() {
        let action = Action::TogglePlayback;
        let json = action.to_string();
        assert!(json.contains(r#""type":"TogglePlayback"#));
    }

    #[test]
    fn test_action_display_filter_albums() {
        let action = Action::FilterAlbums {
            filtered_sources: vec![
                TrackApiSource::Local,
                TrackApiSource::Api(ApiSource::library()),
            ],
            sort: AlbumSort::ArtistAsc,
        };
        let json = action.to_string();
        assert!(json.contains(r#""type":"FilterAlbums"#));
        assert!(json.contains("filtered_sources"));
        assert!(json.contains("sort"));
    }

    #[test]
    fn test_action_display_play_album() {
        let action = Action::PlayAlbum {
            album_id: Id::Number(123),
            api_source: ApiSource::library(),
            version_source: Some(TrackApiSource::Local),
            sample_rate: Some(44100),
            bit_depth: Some(16),
        };
        let json = action.to_string();
        assert!(json.contains(r#""type":"PlayAlbum"#));
        assert!(json.contains("album_id"));
        assert!(json.contains("api_source"));
    }

    #[test]
    fn test_action_try_from_string_refresh_visualization() {
        let json = r#"{"type":"RefreshVisualization"}"#;
        let result = Action::try_from(json);
        assert!(result.is_ok());
        match result.unwrap() {
            Action::RefreshVisualization => {}
            _ => panic!("Expected RefreshVisualization"),
        }
    }

    #[test]
    fn test_action_try_from_string_toggle_playback() {
        let json = r#"{"type":"TogglePlayback"}"#;
        let result = Action::try_from(json);
        assert!(result.is_ok());
        match result.unwrap() {
            Action::TogglePlayback => {}
            _ => panic!("Expected TogglePlayback"),
        }
    }

    #[test]
    fn test_action_try_from_string_invalid() {
        let json = "not valid json";
        let result = Action::try_from(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_action_try_from_string_play_album() {
        let action = Action::PlayAlbum {
            album_id: Id::Number(456),
            api_source: ApiSource::library(),
            version_source: Some(TrackApiSource::Local),
            sample_rate: Some(96000),
            bit_depth: Some(24),
        };
        let json = action.to_string();
        let result = Action::try_from(json.as_str());
        assert!(result.is_ok());
        match result.unwrap() {
            Action::PlayAlbum {
                album_id,
                api_source,
                version_source,
                sample_rate,
                bit_depth,
            } => {
                assert_eq!(album_id, Id::Number(456));
                assert!(api_source.is_library());
                assert_eq!(version_source, Some(TrackApiSource::Local));
                assert_eq!(sample_rate, Some(96000));
                assert_eq!(bit_depth, Some(24));
            }
            _ => panic!("Expected PlayAlbum"),
        }
    }

    #[test]
    fn test_action_roundtrip_refresh_visualization() {
        let action = Action::RefreshVisualization;
        let json = action.to_string();
        let parsed = Action::try_from(json.as_str()).unwrap();
        match parsed {
            Action::RefreshVisualization => {}
            _ => panic!("Roundtrip failed"),
        }
    }

    #[test]
    fn test_action_roundtrip_play_tracks() {
        let action = Action::PlayTracks {
            track_ids: vec![Id::Number(1), Id::Number(2), Id::Number(3)],
            api_source: ApiSource::library(),
        };
        let json = action.to_string();
        let parsed = Action::try_from(json.as_str()).unwrap();
        match parsed {
            Action::PlayTracks {
                track_ids,
                api_source,
            } => {
                assert_eq!(track_ids.len(), 3);
                assert!(api_source.is_library());
            }
            _ => panic!("Roundtrip failed"),
        }
    }

    #[test]
    fn test_action_try_from_str_reference() {
        let json = r#"{"type":"NextTrack"}"#;
        let result = Action::try_from(json);
        assert!(result.is_ok());
        match result.unwrap() {
            Action::NextTrack => {}
            _ => panic!("Expected NextTrack"),
        }
    }

    #[test]
    fn test_action_try_from_string_reference() {
        let json = String::from(r#"{"type":"PreviousTrack"}"#);
        let result = Action::try_from(&json);
        assert!(result.is_ok());
        match result.unwrap() {
            Action::PreviousTrack => {}
            _ => panic!("Expected PreviousTrack"),
        }
    }

    #[test]
    fn test_action_try_from_owned_string() {
        let json = String::from(r#"{"type":"SetVolume"}"#);
        let result = Action::try_from(json);
        assert!(result.is_ok());
        match result.unwrap() {
            Action::SetVolume => {}
            _ => panic!("Expected SetVolume"),
        }
    }

    #[test]
    fn test_action_into_action_effect() {
        let action = Action::TogglePlayback;
        let effect = action.into_action_effect();
        // The effect should be created successfully (no panic)
        let _ = effect;
    }

    #[test]
    fn test_action_into_hyperchad_action() {
        let action = Action::NextTrack;
        let hyperchad_action: hyperchad::actions::Action = action.into();
        // The action should be converted successfully (no panic)
        let _ = hyperchad_action;
    }

    #[test]
    fn test_action_into_action_type() {
        let action = Action::PreviousTrack;
        let action_type: ActionType = action.into();
        match action_type {
            ActionType::Custom { action: _ } => {}
            _ => panic!("Expected Custom action type"),
        }
    }

    #[test]
    fn test_public_img_macro() {
        let path = public_img!("test.png");
        assert_eq!(path, "/public/img/test.png");
    }

    #[test]
    fn test_constants() {
        assert_eq!(VIZ_HEIGHT, 35);
        assert_eq!(VIZ_PADDING, 5);
        assert_eq!(FOOTER_BORDER_SIZE, 3);
        assert_eq!(
            FOOTER_HEIGHT,
            100 + VIZ_HEIGHT + VIZ_PADDING * 2 + FOOTER_BORDER_SIZE
        );
        assert_eq!(FOOTER_ICON_SIZE, 25);
        assert_eq!(CURRENT_ALBUM_SIZE, 70);
        assert_eq!(DARK_BACKGROUND, "#080a0b");
        assert_eq!(BACKGROUND, "#181a1b");
    }
}
