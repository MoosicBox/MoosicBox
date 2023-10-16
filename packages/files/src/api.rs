use std::env;

use actix_web::{error::ErrorInternalServerError, web, HttpRequest, HttpResponse, Result};
use lambda_web::actix_web::{self, get};
use moosicbox_core::{
    app::AppState,
    sqlite::db::{get_album, get_artist, get_track},
};
use regex::{Captures, Regex};
use serde::Deserialize;

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetTrackQuery {
    track_id: i32,
}

#[get("/track")]
pub async fn track_endpoint(
    req: HttpRequest,
    query: web::Query<GetTrackQuery>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let track = get_track(data.db.as_ref().unwrap(), query.track_id)
        .map_err(|_e| ErrorInternalServerError("Failed to fetch track"))?;

    if track.is_none() {
        return Err(ErrorInternalServerError("Failed to find track"));
    }

    let track = track.unwrap();

    let file = match track.file {
        Some(file) => match env::consts::OS {
            "windows" => Regex::new(r"/mnt/(\w+)")
                .unwrap()
                .replace(&file, |caps: &Captures| {
                    format!("{}:", caps[1].to_uppercase())
                })
                .replace('/', "\\"),
            _ => file,
        },
        None => return Err(ErrorInternalServerError("Track is not a local file")),
    };

    let path_buf = std::path::PathBuf::from(file);
    let file_path = path_buf.as_path();

    let file = actix_files::NamedFile::open_async(file_path).await.unwrap();

    Ok(file.into_response(&req))
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

    let artist = get_artist(data.db.as_ref().unwrap(), artist_id)
        .await
        .map_err(|_e| ErrorInternalServerError("Failed to fetch artist"))?;

    if artist.is_none() {
        return Err(ErrorInternalServerError("Failed to find artist"));
    }

    let artist = artist.unwrap();

    if artist.cover.is_none() {
        return Err(ErrorInternalServerError("Album is does not have cover"));
    }

    let cover = match artist.cover {
        Some(cover) => match env::consts::OS {
            "windows" => Regex::new(r"/mnt/(\w+)")
                .unwrap()
                .replace(&cover, |caps: &Captures| {
                    format!("{}:", caps[1].to_uppercase())
                })
                .replace('/', "\\"),
            _ => cover.to_string(),
        },
        None => unreachable!(),
    };

    let path_buf = std::path::PathBuf::from(cover);
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

    let album = get_album(data.db.as_ref().unwrap(), album_id)
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

    let directory = match album.directory {
        Some(file) => match env::consts::OS {
            "windows" => Regex::new(r"/mnt/(\w+)")
                .unwrap()
                .replace(&file, |caps: &Captures| {
                    format!("{}:", caps[1].to_uppercase())
                })
                .replace('/', "\\"),
            _ => file,
        },
        None => return Err(ErrorInternalServerError("Track is not a local file")),
    };

    let path_buf = std::path::PathBuf::from(directory).join(album.artwork.unwrap());
    let file_path = path_buf.as_path();

    let file = actix_files::NamedFile::open_async(file_path).await.unwrap();

    Ok(file.into_response(&req))
}
