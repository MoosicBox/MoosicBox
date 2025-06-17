#![allow(clippy::module_name_repetitions)]

use hyperchad::template::{self as hyperchad_template, Containers, container};
use moosicbox_audio_zone_models::ApiAudioZoneWithSession;
use moosicbox_session_models::ApiConnection;

use crate::{AUDIO_ZONES_CONTENT_ID, public_img};

#[must_use]
pub fn audio_zones(zones: &[ApiAudioZoneWithSession], connections: &[ApiConnection]) -> Containers {
    container! {
        div id=(AUDIO_ZONES_CONTENT_ID) {
            @for connection in connections {
                div padding-y=10 {
                    h1 direction=row {
                        @let icon_size = 20;
                        div
                            width=(icon_size)
                            height=(icon_size)
                            margin-right=5
                        {
                            image
                                width=(icon_size)
                                height=(icon_size)
                                src=(public_img!("speaker-white.svg"));
                        }

                        (connection.name) " players"
                    }
                    div {
                        @for player in &connection.players {
                            div direction=row {
                                @let icon_size = 20;
                                div
                                    width=(icon_size)
                                    height=(icon_size)
                                    margin-right=5
                                {
                                    image
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
                div padding-y=10 {
                    h1 direction=row {
                        @let icon_size = 20;
                        div
                            width=(icon_size)
                            height=(icon_size)
                            margin-right=5
                        {
                            image
                                width=(icon_size)
                                height=(icon_size)
                                src=(public_img!("speaker-white.svg"));
                        }

                        (zone.name)
                    }
                    div {
                        @for player in &zone.players {
                            div direction=row {
                                @let icon_size = 20;
                                div
                                    width=(icon_size)
                                    height=(icon_size)
                                    margin-right=5
                                {
                                    image
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
