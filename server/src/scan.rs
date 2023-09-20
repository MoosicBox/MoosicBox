use audiotags::Tag;
use moosicbox_core::{
    app::AppState,
    slim::{menu::Album, player::Track},
    sqlite::db::{add_album_and_get_value, add_tracks, DbError, InsertTrack},
};
use std::{
    fs::{self, DirEntry},
    num::ParseIntError,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    DbError(#[from] DbError),
    #[error(transparent)]
    ParseIntError(#[from] ParseIntError),
}

pub fn scan(directory: &str, data: &AppState) -> Result<(), ScanError> {
    scan_dir(Path::new(directory).to_path_buf(), &|p| {
        create_track(p, data)
    })
}

fn search_for_artwork(path: PathBuf) -> Option<DirEntry> {
    fs::read_dir(path)
        .unwrap()
        .filter_map(|p| p.ok())
        .find(|p| {
            let name = p.file_name().to_str().unwrap().to_lowercase();
            name.starts_with("cover.")
        })
}

fn create_track(path: PathBuf, data: &AppState) -> Result<(), ScanError> {
    let tag = Tag::new().read_from_path(path.to_str().unwrap()).unwrap();

    let title = tag.title().unwrap().to_string();
    let album = tag.album_title().unwrap_or("(none)").to_string();
    let artist = tag.artist().or(tag.album_artist()).unwrap().to_string();
    let date_released = tag.date_released().map(|date| date.to_string());

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

    println!("====== {} ======", path.clone().to_str().unwrap());
    println!("title: {}", title);
    println!("album title: {}", album);
    println!("artist directory name: {}", artist_dir_name);
    println!("album directory name: {}", album_dir_name);
    println!("artist: {}", artist.clone());
    println!("date_released: {:?}", date_released);
    println!("contains cover: {:?}", tag.album_cover().is_some());

    let mut album = add_album_and_get_value(
        &data.db,
        Album {
            title: album_dir_name,
            artist: artist_dir_name,
            date_released,
            directory: path_album.to_str().map(|p| p.to_string()),
            ..Default::default()
        },
    )?;

    println!("artwork: {:?}", album.artwork);

    if album.artwork.is_none() {
        if let Some(artwork) = search_for_artwork(path_album.clone()) {
            album.artwork = Some(
                artwork
                    .path()
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
            );
            println!(
                "Found artwork for {}: {}",
                path_album.to_str().unwrap(),
                album.artwork.clone().unwrap()
            );
            album = add_album_and_get_value(&data.db, album)?;
        }
    }

    let _track_id = add_tracks(
        &data.db,
        vec![InsertTrack {
            album_id: album.id.parse::<i32>()?,
            file: path.to_str().unwrap().to_string(),
            track: Track {
                title,
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
    for p in fs::read_dir(path).unwrap().filter_map(|p| p.ok()) {
        let metadata = p.metadata().unwrap();

        if metadata.is_dir() {
            scan_dir(p.path(), fun)?;
        } else if metadata.is_file()
            && p.path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .ends_with(".flac")
        {
            fun(p.path())?;
        }
    }

    Ok(())
}
