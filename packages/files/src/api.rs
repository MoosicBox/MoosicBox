use actix_web::{error::ErrorInternalServerError, route, web, HttpRequest, HttpResponse, Result};
use lambda_web::actix_web::{self, get};
use log::error;
use moosicbox_core::app::AppState;
use serde::Deserialize;
use thiserror::Error;

use crate::files::{
    album::{get_album_cover, AlbumCoverError, AlbumCoverSource},
    artist::{get_artist_cover, ArtistCoverError, ArtistCoverSource},
    track::{get_track_source, TrackSource, TrackSourceError},
};

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GetTrackQuery {
    track_id: i32,
}

impl From<TrackSourceError> for actix_web::Error {
    fn from(err: TrackSourceError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[route("/track", method = "GET", method = "HEAD")]
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<GetTrackQuery>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    match get_track_source(query.track_id, data.db.clone().expect("No DB set")).await? {
        TrackSource::LocalFilePath(path) => {
            let path_buf = std::path::PathBuf::from(path);

            Ok(actix_files::NamedFile::open_async(path_buf.as_path())
                .await?
                .into_response(&req))
        }
    }
}

impl From<ArtistCoverError> for actix_web::Error {
    fn from(err: ArtistCoverError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
}

#[get("/artists/{artist_id}/{size}")]
pub async fn artist_cover_endpoint(
    req: HttpRequest,
    path: web::Path<(String, String)>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let paths = path.into_inner();
    let artist_id = paths
        .0
        .parse::<i32>()
        .map_err(|_e| ErrorInternalServerError("Invalid artist_id"))?;

    match get_artist_cover(artist_id, data.db.clone().expect("No DB set")).await? {
        ArtistCoverSource::LocalFilePath(path) => {
            let path_buf = std::path::PathBuf::from(path);

            Ok(actix_files::NamedFile::open_async(path_buf.as_path())
                .await?
                .into_response(&req))
        }
    }
}

#[derive(Debug, Error)]
pub enum AlbumArtworkError {
    #[error("Failed to read file with path: {0} ({1})")]
    File(String, String),
}

impl From<AlbumCoverError> for actix_web::Error {
    fn from(err: AlbumCoverError) -> Self {
        ErrorInternalServerError(err.to_string())
    }
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

    match get_album_cover(album_id, data.db.clone().expect("No DB set")).await? {
        AlbumCoverSource::LocalFilePath(path) => {
            let path_buf = std::path::PathBuf::from(path);
            let file_path = path_buf.as_path();

            let file = actix_files::NamedFile::open_async(file_path)
                .await
                .map_err(|e| {
                    AlbumArtworkError::File(file_path.to_str().unwrap().into(), format!("{e:?}"))
                })
                .unwrap();

            Ok(file.into_response(&req))
        }
    }
}
