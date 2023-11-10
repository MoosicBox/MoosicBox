use audiotags::{AudioTag, Tag};
use log::info;
use moosicbox_core::{
    app::AppState,
    sqlite::{
        db::{
            add_album_and_get_album, add_album_map_and_get_album, add_artist_and_get_artist,
            add_artist_map_and_get_artist, add_tracks, DbError, InsertTrack, SqliteValue,
        },
        models::Track,
    },
};
use regex::Regex;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    num::ParseIntError,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("No DB set")]
    NoDb,
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error(transparent)]
    Tag(#[from] audiotags::error::Error),
    #[error(transparent)]
    IO(#[from] std::io::Error),
}

pub fn scan(directory: &str, data: &AppState) -> Result<(), ScanError> {
    scan_dir(Path::new(directory).to_path_buf(), &|p| {
        create_track(p, data)
    })
}

fn save_bytes_to_file(bytes: &[u8], path: &PathBuf) {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .unwrap();

    let _ = file.write_all(bytes);
}

fn search_for_artwork(
    path: PathBuf,
    file_name: &str,
    tag: Option<Box<dyn AudioTag>>,
) -> Option<PathBuf> {
    if let Some(cover_file) = fs::read_dir(path.clone())
        .unwrap()
        .filter_map(|p| p.ok())
        .find(|p| {
            let name = p.file_name().to_str().unwrap().to_lowercase();
            name.starts_with(format!("{file_name}.").as_str())
        })
        .map(|dir| dir.path())
    {
        Some(cover_file)
    } else if let Some(tag) = tag {
        if let Some(tag_cover) = tag.album_cover() {
            let cover_file_path = match tag_cover.mime_type {
                audiotags::MimeType::Png => path.join(format!("{file_name}.png")),
                audiotags::MimeType::Jpeg => path.join(format!("{file_name}.jpg")),
                audiotags::MimeType::Tiff => path.join(format!("{file_name}.tiff")),
                audiotags::MimeType::Bmp => path.join(format!("{file_name}.bmp")),
                audiotags::MimeType::Gif => path.join(format!("{file_name}.gif")),
            };
            save_bytes_to_file(tag_cover.data, &cover_file_path);
            Some(cover_file_path)
        } else {
            None
        }
    } else {
        None
    }
}

fn create_track(path: PathBuf, data: &AppState) -> Result<(), ScanError> {
    let tag = Tag::new().read_from_path(path.to_str().unwrap())?;

    let duration = if path.to_str().unwrap().ends_with(".mp3") {
        mp3_duration::from_path(path.as_path())
            .unwrap()
            .as_secs_f64()
    } else {
        tag.duration().unwrap()
    };

    let bytes = {
        File::open(path.to_str().unwrap())
            .unwrap()
            .metadata()
            .unwrap()
            .len()
    };
    let title = tag.title().unwrap().to_string();
    let number = tag.track_number().unwrap_or(1) as i32;
    let album = tag.album_title().unwrap_or("(none)").to_string();
    let artist_name = tag.artist().or(tag.album_artist()).unwrap().to_string();
    let album_artist = tag
        .album_artist()
        .unwrap_or(artist_name.as_str())
        .to_string();
    let date_released = tag.date().map(|date| date.to_string());

    let multi_artist_pattern = Regex::new(r"\S,\S").unwrap();

    let path_artist = path.clone().parent().unwrap().parent().unwrap().to_owned();
    let artist_dir_name = path_artist
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let path_album = path.clone().parent().unwrap().to_owned();
    let album_dir_name = path_album
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    info!("====== {} ======", path.clone().to_str().unwrap());
    info!("title: {}", title);
    info!("number: {}", number);
    info!("duration: {}", duration);
    info!("bytes: {}", bytes);
    info!("album title: {}", album);
    info!("artist directory name: {}", artist_dir_name);
    info!("album directory name: {}", album_dir_name);
    info!("artist: {}", artist_name.clone());
    info!("album_artist: {}", album_artist.clone());
    info!("date_released: {:?}", date_released);
    info!("contains cover: {:?}", tag.album_cover().is_some());

    let album_artist = match multi_artist_pattern.find(album_artist.as_str()) {
        Some(comma) => album_artist[..comma.start() + 1].to_string(),
        None => album_artist,
    };

    let library = data
        .db
        .as_ref()
        .ok_or(ScanError::NoDb)?
        .library
        .lock()
        .unwrap();

    let mut artist = add_artist_map_and_get_artist(
        &library,
        HashMap::from([("title", SqliteValue::String(album_artist))]),
    )?;

    let mut album = add_album_map_and_get_album(
        &library,
        HashMap::from([
            ("title", SqliteValue::String(album)),
            ("artist_id", SqliteValue::Number(artist.id as i64)),
            ("date_released", SqliteValue::StringOpt(date_released)),
            (
                "directory",
                SqliteValue::StringOpt(path_album.to_str().map(|p| p.to_string())),
            ),
        ]),
    )?;

    info!("artwork: {:?}", album.artwork);

    if album.artwork.is_none() {
        if let Some(artwork) = search_for_artwork(path_album.clone(), "cover", Some(tag)) {
            album.artwork = Some(artwork.file_name().unwrap().to_str().unwrap().to_string());
            info!(
                "Found artwork for {}: {}",
                path_album.to_str().unwrap(),
                album.artwork.clone().unwrap()
            );
            album = add_album_and_get_album(&library, album)?;
        }
    }
    if artist.cover.is_none() {
        if let Some(cover) = search_for_artwork(path_album.clone(), "artist", None) {
            artist.cover = Some(cover.to_str().unwrap().to_string());
            info!(
                "Found cover for {}: {}",
                path_album.to_str().unwrap(),
                artist.cover.clone().unwrap()
            );
            let _ = add_artist_and_get_artist(&library, artist)?;
        }
    }

    let _track_id = add_tracks(
        &library,
        vec![InsertTrack {
            album_id: album.id,
            file: path.to_str().unwrap().to_string(),
            track: Track {
                number,
                title,
                duration,
                bytes,
                ..Default::default()
            },
        }],
    );

    Ok(())
}

fn scan_dir<F>(path: PathBuf, fun: &F) -> Result<(), ScanError>
where
    F: Fn(PathBuf) -> Result<(), ScanError>,
{
    let music_file_pattern = Regex::new(r".+\.(flac|m4a|mp3)").unwrap();

    let dir = match fs::read_dir(path) {
        Ok(dir) => dir,
        Err(_err) => return Ok(()),
    };

    for p in dir.filter_map(|p| p.ok()) {
        let metadata = p.metadata().unwrap();

        if metadata.is_dir() {
            scan_dir(p.path(), fun)?;
        } else if metadata.is_file()
            && music_file_pattern.is_match(p.path().file_name().unwrap().to_str().unwrap())
        {
            fun(p.path())?;
        }
    }

    Ok(())
}
