#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use actix_web::error::ErrorBadRequest;
use actix_web::Result;
use moosicbox_core::{
    integer_range::{parse_integer_ranges, ParseIntegersError},
    sqlite::{db::get_tracks, models::ApiSource},
};
use moosicbox_database::Database;
use player::TrackOrId;

pub mod api;
pub mod player;

pub async fn get_track_or_ids_from_track_id_ranges(
    db: &Box<dyn Database>,
    track_ids: &str,
    source: Option<ApiSource>,
    host: Option<&str>,
) -> Result<Vec<TrackOrId>> {
    let track_ids = parse_integer_ranges(track_ids).map_err(|e| match e {
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

    Ok(
        if !source.is_some_and(|x| x != ApiSource::Library) && host.is_none() {
            get_tracks(db, Some(track_ids.as_ref()))
                .await?
                .into_iter()
                .map(|track| TrackOrId::Track(Box::new(track.into())))
                .collect()
        } else {
            track_ids
                .into_iter()
                .map(|id| TrackOrId::Id(id as i32, source.unwrap_or(ApiSource::Library)))
                .collect()
        },
    )
}
