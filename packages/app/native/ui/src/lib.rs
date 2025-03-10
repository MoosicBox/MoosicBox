#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::branches_sharing_code)]

pub mod albums;
pub mod artists;
pub mod audio_zones;
pub mod formatting;
pub mod play_queue;
pub mod playback_sessions;
pub mod settings;
pub mod state;

use albums::album_cover_img_from_album;
use formatting::TimeFormat;
use hyperchad_actions::{
    ActionType,
    logic::{
        get_height_px_str_id, get_mouse_x_self, get_mouse_y_str_id, get_visibility_str_id,
        get_width_px_self,
    },
};
use hyperchad_transformer_models::{
    AlignItems, JustifyContent, LayoutOverflow, Position, Visibility,
};
use maud::{Markup, html};
use moosicbox_music_models::{AlbumSort, ApiSource, TrackApiSource, api::ApiTrack, id::Id};
use moosicbox_session_models::{ApiSession, ApiUpdateSession};
use play_queue::play_queue;
use serde::{Deserialize, Serialize};
use state::State;

pub static VIZ_HEIGHT: u16 = 35;
pub static VIZ_PADDING: u16 = 5;
pub static FOOTER_BORDER_SIZE: u16 = 3;
pub static FOOTER_HEIGHT: u16 = 100 + VIZ_HEIGHT + VIZ_PADDING * 2 + FOOTER_BORDER_SIZE;
pub static FOOTER_ICON_SIZE: u16 = 25;
pub static CURRENT_ALBUM_SIZE: u16 = 70;

#[macro_export]
macro_rules! public_img {
    ($path:expr $(,)?) => {
        moosicbox_app_native_image::image!(concat!("../../public/img/", $path))
    };
}

#[macro_export]
macro_rules! pre_escaped {
    ($($message:tt)+) => {
        maud::PreEscaped(format!($($message)*))
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

impl From<Action> for hyperchad_actions::Action {
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

impl<'a> TryFrom<&'a str> for Action {
    type Error = serde_json::Error;

    fn try_from(value: &'a str) -> Result<Self, Self::Error> {
        serde_json::from_str(value)
    }
}

#[must_use]
pub fn sidebar_navigation() -> Markup {
    html! {
        aside sx-width="calc(max(240, min(280, 15%)))" sx-background="#080a0b" {
            div class="navigation-bar" sx-padding=(20) {
                @let size = 36;
                div class="navigation-bar-header" sx-dir="row" sx-align-items="center" sx-height=(size) {
                    @let icon_size = 36;
                    a href="/" sx-dir="row" sx-height=(icon_size) {
                        img
                            sx-width=(icon_size)
                            sx-height=(icon_size)
                            src=(public_img!("icon128.png"));

                        h1 class="navigation-bar-header-home-link-text" {
                            "MoosicBox"
                        }
                    }
                    @let size = 22;
                    div sx-dir="row" sx-justify-content="end" sx-align-items="center" sx-height=(size) {
                        a href="/settings" sx-dir="row" sx-width=(size + 10) {
                            img
                                sx-width=(size)
                                sx-height=(size)
                                src=(public_img!("settings-gear-white.svg"));
                        }
                        div sx-width=(size + 10) {
                            img
                                sx-width=(size)
                                sx-height=(size)
                                src=(public_img!("chevron-left-white.svg"));
                        }
                    }
                }
                ul {
                    li {
                        a href="/" {
                            "Home"
                        }
                    }
                    li {
                        a href="/downloads" {
                            "Downloads"
                        }
                    }
                }
                h1 class="my-collection-header" {
                    "My Collection"
                }
                ul {
                    li {
                        a href="/albums" {
                            "Albums"
                        }
                    }
                    li {
                        a href="/artists" {
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
pub fn player(state: &State) -> Markup {
    html! {
        div sx-height=(FOOTER_HEIGHT) sx-border-top={(FOOTER_BORDER_SIZE)", #222"} {
            div sx-height=(VIZ_HEIGHT) sx-padding-y=(VIZ_PADDING) sx-dir="row" {
                canvas
                    id="visualization"
                    sx-cursor="pointer"
                    sx-width="100%"
                    sx-height=(VIZ_HEIGHT)
                    fx-click=(get_mouse_x_self().divide(get_width_px_self()).then_pass_to(Action::SeekCurrentTrackPercent))
                    fx-resize=(get_width_px_self().then_pass_to(Action::RefreshVisualization))
                    fx-immediate=(get_width_px_self().then_pass_to(Action::RefreshVisualization))
                {}
            }
            div sx-height=(100) sx-dir="row" {
                div sx-flex=(1) {
                    (player_current_album_from_state(state, 70))
                }
                div sx-flex=(2) sx-align-items="center" {
                    @let button_size = 40;
                    @let progress_size = 20;
                    div sx-height=(button_size + progress_size) {
                        div sx-height=(button_size) sx-dir="row" sx-justify-content="center" sx-align-items="center" {
                            button
                                sx-width=(button_size)
                                sx-height=(button_size)
                                sx-margin-x=(5)
                                sx-dir="row"
                                sx-justify-content="center"
                                sx-align-items="center"
                                sx-background="#181a1b"
                                sx-border-radius="100%"
                                fx-click=(Action::PreviousTrack)
                            {
                                @let icon_size = 18;
                                img
                                    sx-width=(icon_size)
                                    sx-height=(icon_size)
                                    src=(public_img!("previous-button-white.svg"));
                            }
                            (player_play_button_from_state(state))
                            button
                                sx-width=(button_size)
                                sx-height=(button_size)
                                sx-margin-x=(5)
                                sx-dir="row"
                                sx-justify-content="center"
                                sx-align-items="center"
                                sx-background="#181a1b"
                                sx-border-radius="100%"
                                fx-click=(Action::NextTrack)
                            {
                                @let icon_size = 18;
                                img
                                    sx-width=(icon_size)
                                    sx-height=(icon_size)
                                    src=(public_img!("next-button-white.svg"));
                            }
                        }
                        div sx-height=(progress_size) sx-margin-top=(10) {
                            (player_current_progress_from_state(state))
                        }
                    }
                }
                div sx-flex=(1) sx-dir="row" sx-justify-content="end" sx-align-items="center" sx-padding-right=(20) {
                    (volume(state, FOOTER_ICON_SIZE))
                    button
                        sx-width=(FOOTER_ICON_SIZE)
                        sx-height=(FOOTER_ICON_SIZE)
                        sx-margin-left=(10)
                        fx-click=(
                            get_visibility_str_id(AUDIO_ZONES_ID)
                                .eq(Visibility::Hidden)
                                .then(ActionType::show_str_id(AUDIO_ZONES_ID))
                                .or_else(ActionType::hide_str_id(AUDIO_ZONES_ID))
                        )
                    {
                        img
                            sx-width=(FOOTER_ICON_SIZE)
                            sx-height=(FOOTER_ICON_SIZE)
                            src=(public_img!("speaker-white.svg"));
                    }
                    button
                        sx-width=(FOOTER_ICON_SIZE)
                        sx-height=(FOOTER_ICON_SIZE)
                        sx-margin-left=(10)
                        fx-click=(
                            get_visibility_str_id(PLAYBACK_SESSIONS_ID)
                                .eq(Visibility::Hidden)
                                .then(ActionType::show_str_id(PLAYBACK_SESSIONS_ID))
                                .or_else(ActionType::hide_str_id(PLAYBACK_SESSIONS_ID))
                        )
                    {
                        img
                            sx-width=(FOOTER_ICON_SIZE)
                            sx-height=(FOOTER_ICON_SIZE)
                            src=(public_img!("sessions-white.svg"));
                    }
                    button
                        fx-click=(
                            get_visibility_str_id("play-queue")
                                .eq(Visibility::Hidden)
                                .then(ActionType::show_str_id("play-queue"))
                                .or_else(ActionType::hide_str_id("play-queue"))
                        )
                        sx-width=(FOOTER_ICON_SIZE)
                        sx-height=(FOOTER_ICON_SIZE)
                        sx-margin-left=(10)
                    {
                        img
                            sx-width=(FOOTER_ICON_SIZE)
                            sx-height=(FOOTER_ICON_SIZE)
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

fn volume(state: &State, size: u16) -> Markup {
    let volume_percent = state.player.playback.as_ref().map_or(1.0, |x| x.volume);
    html! {
        div
            id=(VOLUME_SLIDER_CONTAINER_ID)
            sx-width=(size)
            sx-height=(size)
            sx-position="relative"
            fx-hover=(ActionType::show_str_id(VOLUME_SLIDER_ID).delay_off(400))
        {
            button {
                img
                    sx-width=(size)
                    sx-height=(size)
                    src=(public_img!("audio-white.svg"));
            }
            (volume_slider(size, volume_percent))
        }
    }
}

fn volume_slider(size: u16, volume_percent: f64) -> Markup {
    html! {
        div
            id=(VOLUME_SLIDER_ID)
            sx-visibility=(Visibility::Hidden)
            sx-width=(30)
            sx-height=(130)
            sx-padding-y=(15)
            sx-position="absolute"
            sx-bottom=(size)
            sx-left=(0)
            sx-margin-y=(5)
            sx-align-items="center"
            sx-justify-content="center"
            sx-border-radius=(30)
            sx-background="#181a1b"
            sx-cursor="pointer"
            fx-mouse-down=(
                hyperchad_actions::logic::Arithmetic::group(
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
            div
                id=(VOLUME_SLIDER_VALUE_CONTAINER_ID)
                sx-position="relative"
                sx-width=(3)
                sx-height="100%"
                sx-border-radius=(30)
                sx-background="#444"
            {
                (volume_slider_value(size, volume_percent))
            }
        }
    }
}

fn volume_slider_value(size: u16, volume_percent: f64) -> Markup {
    let height_percent = volume_percent * 100.0;
    html! {
        div
            id=(VOLUME_SLIDER_VALUE_ID)
            sx-position="absolute"
            sx-bottom=(0)
            sx-left=(0)
            sx-width="100%"
            sx-height=(format!("{height_percent}%"))
            sx-border-radius=(30)
            sx-background="#fff"
        {
            div sx-position="relative" {
                @let slider_top_width = f32::from(size) / 2.5;
                div
                    sx-position="absolute"
                    sx-top=(0)
                    sx-left=(format!("calc(50% - {})", slider_top_width / 2.0))
                    sx-width=(slider_top_width)
                    sx-height=(3)
                    sx-border-radius=(30)
                    sx-background="#fff"
                {}
            }
        }
    }
}

fn player_play_button(playing: bool) -> Markup {
    html! {
        @let button_size = 40;
        button
            id="player-play-button"
            sx-width=(button_size)
            sx-height=(button_size)
            sx-margin-x=(5)
            sx-dir="row"
            sx-justify-content="center"
            sx-align-items="center"
            sx-background="#181a1b"
            sx-border-radius="100%"
            fx-click=(Action::TogglePlayback)
        {
            @let icon_size = 16;
            img
                sx-width=(icon_size)
                sx-height=(icon_size)
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

fn player_play_button_from_state(state: &State) -> Markup {
    state.player.playback.as_ref().map_or_else(
        || player_play_button(false),
        |playback| player_play_button(playback.playing),
    )
}

fn player_current_album(track: &ApiTrack, size: u16) -> Markup {
    html! {
        div id="player-current-playing" sx-dir="row" sx-align-items="center" {
            div sx-width=(size) sx-padding-x=(20) sx-align-items="center" {
                a href=(pre_escaped!("/albums?albumId={}&source={}", track.album_id, track.api_source)) sx-width=(size) sx-height=(size) {
                    (album_cover_img_from_album(&track.into(), size))
                }
            }
            div sx-dir="row" sx-align-items="center" {
                div sx-height=(60) {
                    div sx-height=(20) {
                        a href=(pre_escaped!("/albums?albumId={}&source={}", track.album_id, track.api_source)) { (track.title) }
                    }
                    div sx-height=(20) {
                        a href=(pre_escaped!("/artists?artistId={}&source={}", track.artist_id, track.api_source)) { (track.artist) }
                    }
                    div sx-height=(20) sx-dir="row" {
                        "Playing from:" a href=(pre_escaped!("/albums?albumId={}&source={}", track.album_id, track.api_source)) { (track.album) }
                    }
                }
            }
        }
    }
}

fn player_current_album_from_state(state: &State, size: u16) -> Markup {
    if let Some(playback) = &state.player.playback {
        let track: Option<&ApiTrack> = playback.tracks.get(playback.position as usize);

        if let Some(track) = track {
            return player_current_album(track, size);
        }
    }

    html! {
        div id="player-current-playing" sx-dir="row" {}
    }
}

fn player_current_progress(progress: f64, duration: f64) -> Markup {
    html! {
        div id="player-current-progress" sx-justify-content="center" sx-align-content="center" {
            div sx-width=(70) {
                (progress.into_formatted()) " // " (duration.into_formatted())
            }
        }
    }
}

fn player_current_progress_from_state(state: &State) -> Markup {
    if let Some(playback) = &state.player.playback {
        let track: Option<&ApiTrack> = playback.tracks.get(playback.position as usize);

        if let Some(track) = track {
            return player_current_progress(playback.seek, track.duration);
        }
    }

    html! {
        div id="player-current-progress" {}
    }
}

#[must_use]
pub fn session_updated(
    state: &State,
    update: &ApiUpdateSession,
    session: &ApiSession,
) -> Vec<(String, Markup)> {
    let mut partials = vec![];

    if update.position.is_some() || update.playlist.is_some() {
        log::debug!("session_updated: position or playlist updated");
        let track: Option<&ApiTrack> = session
            .playlist
            .tracks
            .get(session.position.unwrap_or(0) as usize);

        if let Some(track) = track {
            log::debug!("session_updated: rendering current playing");
            partials.push((
                "player-current-playing".to_string(),
                player_current_album(track, CURRENT_ALBUM_SIZE),
            ));
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
pub fn footer(state: &State) -> Markup {
    html! {
        footer sx-height=(FOOTER_HEIGHT) sx-background="#080a0b" {
            (player(state))
        }
    }
}

#[must_use]
pub fn main(slot: &Markup) -> Markup {
    html! {
        main class="main-content" sx-overflow-y="auto" sx-flex-grow=(1) {
            (slot)
        }
    }
}

#[must_use]
pub fn home(state: &State) -> Markup {
    page(
        state,
        &html! {
            "home"
        },
    )
}

#[must_use]
pub fn downloads(state: &State) -> Markup {
    page(
        state,
        &html! {
            "downloads"
        },
    )
}

#[must_use]
pub fn page(state: &State, slot: &Markup) -> Markup {
    html! {
        div id="root" class="dark" sx-width="100%" sx-height="100%" sx-position="relative" sx-color="#fff" {
            section class="navigation-bar-and-main-content" sx-dir="row" sx-height=(format!("calc(100% - {FOOTER_HEIGHT})")) {
                (sidebar_navigation())
                (main(&slot))
            }
            (footer(state))
            (play_queue(state))
            (audio_zones())
            (playback_sessions())
        }
    }
}

pub static AUDIO_ZONES_ID: &str = "audio-zones";
pub static AUDIO_ZONES_CONTENT_ID: &str = "audio-zones-content";

#[must_use]
pub fn audio_zones() -> Markup {
    modal(
        AUDIO_ZONES_ID,
        &html! {
            h1 { "Audio Zones" }
            button { "New" }
        },
        &html! {
            div hx-get=(pre_escaped!("/audio-zones")) hx-trigger="load" {
                "Loading..."
            }
        },
    )
}

pub static PLAYBACK_SESSIONS_ID: &str = "playback-sessions";
pub static PLAYBACK_SESSIONS_CONTENT_ID: &str = "playback-sessions-content";

#[must_use]
pub fn playback_sessions() -> Markup {
    modal(
        PLAYBACK_SESSIONS_ID,
        &html! {
            h1 { "Playback Sessions" }
            button { "New" }
        },
        &html! {
            div hx-get=(pre_escaped!("/playback-sessions")) hx-trigger="load" {
                "Loading..."
            }
        },
    )
}

#[must_use]
pub fn modal(id: &str, header: &Markup, content: &Markup) -> Markup {
    html! {
        div
            id=(id)
            sx-visibility=(Visibility::Hidden)
            sx-dir="row"
            sx-position="fixed"
            sx-width="100%"
            sx-height="100%"
            sx-align-items=(AlignItems::Center)
        {
            div
                sx-flex=(1)
                sx-background="#080a0b"
                sx-margin-x="calc(20vw)"
                sx-min-height="calc(min(90vh, 300px))"
                sx-max-height="90vh"
                sx-border-radius=(15)
                fx-click-outside=(
                    get_visibility_str_id(id)
                        .eq(Visibility::Visible)
                        .then(ActionType::hide_str_id(id))
                )
                sx-overflow-y=(LayoutOverflow::Auto)
            {
                div
                    sx-dir="row"
                    sx-background="#080a0b"
                    sx-padding-x=(30)
                    sx-padding-top=(20)
                    sx-justify-content=(JustifyContent::SpaceBetween)
                    sx-position=(Position::Sticky)
                    sx-top=(0)
                {
                    div sx-dir="row" { (header) }
                    div sx-dir="row" sx-justify-content=(JustifyContent::End) {
                        @let icon_size = 20;
                        button
                            sx-width=(icon_size)
                            sx-height=(icon_size)
                            fx-click=(
                                get_visibility_str_id(id)
                                    .eq(Visibility::Visible)
                                    .then(ActionType::hide_str_id(id))
                            )
                        {
                            img
                                sx-width=(icon_size)
                                sx-height=(icon_size)
                                src=(public_img!("cross-white.svg"));
                        }
                    }
                }
                div
                    sx-padding-x=(30)
                    sx-padding-bottom=(20)
                {
                    (content)
                }
            }
        }
    }
}
