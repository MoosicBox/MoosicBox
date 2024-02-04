use crate::db::get_download_location;
use crate::download_album;
use crate::download_track_id;
use crate::DownloadAlbumError;
use crate::DownloadApiSource;
use crate::DownloadTrackError;
use actix_web::error::ErrorInternalServerError;
use actix_web::error::ErrorNotFound;
use actix_web::{
    route,
    web::{self, Json},
    Result,
};
use moosicbox_config::get_config_dir_path;
use moosicbox_files::files::track::TrackAudioQuality;
use serde::Deserialize;
use serde_json::Value;

impl From<DownloadTrackError> for actix_web::Error {
    fn from(err: DownloadTrackError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

impl From<DownloadAlbumError> for actix_web::Error {
    fn from(err: DownloadAlbumError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadQuery {
    location_id: Option<u64>,
    track_id: Option<u64>,
    album_id: Option<u64>,
    quality: Option<TrackAudioQuality>,
    source: Option<DownloadApiSource>,
}

#[route("/download", method = "POST")]
pub async fn download_endpoint(
    query: web::Query<DownloadQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    let path = if let Some(location_id) = query.location_id {
        get_download_location(
            &data
                .db
                .as_ref()
                .unwrap()
                .library
                .lock()
                .as_ref()
                .unwrap()
                .inner,
            location_id,
        )?
        .ok_or(ErrorNotFound("Database Location with id not found"))?
        .path
    } else {
        get_config_dir_path()
            .ok_or(ErrorInternalServerError(
                "Failed to get moosicbox config dir",
            ))?
            .join("downloads")
            .to_str()
            .unwrap()
            .to_string()
    };

    if let Some(track_id) = query.track_id {
        download_track_id(
            &data.db.as_ref().expect("No DB set"),
            &path,
            track_id,
            query.quality,
            query.source,
        )
        .await?;
    }

    if let Some(album_id) = query.album_id {
        download_album(
            &data.db.as_ref().expect("No DB set"),
            &path,
            album_id,
            query.quality,
            query.source,
        )
        .await?;
    }

    Ok(Json(serde_json::json!({"success": true})))
}
