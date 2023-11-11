use actix_web::{
    error::ErrorInternalServerError,
    web::{self},
    HttpRequest, Result,
};
use actix_web::{route, HttpResponse};
use moosicbox_core::app::AppState;
use moosicbox_files::files::track::{get_track_source, TrackSource};
use serde::Deserialize;

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetTrackQuery {
    track_id: i32,
}

#[route("/track", method = "GET", method = "HEAD")]
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<GetTrackQuery>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    match get_track_source(
        query.track_id,
        data.db
            .clone()
            .ok_or(ErrorInternalServerError("No DB set"))?,
    )
    .await?
    {
        TrackSource::LocalFilePath(path) => {
            let path_buf = std::path::PathBuf::from(path);

            Ok(actix_files::NamedFile::open_async(path_buf.as_path())
                .await?
                .into_response(&req))
        }
    }
}
