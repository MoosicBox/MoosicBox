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
    actions::{
        self as hyperchad_actions, ActionType,
        logic::{
            get_height_px_str_id, get_mouse_x_self, get_mouse_y_str_id, get_visibility_str_id,
            get_width_px_self,
        },
    },
    template2::{self as hyperchad_template2, Containers, IntoActionEffect, container},
    transformer::models::Visibility,
};
use moosicbox_music_models::{
    API_SOURCES, AlbumSort, ApiSource, TrackApiSource, api::ApiTrack, id::Id,
};
use moosicbox_session_models::{ApiSession, ApiUpdateSession};
use play_queue::play_queue;
use search::search;
use serde::{Deserialize, Serialize};
use state::State;

pub const VIZ_HEIGHT: u16 = 35;
pub const VIZ_PADDING: u16 = 5;
pub const FOOTER_BORDER_SIZE: u16 = 3;
pub const FOOTER_HEIGHT: u16 = 100 + VIZ_HEIGHT + VIZ_PADDING * 2 + FOOTER_BORDER_SIZE;
pub const FOOTER_ICON_SIZE: u16 = 25;
pub const CURRENT_ALBUM_SIZE: u16 = 70;

pub const DARK_BACKGROUND: &str = "#080a0b";
pub const BACKGROUND: &str = "#181a1b";

#[macro_export]
macro_rules! public_img {
    ($path:expr $(,)?) => {
        concat!("/public/img/", $path)
    };
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    RefreshVisualization,
    TogglePlayback,
    PreviousTrack,
    NextTrack,
    SetVolume,
    SeekCurrentTrackPercent,
    FilterAlbums {
        filtered_sources: Vec<TrackApiSource>,
        sort: AlbumSort,
    },
    PlayAlbum {
        album_id: Id,
        api_source: ApiSource,
        version_source: Option<TrackApiSource>,
        sample_rate: Option<u32>,
        bit_depth: Option<u8>,
    },
    AddAlbumToQueue {
        album_id: Id,
        api_source: ApiSource,
        version_source: Option<TrackApiSource>,
        sample_rate: Option<u32>,
        bit_depth: Option<u8>,
    },
    PlayAlbumStartingAtTrackId {
        album_id: Id,
        start_track_id: Id,
        api_source: ApiSource,
        version_source: Option<TrackApiSource>,
        sample_rate: Option<u32>,
        bit_depth: Option<u8>,
    },
    PlayTracks {
        track_ids: Vec<Id>,
        api_source: ApiSource,
    },
}

impl IntoActionEffect for Action {
    fn into_action_effect(self) -> hyperchad_actions::ActionEffect {
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).unwrap())
    }
}

impl TryFrom<String> for Action {
    type Error = serde_json::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        serde_json::from_str(&value)
    }
}

impl TryFrom<&String> for Action {
    type Error = serde_json::Error;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        serde_json::from_str(value)
    }
}

impl<'a> TryFrom<&'a str> for Action {
    type Error = serde_json::Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        serde_json::from_str(value)
    }
}

#[must_use]
pub fn sidebar_navigation() -> Containers {
    container! {
        Aside width="calc(max(240, min(280, 15%)))" background=(DARK_BACKGROUND) {
            Div .navigation-bar padding=20 {
                @let size = 36;
                Div .navigation-bar-header direction=row align-items=center height=(size) {
                    @let icon_size = 36;
                    Anchor href="/" direction=row height=(icon_size) {
                        Image
                            width=(icon_size)
                            height=(icon_size)
                            src=(public_img!("icon128.png"));

                        H1 .navigation-bar-header-home-link-text font-size=20 {
                            "MoosicBox"
                        }
                    }
                    @let size = 22;
                    Div direction=row justify-content=end align-items=center height=(size) {
                        Anchor href="/settings" direction=row width=(size + 10) {
                            Image
                                width=(size)
                                height=(size)
                                src=(public_img!("settings-gear-white.svg"));
                        }
                        Div width=(size + 10) {
                            Image
                                width=(size)
                                height=(size)
                                src=(public_img!("chevron-left-white.svg"));
                        }
                    }
                }
                Ul {
                    Li {
                        Anchor href="/" {
                            "Home"
                        }
                    }
                    Li {
                        Anchor href="/downloads" {
                            "Downloads"
                        }
                    }
                }
                H1 .my-collection-header font-size=20 {
                    "My Collection"
                }
                Ul {
                    Li {
                        Anchor href="/albums" {
                            "Albums"
                        }
                    }
                    Li {
                        Anchor href="/artists" {
                            "Artists"
                        }
                    }
                }
            }
        }
    }
}

#[allow(clippy::too_many_lines)]
#[must_use]
pub fn player(state: &State) -> Containers {
    container! {
        Div height=(FOOTER_HEIGHT) border-top=((FOOTER_BORDER_SIZE, "#222")) {
            Div height=(VIZ_HEIGHT) padding-y=(VIZ_PADDING) direction=row {
                Canvas
                    #visualization
                    cursor="pointer"
                    width=100%
                    height=(VIZ_HEIGHT)
                    fx-click=(get_mouse_x_self().divide(get_width_px_self()).then_pass_to(Action::SeekCurrentTrackPercent))
                    fx-resize=(get_width_px_self().then_pass_to(Action::RefreshVisualization))
                    fx-immediate=(get_width_px_self().then_pass_to(Action::RefreshVisualization))
                {}
            }
            Div height=100 direction=row {
                Div flex=1 justify-content=center {
                    (player_current_album_from_state(state, 70))
                }
                Div flex=2 align-items=center justify-content=center {
                    @let button_size = 40;
                    @let progress_size = 20;
                    Div height=(button_size) direction=row justify-content=center {
                        Button
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
                            Image
                                width=(icon_size)
                                height=(icon_size)
                                src=(public_img!("previous-button-white.svg"));
                        }
                        (player_play_button_from_state(state))
                        Button
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
                            Image
                                width=(icon_size)
                                height=(icon_size)
                                src=(public_img!("next-button-white.svg"));
                        }
                    }
                    Div height=(progress_size) margin-top=10 {
                        (player_current_progress_from_state(state))
                    }
                }
                Div flex=1 direction=row justify-content=end align-items=center padding-right=20 {
                    (volume(state, FOOTER_ICON_SIZE))
                    Button
                        width=(FOOTER_ICON_SIZE)
                        height=(FOOTER_ICON_SIZE)
                        margin-left=10
                        fx-click=(
                            get_visibility_str_id(AUDIO_ZONES_ID)
                                .eq(Visibility::Hidden)
                                .then(ActionType::show_str_id(AUDIO_ZONES_ID))
                                .or_else(ActionType::hide_str_id(AUDIO_ZONES_ID))
                        )
                    {
                        Image
                            width=(FOOTER_ICON_SIZE)
                            height=(FOOTER_ICON_SIZE)
                            src=(public_img!("speaker-white.svg"));
                    }
                    Button
                        width=(FOOTER_ICON_SIZE)
                        height=(FOOTER_ICON_SIZE)
                        margin-left=10
                        fx-click=(
                            get_visibility_str_id(PLAYBACK_SESSIONS_ID)
                                .eq(Visibility::Hidden)
                                .then(ActionType::show_str_id(PLAYBACK_SESSIONS_ID))
                                .or_else(ActionType::hide_str_id(PLAYBACK_SESSIONS_ID))
                        )
                    {
                        Image
                            width=(FOOTER_ICON_SIZE)
                            height=(FOOTER_ICON_SIZE)
                            src=(public_img!("sessions-white.svg"));
                    }
                    Button
                        fx-click=(
                            get_visibility_str_id("play-queue")
                                .eq(Visibility::Hidden)
                                .then(ActionType::show_str_id("play-queue"))
                                .or_else(ActionType::hide_str_id("play-queue"))
                        )
                        width=(FOOTER_ICON_SIZE)
                        height=(FOOTER_ICON_SIZE)
                        margin-left=10
                    {
                        Image
                            width=(FOOTER_ICON_SIZE)
                            height=(FOOTER_ICON_SIZE)
                            src=(public_img!("playlist-white.svg"));
                    }
                }
            }
        }
    }
}

pub const VOLUME_SLIDER_CONTAINER_ID: &str = "volume-slider-container";
pub const VOLUME_SLIDER_ID: &str = "volume-slider";
pub const VOLUME_SLIDER_VALUE_CONTAINER_ID: &str = "volume-slider-value-container";
pub const VOLUME_SLIDER_VALUE_ID: &str = "volume-slider-value";

fn volume(state: &State, size: u16) -> Containers {
    let volume_percent = state.player.playback.as_ref().map_or(1.0, |x| x.volume);
    container! {
        Div
            id=(VOLUME_SLIDER_CONTAINER_ID)
            width=(size)
            height=(size)
            position=relative
            fx-hover=(ActionType::show_str_id(VOLUME_SLIDER_ID).delay_off(400))
        {
            Button {
                Image
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
        Div
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
            cursor="pointer"
            fx-mouse-down=(
                hyperchad::actions::logic::Arithmetic::group(
                    get_height_px_str_id(VOLUME_SLIDER_VALUE_CONTAINER_ID)
                        .minus(get_mouse_y_str_id(VOLUME_SLIDER_VALUE_CONTAINER_ID))
                )
                    .divide(get_height_px_str_id(VOLUME_SLIDER_VALUE_CONTAINER_ID))
                    .clamp(0.0, 1.0)
                    .then_pass_to(Action::SetVolume)
                    .throttle(30)
            )
            fx-hover=(ActionType::show_self().delay_off(400))
        {
            Div
                id=(VOLUME_SLIDER_VALUE_CONTAINER_ID)
                position=relative
                width=3
                height=100%
                border-radius=30
                background="#444"
            {
                (volume_slider_value(size, volume_percent))
            }
        }
    }
}

fn volume_slider_value(size: u16, volume_percent: f64) -> Containers {
    let height_percent = volume_percent * 100.0;
    container! {
        Div
            id=(VOLUME_SLIDER_VALUE_ID)
            position=absolute
            bottom=0
            left=0
            width=100%
            height=(format!("{height_percent}%"))
            border-radius=30
            background="#fff"
        {
            Div position=relative {
                @let slider_top_width = f32::from(size) / 2.5;
                Div
                    position=absolute
                    top=0
                    left=(format!("calc(50% - {})", slider_top_width / 2.0))
                    width=(slider_top_width)
                    height=3
                    border-radius=30
                    background="#fff"
                {}
            }
        }
    }
}

fn player_play_button(playing: bool) -> Containers {
    container! {
        @let button_size = 40;
        Button
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
            Image
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
        Div #player-current-playing direction=row align-items=center {
            Div width=(size) padding-x=20 align-items=center justify-content=center {
                Anchor href=(format!("/albums?albumId={}&source={}", track.album_id, track.api_source)) width=(size) height=(size) {
                    (album_cover_img_from_album(host, &track.into(), size))
                }
            }
            Div justify-content=center gap=3 {
                Div {
                    Anchor href=(format!("/albums?albumId={}&source={}", track.album_id, track.api_source)) { (track.title) }
                }
                Div {
                    Anchor href=(format!("/artists?artistId={}&source={}", track.artist_id, track.api_source)) { (track.artist) }
                }
                Div direction=row {
                    "Playing from: " Anchor href=(format!("/albums?albumId={}&source={}", track.album_id, track.api_source)) { (track.album) }
                }
            }
        }
    }
}

fn player_current_album_from_state(state: &State, size: u16) -> Containers {
    if let Some(connection) = &state.connection {
        if let Some(playback) = &state.player.playback {
            let track: Option<&ApiTrack> = playback.tracks.get(playback.position as usize);

            if let Some(track) = track {
                return player_current_album(&connection.api_url, track, size);
            }
        }
    }

    container! {
        Div #player-current-playing direction=row {}
    }
}

fn player_current_progress(progress: f64, duration: f64) -> Containers {
    container! {
        Div #player-current-progress {
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
        Div #player-current-progress {}
    }
}

#[must_use]
pub fn session_updated(
    state: &State,
    update: &ApiUpdateSession,
    session: &ApiSession,
) -> Vec<(String, Containers)> {
    let mut partials = vec![];

    if update.position.is_some() || update.playlist.is_some() {
        log::debug!("session_updated: position or playlist updated");
        let track: Option<&ApiTrack> = session
            .playlist
            .tracks
            .get(session.position.unwrap_or(0) as usize);

        if let Some(connection) = &state.connection {
            if let Some(track) = track {
                log::debug!("session_updated: rendering current playing");
                partials.push((
                    "player-current-playing".to_string(),
                    player_current_album(&connection.api_url, track, CURRENT_ALBUM_SIZE),
                ));
            }
        }

        partials.push(("play-queue".to_string(), play_queue(state)));
    }
    if let Some(playing) = update.playing {
        log::debug!("session_updated: rendering play button");
        partials.push((
            "player-play-button".to_string(),
            player_play_button(playing),
        ));
    }
    if let Some(seek) = update.seek {
        let track: Option<&ApiTrack> = session
            .playlist
            .tracks
            .get(session.position.unwrap_or(0) as usize);

        if let Some(track) = track {
            log::debug!("session_updated: rendering current progress");
            partials.push((
                "player-current-progress".to_string(),
                player_current_progress(seek, track.duration),
            ));
        }
    }
    if let Some(volume) = update.volume {
        log::debug!("session_updated: rendering volume");
        partials.push((
            VOLUME_SLIDER_VALUE_ID.to_string(),
            volume_slider_value(FOOTER_ICON_SIZE, volume),
        ));
    }

    partials
}

#[must_use]
pub fn footer(state: &State) -> Containers {
    container! {
        Footer height=(FOOTER_HEIGHT) background=(DARK_BACKGROUND) {
            (player(state))
        }
    }
}

#[must_use]
pub fn main(slot: &Containers) -> Containers {
    container! {
        Main .main-content overflow-y=auto flex-grow=1 {
            (slot)
        }
    }
}

#[must_use]
pub fn home(state: &State) -> Containers {
    page(
        state,
        &container! {
            "home"
        },
    )
}

#[must_use]
pub fn downloads(state: &State) -> Containers {
    page(
        state,
        &container! {
            "downloads"
        },
    )
}

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
        Div
            #root
            .dark
            width=100%
            height=100%
            position=relative
            color="#fff"
        {
            Section
                .navigation-bar-and-main-content
                direction=row
                height={"calc(100% - "(FOOTER_HEIGHT)")"}
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

pub static AUDIO_ZONES_ID: &str = "audio-zones";
pub static AUDIO_ZONES_CONTENT_ID: &str = "audio-zones-content";

#[must_use]
pub fn audio_zones() -> Containers {
    modal(
        AUDIO_ZONES_ID,
        &container! {
            H1 { "Audio Zones" }
            Button { "New" }
        },
        &container! {
            Div hx-get="/audio-zones" hx-trigger="load" {
                "Loading..."
            }
        },
    )
}

pub static PLAYBACK_SESSIONS_ID: &str = "playback-sessions";
pub static PLAYBACK_SESSIONS_CONTENT_ID: &str = "playback-sessions-content";

#[must_use]
pub fn playback_sessions() -> Containers {
    modal(
        PLAYBACK_SESSIONS_ID,
        &container! {
            H1 { "Playback Sessions" }
            Button { "New" }
        },
        &container! {
            Div hx-get="/playback-sessions" hx-trigger="load" {
                "Loading..."
            }
        },
    )
}

#[must_use]
pub fn modal(id: &str, header: &Containers, content: &Containers) -> Containers {
    container! {
        Div
            id=(id)
            visibility=hidden
            direction=row
            position=fixed
            width=100%
            height=100%
            align-items=center
        {
            Div
                flex=1
                background=(DARK_BACKGROUND)
                margin-x="calc(20vw)"
                min-height="calc(min(90vh, 300))"
                max-height="90vh"
                border-radius=15
                fx-click-outside=(
                    get_visibility_str_id(id)
                        .eq(Visibility::Visible)
                        .then(ActionType::hide_str_id(id))
                )
                overflow-y=auto
            {
                Div
                    direction=row
                    background=(DARK_BACKGROUND)
                    padding-x=30
                    padding-top=20
                    border-top-radius=15
                    justify-content=space-between
                    position=sticky
                    top=0
                {
                    Div direction=row { (header) }
                    Div direction=row justify-content=end {
                        @let icon_size = 20;
                        Button
                            width=(icon_size)
                            height=(icon_size)
                            fx-click=(
                                get_visibility_str_id(id)
                                    .eq(Visibility::Visible)
                                    .then(ActionType::hide_str_id(id))
                            )
                        {
                            Image
                                width=(icon_size)
                                height=(icon_size)
                                src=(public_img!("cross-white.svg"));
                        }
                    }
                }
                Div
                    padding-x=30
                    padding-bottom=20
                {
                    (content)
                }
            }
        }
    }
}
