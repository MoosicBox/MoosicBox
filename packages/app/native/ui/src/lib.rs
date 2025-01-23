#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::branches_sharing_code)]

pub mod albums;
pub mod artists;
pub mod formatting;
pub mod play_queue;
pub mod settings;
pub mod state;

use albums::album_cover_img_from_album;
use formatting::TimeFormat;
use gigachad_actions::{
    logic::{get_height_px_str_id, get_mouse_y_str_id, get_visibility_str_id},
    ActionType,
};
use gigachad_transformer_models::Visibility;
use maud::{html, Markup};
use moosicbox_core::sqlite::models::{ApiSource, ApiTrack, Id, TrackApiSource};
use moosicbox_session_models::{ApiSession, ApiUpdateSession};
use play_queue::play_queue;
use serde::{Deserialize, Serialize};
use state::State;

static VIZ_HEIGHT: u16 = 35;
static VIZ_PADDING: u16 = 5;
static FOOTER_BORDER_SIZE: u16 = 3;
static FOOTER_HEIGHT: u16 = 100 + VIZ_HEIGHT + VIZ_PADDING * 2 + FOOTER_BORDER_SIZE;
static FOOTER_ICON_SIZE: u16 = 25;
static CURRENT_ALBUM_SIZE: u16 = 70;

#[macro_export]
macro_rules! public_img {
    ($path:expr $(,)?) => {
        moosicbox_app_native_image::image!(concat!("../../../../../app-website/public/img/", $path))
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
    TogglePlayback,
    PreviousTrack,
    NextTrack,
    SetVolume,
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

impl From<Action> for gigachad_actions::Action {
    fn from(value: Action) -> Self {
        Self {
            trigger: gigachad_actions::ActionTrigger::Immediate,
            action: ActionType::Custom {
                action: value.to_string(),
            },
        }
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

#[must_use]
pub fn player(state: &State) -> Markup {
    html! {
        div sx-height=(FOOTER_HEIGHT) sx-border-top={(FOOTER_BORDER_SIZE)", #222"} {
            div sx-height=(VIZ_HEIGHT) sx-padding-y=(VIZ_PADDING) sx-dir="row" {
                canvas id="visualization" sx-cursor="pointer" {}
            }
            div sx-height=(100) sx-dir="row" {
                (player_current_album_from_state(state, 70))
                div sx-align-items="center" {
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
                div sx-dir="row" sx-justify-content="end" sx-align-items="center" sx-padding-right=(20) {
                    (volume(state, FOOTER_ICON_SIZE))
                    button sx-width=(FOOTER_ICON_SIZE) sx-height=(FOOTER_ICON_SIZE) sx-margin-left=(10) {
                        img
                            sx-width=(FOOTER_ICON_SIZE)
                            sx-height=(FOOTER_ICON_SIZE)
                            src=(public_img!("speaker-white.svg"));
                    }
                    button sx-width=(FOOTER_ICON_SIZE) sx-height=(FOOTER_ICON_SIZE) sx-margin-left=(10) {
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

fn volume(state: &State, size: u16) -> Markup {
    let volume_percent = state.player.playback.as_ref().map_or(1.0, |x| x.volume);
    html! {
        div
            sx-width=(size)
            sx-height=(size)
            sx-position="relative"
            fx-hover=(ActionType::show_str_id("volume-slider"))
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
    const VOLUME_SLIDER_VALUE_CONTAINER_ID: &str = "volume-slider-value-container";
    html! {
        div
            id="volume-slider"
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
            sx-border-radius="100%"
            sx-background="#181a1b"
            sx-cursor="pointer"
            fx-mouse-down=(
                get_height_px_str_id(VOLUME_SLIDER_VALUE_CONTAINER_ID)
                    .minus(get_mouse_y_str_id(VOLUME_SLIDER_VALUE_CONTAINER_ID))
                    .divide(get_height_px_str_id(VOLUME_SLIDER_VALUE_CONTAINER_ID))
                    .clamp(0.0, 1.0)
                    .then_pass_to(Action::SetVolume)
            )
        {
            div
                id=(VOLUME_SLIDER_VALUE_CONTAINER_ID)
                sx-position="relative"
                sx-width=(3)
                sx-height="100%"
                sx-border-radius="100%"
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
            id="volume-slider-value"
            sx-position="absolute"
            sx-bottom=(0)
            sx-left=(0)
            sx-width="100%"
            sx-height=(format!("{height_percent}%"))
            sx-border-radius="100%"
            sx-background="#fff"
        {
            div sx-position="relative" {
                @let slider_top_width = f32::from(size) / 3.0;
                div
                    sx-position="absolute"
                    sx-top=(0)
                    sx-left=(format!("calc(50% - {})", slider_top_width / 2.0))
                    sx-width=(slider_top_width)
                    sx-height=(3)
                    sx-border-radius="100%"
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
            "volume-slider-value".to_string(),
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
        main class="main-content" sx-overflow-y="auto" {
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
        div state=(state) id="root" class="dark" sx-width="100%" sx-height="100%" sx-position="relative" {
            section class="navigation-bar-and-main-content" sx-dir="row" sx-height=(pre_escaped!("calc(100% - {})", FOOTER_HEIGHT)) {
                (sidebar_navigation())
                (main(&slot))
            }
            (footer(state))
            (play_queue(state))
        }
    }
}
