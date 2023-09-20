use actix_web::{error::ErrorInternalServerError, web, HttpRequest, HttpResponse, Result};
use lambda_web::actix_web::{self, get};
use moosicbox_core::{
    app::AppState,
    sqlite::db::{get_album, get_track},
};
use serde::Deserialize;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetTrackQuery {
    id: i32,
}

#[get("/track")]
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<GetTrackQuery>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let track = get_track(&data.db, query.id)
        .await
        .map_err(|_e| ErrorInternalServerError("Failed to fetch track"))?;

    if track.is_none() {
        return Err(ErrorInternalServerError("Failed to find track"));
    }

    let track = track.unwrap();

    if track.file.is_none() {
        return Err(ErrorInternalServerError("Track is not a local file"));
    }

    let path_buf = std::path::PathBuf::from(track.file.unwrap());
    let file_path = path_buf.as_path();

    let file = actix_files::NamedFile::open_async(file_path).await.unwrap();

    Ok(file.into_response(&req))
}

#[get("/albums/{album_id}/{size}")]
pub async fn album_artwork_endpoint(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let paths = path.into_inner();
    let album_id = paths
        .0
        .parse::<i32>()
        .map_err(|_e| ErrorInternalServerError("Invalid album_id"))?;

    let album = get_album(&data.db, album_id)
        .await
        .map_err(|_e| ErrorInternalServerError("Failed to fetch album"))?;

    if album.is_none() {
        return Err(ErrorInternalServerError("Failed to find album"));
    }

    let album = album.unwrap();

    if album.artwork.is_none() {
        return Err(ErrorInternalServerError("Album is does not have artwork"));
    }
    if album.directory.is_none() {
        return Err(ErrorInternalServerError("Album is not locally hosted"));
    }

    let path_buf = std::path::PathBuf::from(album.directory.unwrap()).join(album.artwork.unwrap());
    let file_path = path_buf.as_path();

    let file = actix_files::NamedFile::open_async(file_path).await.unwrap();

    Ok(file.into_response(&req))
}
