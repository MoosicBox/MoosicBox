use crate::download;
use crate::DownloadError;
use actix_web::error::ErrorInternalServerError;
use actix_web::{
    route,
    web::{self, Json},
    Result,
};
use moosicbox_files::files::track::TrackAudioQuality;
use serde::Deserialize;
use serde_json::Value;

impl From<DownloadError> for actix_web::Error {
    fn from(err: DownloadError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadQuery {
    track_id: u64,
}

#[route("/download", method = "POST")]
pub async fn download_endpoint(
    query: web::Query<DownloadQuery>,
    data: web::Data<moosicbox_core::app::AppState>,
) -> Result<Json<Value>> {
    download(
        &data.db.as_ref().expect("No DB set"),
        query.track_id,
        Some(TrackAudioQuality::FlacHighestRes),
    )
    .await?;
    Ok(Json(serde_json::json!({"success": true})))
}
