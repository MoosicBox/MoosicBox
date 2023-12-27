use std::{
    env,
    fs::File,
    sync::{Arc, RwLock},
};

use log::{debug, error, trace};
use moosicbox_core::{
    app::{Db, DbConnection},
    sqlite::{
        db::{get_track, get_track_size, get_tracks, set_track_size, DbError, SetTrackSize},
        models::Track,
    },
    types::{AudioFormat, PlaybackQuality},
};
use moosicbox_stream_utils::ByteWriter;
use moosicbox_symphonia_player::{output::AudioOutputHandler, play_file_path_str, PlaybackError};
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use symphonia::core::{
    audio::{AudioBuffer, AudioBufferRef, Signal},
    conv::{FromSample, IntoSample},
    sample::Sample,
};
use thiserror::Error;

#[derive(Clone)]
pub enum TrackSource {
    LocalFilePath(String),
}

#[derive(Debug, Error)]
pub enum TrackSourceError {
    #[error("Track not found: {0}")]
    NotFound(i32),
    #[error("Invalid source")]
    InvalidSource,
    #[error(transparent)]
    Db(#[from] DbError),
}

pub async fn get_track_source(track_id: i32, db: Db) -> Result<TrackSource, TrackSourceError> {
    debug!("Getting track audio file {track_id}");

    let track = {
        let library = db.library.lock().unwrap();
        get_track(&library.inner, track_id)?
    };

    debug!("Got track {track:?}");

    if track.is_none() {
        return Err(TrackSourceError::NotFound(track_id));
    }

    let track = track.unwrap();

    match track.file {
        Some(file) => match env::consts::OS {
            "windows" => Ok(TrackSource::LocalFilePath(
                Regex::new(r"/mnt/(\w+)")
                    .unwrap()
                    .replace(&file, |caps: &Captures| {
                        format!("{}:", caps[1].to_uppercase())
                    })
                    .replace('/', "\\"),
            )),
            _ => Ok(TrackSource::LocalFilePath(file)),
        },
        None => Err(TrackSourceError::InvalidSource),
    }
}

#[derive(Debug, Error)]
pub enum TrackInfoError {
    #[error("Format not supported: {0:?}")]
    UnsupportedFormat(AudioFormat),
    #[error("Track not found: {0}")]
    NotFound(i32),
    #[error(transparent)]
    Playback(#[from] PlaybackError),
    #[error(transparent)]
    Db(#[from] DbError),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub id: i32,
    pub number: i32,
    pub title: String,
    pub duration: f64,
    pub album: String,
    pub album_id: i32,
    pub date_released: Option<String>,
    pub artist: String,
    pub artist_id: i32,
    pub blur: bool,
}

impl From<Track> for TrackInfo {
    fn from(value: Track) -> Self {
        TrackInfo {
            id: value.id,
            number: value.number,
            title: value.title,
            duration: value.duration,
            album: value.album,
            album_id: value.album_id,
            date_released: value.date_released,
            artist: value.artist,
            artist_id: value.artist_id,
            blur: value.blur,
        }
    }
}

pub async fn get_tracks_info(
    track_ids: Vec<i32>,
    db: Db,
) -> Result<Vec<TrackInfo>, TrackInfoError> {
    debug!("Getting tracks info {track_ids:?}");

    let tracks = {
        let library = db.library.lock().unwrap();
        get_tracks(&library.inner, &track_ids)?
    };

    trace!("Got tracks {tracks:?}");

    Ok(tracks.into_iter().map(|t| t.into()).collect())
}

pub async fn get_track_info(track_id: i32, db: Db) -> Result<TrackInfo, TrackInfoError> {
    debug!("Getting track info {track_id}");

    let track = {
        let library = db.library.lock().unwrap();
        get_track(&library.inner, track_id)?
    };

    trace!("Got track {track:?}");

    if track.is_none() {
        return Err(TrackInfoError::NotFound(track_id));
    }

    Ok(track.unwrap().into())
}

pub fn visualize(input: &AudioBufferRef<'_>) -> u8 {
    match input {
        AudioBufferRef::U8(input) => visualize_inner(input),
        AudioBufferRef::U16(input) => visualize_inner(input),
        AudioBufferRef::U24(input) => visualize_inner(input),
        AudioBufferRef::U32(input) => visualize_inner(input),
        AudioBufferRef::S8(input) => visualize_inner(input),
        AudioBufferRef::S16(input) => visualize_inner(input),
        AudioBufferRef::S24(input) => visualize_inner(input),
        AudioBufferRef::S32(input) => visualize_inner(input),
        AudioBufferRef::F32(input) => visualize_inner(input),
        AudioBufferRef::F64(input) => visualize_inner(input),
    }
}

fn visualize_inner<S>(input: &AudioBuffer<S>) -> u8
where
    S: Sample + FromSample<u8> + IntoSample<u8>,
{
    let channels = input.spec().channels.count();

    let mut step = 1_u64;
    let max = step * channels as u64;
    let mut count = 0_u64;
    let mut sum = 0_u64;

    for c in 0..channels {
        for x in input.chan(c) {
            sum += (*x).into_sample() as u64;
            count += 1;
            if count >= step {
                step += step;
                break;
            }
        }
        if count >= max {
            break;
        }
    }

    if count == 0 {
        return 0;
    }

    (sum / count) as u8
}

pub fn get_or_init_track_visualization(
    track_id: i32,
    source: &TrackSource,
    max: u16,
) -> Result<Vec<u8>, TrackInfoError> {
    debug!("Getting track visualization {track_id}");

    match source {
        TrackSource::LocalFilePath(ref path) => {
            let mut audio_output_handler = AudioOutputHandler::new();
            let viz = Arc::new(RwLock::new(vec![]));

            let inner_viz = viz.clone();
            audio_output_handler.with_filter(Box::new(move |decoded, _packet, _track| {
                inner_viz.write().unwrap().push(visualize(decoded));
                Ok(())
            }));

            play_file_path_str(path, &mut audio_output_handler, true, true, None, None)?;

            let viz = viz.read().unwrap();
            let mut ret_viz = Vec::with_capacity(std::cmp::min(max as usize, viz.len()));

            if viz.len() as u16 > max {
                let offset = (viz.len() as f64) / (max as f64);
                let mut last_pos = 0_usize;
                let mut pos = offset;

                while (pos as usize) < viz.len() {
                    let pos_usize = pos as usize;
                    let mut sum = viz[last_pos] as usize;
                    let mut count = 0_usize;

                    while pos_usize > last_pos {
                        last_pos += 1;
                        count += 1;
                        sum += viz[last_pos] as usize;
                    }

                    ret_viz.push((sum / count) as u8);
                    pos += offset;
                }

                if ret_viz.len() < max as usize {
                    ret_viz.push(viz[viz.len() - 1]);
                }
            }

            Ok(ret_viz)
        }
    }
}

pub fn get_or_init_track_size(
    track_id: i32,
    source: &TrackSource,
    quality: PlaybackQuality,
    connection: &DbConnection,
) -> Result<u64, TrackInfoError> {
    debug!("Getting track size {track_id}");

    if let Some(size) = get_track_size(&connection.inner, track_id, &quality)? {
        return Ok(size);
    }

    let writer = ByteWriter::default();

    let bytes = match source {
        TrackSource::LocalFilePath(ref path) => match quality.format {
            #[cfg(feature = "aac")]
            AudioFormat::Aac => {
                moosicbox_symphonia_player::output::encoder::aac::encoder::encode_aac(
                    path.to_string(),
                    writer.clone(),
                );
                writer.bytes_written()
            }
            #[cfg(feature = "flac")]
            AudioFormat::Flac => return Err(TrackInfoError::UnsupportedFormat(quality.format)),
            #[cfg(feature = "mp3")]
            AudioFormat::Mp3 => {
                moosicbox_symphonia_player::output::encoder::mp3::encoder::encode_mp3(
                    path.to_string(),
                    writer.clone(),
                );
                writer.bytes_written()
            }
            #[cfg(feature = "opus")]
            AudioFormat::Opus => {
                moosicbox_symphonia_player::output::encoder::opus::encoder::encode_opus(
                    path.to_string(),
                    writer.clone(),
                );
                writer.bytes_written()
            }
            AudioFormat::Source => File::open(path).unwrap().metadata().unwrap().len(),
        },
    };

    set_track_size(
        &connection.inner,
        SetTrackSize {
            track_id,
            quality,
            bytes,
            bit_depth: Some(None),
            audio_bitrate: Some(None),
            overall_bitrate: Some(None),
            sample_rate: Some(None),
            channels: Some(None),
        },
    )?;

    Ok(bytes)
}
