#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use actix_web::error::{ErrorBadRequest, ErrorInternalServerError};
use actix_web::Result;
use moosicbox_core::{
    integer_range::{parse_integer_ranges_to_ids, ParseIntegersError},
    sqlite::models::ApiSource,
};
use moosicbox_music_api::MusicApi;
use player::Track;

#[cfg(feature = "api")]
pub mod api;

pub mod player;

pub async fn get_track_or_ids_from_track_id_ranges(
    api: &dyn MusicApi,
    track_ids: &str,
    host: Option<&str>,
) -> Result<Vec<Track>> {
    let track_ids = parse_integer_ranges_to_ids(track_ids).map_err(|e| match e {
        ParseIntegersError::ParseId(id) => {
            ErrorBadRequest(format!("Could not parse trackId '{id}'"))
        }
        ParseIntegersError::UnmatchedRange(range) => {
            ErrorBadRequest(format!("Unmatched range '{range}'"))
        }
        ParseIntegersError::RangeTooLarge(range) => {
            ErrorBadRequest(format!("Range too large '{range}'"))
        }
    })?;

    Ok(if api.source() == ApiSource::Library && host.is_none() {
        api.tracks(Some(track_ids.as_ref()), None, None, None, None)
            .await
            .map_err(|e| ErrorInternalServerError(format!("Failed to get tracks: {e:?}")))?
            .with_rest_of_items_in_batches()
            .await
            .map_err(|e| ErrorInternalServerError(format!("Failed to get tracks: {e:?}")))?
            .into_iter()
            .map(|track| Track {
                id: track.id.to_owned(),
                source: ApiSource::Library,
                data: Some(serde_json::to_value(track).unwrap()),
            })
            .collect()
    } else {
        track_ids
            .into_iter()
            .map(|id| Track {
                id,
                source: api.source(),
                data: None,
            })
            .collect()
    })
}
