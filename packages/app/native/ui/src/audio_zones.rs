#![allow(clippy::module_name_repetitions)]

use hyperchad::template2::{self as hyperchad_template2, Containers, container};
use moosicbox_audio_zone_models::ApiAudioZoneWithSession;
use moosicbox_session_models::ApiConnection;

use crate::{AUDIO_ZONES_CONTENT_ID, public_img};

#[must_use]
pub fn audio_zones(zones: &[ApiAudioZoneWithSession], connections: &[ApiConnection]) -> Containers {
    container! {
        Div id=(AUDIO_ZONES_CONTENT_ID) {
            @for connection in connections {
                Div padding-y=(10) {
                    H1 direction="row" {
                        @let icon_size = 20;
                        Div
                            width=(icon_size)
                            height=(icon_size)
                            margin-right=(5)
                        {
                            Image
                                width=(icon_size)
                                height=(icon_size)
                                src=(public_img!("speaker-white.svg"));
                        }

                        (connection.name) " players"
                    }
                    Div {
                        @for player in &connection.players {
                            Div direction="row" {
                                @let icon_size = 20;
                                Div
                                    width=(icon_size)
                                    height=(icon_size)
                                    margin-right=(5)
                                {
                                    Image
                                        width=(icon_size)
                                        height=(icon_size)
                                        src=(public_img!("audio-white.svg"));
                                }

                                (player.name)
                            }
                        }
                    }
                }
            }
            @for zone in zones {
                Div padding-y=(10) {
                    H1 direction="row" {
                        @let icon_size = 20;
                        Div
                            width=(icon_size)
                            height=(icon_size)
                            margin-right=(5)
                        {
                            Image
                                width=(icon_size)
                                height=(icon_size)
                                src=(public_img!("speaker-white.svg"));
                        }

                        (zone.name)
                    }
                    Div {
                        @for player in &zone.players {
                            Div direction="row" {
                                @let icon_size = 20;
                                Div
                                    width=(icon_size)
                                    height=(icon_size)
                                    margin-right=(5)
                                {
                                    Image
                                        width=(icon_size)
                                        height=(icon_size)
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
