#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::branches_sharing_code)]

pub mod albums;
pub mod artists;
pub mod formatting;
pub mod settings;
pub mod state;

use albums::album_cover_img;
use maud::{html, Markup};
use moosicbox_core::sqlite::models::Track;
use moosicbox_session_models::{ApiSession, UpdateSession};
use serde::{Deserialize, Serialize};
use state::State;

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

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Action {
    TogglePlayback,
    PreviousTrack,
    NextTrack,
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
            div class="navigation-bar" {
                div class="navigation-bar-header" sx-dir="row" {
                    a href="/" sx-dir="row" {
                        @let size = 36;
                        img
                            sx-width=(size)
                            sx-height=(size)
                            src=(public_img!("icon128.png"));

                        h1 class="navigation-bar-header-home-link-text" {
                            ("MoosicBox")
                        }
                    }
                    @let size = 22;
                    a href="/settings" sx-dir="row" sx-width=(size + 10) {
                        img
                            sx-width=(size)
                            sx-height=(size)
                            src=(public_img!("settings-gear-white.svg"));
                    }
                    @let size = 22;
                    div sx-width=(size + 10) {
                        img
                            sx-width=(size)
                            sx-height=(size)
                            src=(public_img!("chevron-left-white.svg"));
                    }
                }
                ul {
                    li {
                        a href="/" {
                            ("Home")
                        }
                    }
                    li {
                        a href="/downloads" {
                            ("Downloads")
                        }
                    }
                }
                h1 class="my-collection-header" {
                    ("My Collection")
                }
                ul {
                    li {
                        a href="/albums" {
                            ("Albums")
                        }
                    }
                    li {
                        a href="/artists" {
                            ("Artists")
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
        div sx-height=(100) sx-dir="row" sx-border-top="3, #222" {
            (player_current_album_from_state(state))
            div sx-dir="row" {
                @let size = 36;
                button sx-width=(size) sx-height=(size) fx-click=(Action::PreviousTrack) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(public_img!("previous-button-white.svg"));
                }
                (player_play_button_from_state(state))
                button sx-width=(size) sx-height=(size) fx-click=(Action::NextTrack) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(public_img!("next-button-white.svg"));
                }
            }
            div sx-dir="row" {
                @let size = 25;
                button sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(public_img!("audio-white.svg"));
                }
                button sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(public_img!("speaker-white.svg"));
                }
                button sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(public_img!("sessions-white.svg"));
                }
                button sx-width=(size) sx-height=(size) {
                    img
                        sx-width=(size)
                        sx-height=(size)
                        src=(public_img!("playlist-white.svg"));
                }
            }
        }
    }
}

fn player_play_button(playing: bool) -> Markup {
    html! {
        @let size = 28;
        button id="player-play-button" sx-width=(size) sx-height=(size) fx-click=(Action::TogglePlayback) {
            img
                sx-width=(size)
                sx-height=(size)
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

fn player_current_album(track: &Track) -> Markup {
    html! {
        div id="player-current-playing" sx-dir="row" {
            @let size = 70;
            div sx-width=(size) sx-height=(size) {
                a href=(pre_escaped!("/albums?albumId={}&source={}", track.album_id, track.api_source)) sx-width=(size) sx-height=(size) {
                    (album_cover_img(&track.into(), size))
                }
            }
            div {
                div {
                    a href=(pre_escaped!("/albums?albumId={}&source={}", track.album_id, track.api_source)) { (track.title) }
                }
                div {
                    a href=(pre_escaped!("/artists?artistId={}&source={}", track.artist_id, track.api_source)) { (track.artist) }
                }
                div sx-dir="row" {
                    "Playing from:" a href=(pre_escaped!("/albums?albumId={}&source={}", track.album_id, track.api_source)) { (track.album) }
                }
            }
        }
    }
}

fn player_current_album_from_state(state: &State) -> Markup {
    if let Some(playback) = &state.player.playback {
        let track: Result<Option<Track>, _> = playback
            .tracks
            .get(playback.position as usize)
            .map(TryInto::try_into)
            .transpose();

        if let Ok(Some(track)) = track {
            return player_current_album(&track);
        }
    }

    html! {
        div id="player-current-playing" sx-dir="row" {}
    }
}

#[must_use]
pub fn session_updated(update: &UpdateSession, session: &ApiSession) -> Vec<(String, Markup)> {
    let mut partials = vec![];

    if update.position.is_some() || update.playlist.is_some() {
        let track: Result<Option<Track>, _> = session
            .playlist
            .tracks
            .get(session.position.unwrap_or(0) as usize)
            .map(TryInto::try_into)
            .transpose();

        match track {
            Ok(Some(track)) => {
                partials.push((
                    "player-current-playing".to_string(),
                    player_current_album(&track),
                ));
            }
            Ok(None) => {}
            Err(e) => {
                log::error!("session_updated: {e:?}");
            }
        }
    }
    if let Some(playing) = update.playing {
        partials.push((
            "player-play-button".to_string(),
            player_play_button(playing),
        ));
    }

    partials
}

#[must_use]
pub fn footer(state: &State) -> Markup {
    html! {
        footer sx-height=(100) sx-background="#080a0b" {
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
            ("home")
        },
    )
}

#[must_use]
pub fn downloads(state: &State) -> Markup {
    page(
        state,
        &html! {
            ("downloads")
        },
    )
}

#[must_use]
pub fn page(state: &State, slot: &Markup) -> Markup {
    html! {
        div state=(state) id="root" class="dark" sx-width="100%" sx-height="100%" {
            section class="navigation-bar-and-main-content" sx-dir="row" sx-height="calc(100% - 100px)" {
                (sidebar_navigation())
                (main(&slot))
            }
            (footer(state))
        }
    }
}
