#![allow(clippy::module_name_repetitions)]

use maud::{html, Markup};
use moosicbox_audio_zone_models::ApiAudioZoneWithSession;
use moosicbox_session_models::ApiConnection;

use crate::{public_img, AUDIO_ZONES_CONTENT_ID};

#[must_use]
pub fn audio_zones(zones: &[ApiAudioZoneWithSession], connections: &[ApiConnection]) -> Markup {
    html! {
        div id=(AUDIO_ZONES_CONTENT_ID) {
            @for connection in connections {
                div sx-padding-y=(10) {
                    h1 sx-dir="row" {
                        @let icon_size = 20;
                        div
                            sx-width=(icon_size)
                            sx-height=(icon_size)
                            sx-margin-right=(5)
                        {
                            img
                                sx-width=(icon_size)
                                sx-height=(icon_size)
                                src=(public_img!("speaker-white.svg"));
                        }

                        (connection.name) " players"
                    }
                    div {
                        @for player in &connection.players {
                            div sx-dir="row" {
                                @let icon_size = 20;
                                div
                                    sx-width=(icon_size)
                                    sx-height=(icon_size)
                                    sx-margin-right=(5)
                                {
                                    img
                                        sx-width=(icon_size)
                                        sx-height=(icon_size)
                                        src=(public_img!("audio-white.svg"));
                                }

                                (player.name)
                            }
                        }
                    }
                }
            }
            @for zone in zones {
                div sx-padding-y=(10) {
                    h1 sx-dir="row" {
                        @let icon_size = 20;
                        div
                            sx-width=(icon_size)
                            sx-height=(icon_size)
                            sx-margin-right=(5)
                        {
                            img
                                sx-width=(icon_size)
                                sx-height=(icon_size)
                                src=(public_img!("speaker-white.svg"));
                        }

                        (zone.name)
                    }
                    div {
                        @for player in &zone.players {
                            div sx-dir="row" {
                                @let icon_size = 20;
                                div
                                    sx-width=(icon_size)
                                    sx-height=(icon_size)
                                    sx-margin-right=(5)
                                {
                                    img
                                        sx-width=(icon_size)
                                        sx-height=(icon_size)
                                        src=(public_img!("audio-white.svg"));
                                }

                                (player.name)
                            }
                        }
                    }
                }
            }
        }
    }
}
