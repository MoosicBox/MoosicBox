#![allow(clippy::module_name_repetitions)]

use maud::{html, Markup};
use moosicbox_audio_zone_models::ApiAudioZoneWithSession;

use crate::AUDIO_ZONES_CONTENT_ID;

#[must_use]
pub fn audio_zones(zones: &[ApiAudioZoneWithSession]) -> Markup {
    html! {
        div id=(AUDIO_ZONES_CONTENT_ID) {
            @for zone in zones {
                div {
                    h1 { (zone.name) }
                    div {
                        @for player in &zone.players {
                            div { (player.audio_output_id) (player.name) }
                        }
                    }
                }
            }
        }
    }
}
