//! Audio format converter command-line tool.
//!
//! This binary provides a CLI interface for converting audio files between different formats
//! (AAC, FLAC, MP3, Opus) with configurable quality settings. It preserves metadata tags
//! during conversion.
//!
//! # Features
//!
//! * Convert between multiple audio formats
//! * Preserve audio metadata (title, artist, album, etc.)
//! * Configurable quality settings
//! * Support for various input and output formats
//!
//! # Usage
//!
//! ```text
//! aconverter <FILE> --output <OUTPUT> [--encoding <ENCODING>] [--quality <QUALITY>]
//! ```
//!
//! # Examples
//!
//! Convert FLAC to MP3:
//! ```text
//! aconverter input.flac --output output.mp3 --quality 90
//! ```
//!
//! Convert with explicit encoding:
//! ```text
//! aconverter input.wav --output output.opus --encoding OPUS
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use clap::Parser;
use futures::StreamExt as _;
use moosicbox_audiotags::Tag;
use moosicbox_files::{files::track::get_audio_bytes, save_bytes_stream_to_file};
use moosicbox_music_api::models::TrackSource;
use moosicbox_music_models::{AudioFormat, TrackApiSource, from_extension_to_audio_format};
use thiserror::Error;

/// Command-line arguments for the audio converter.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Input audio file path.
    #[arg(index = 1)]
    file: String,

    /// Output audio file path.
    #[arg(short, long)]
    output: String,

    /// Target audio encoding format (e.g., "AAC", "FLAC", "MP3", "OPUS").
    ///
    /// If not specified, the format is inferred from the output file extension.
    #[arg(short, long)]
    encoding: Option<String>,

    /// Image width for embedded artwork (reserved for future use).
    #[arg(long)]
    width: Option<u32>,

    /// Image height for embedded artwork (reserved for future use).
    #[arg(long)]
    height: Option<u32>,

    /// Audio quality setting (0-100, default: 80).
    #[arg(short, long, default_value_t = 80)]
    quality: u8,
}

/// Converts an audio file from one format to another while preserving metadata.
///
/// Parses command-line arguments, reads the input audio file, converts it to the target format,
/// saves the output, and copies metadata tags from the input to the output file.
///
/// # Errors
///
/// * If the input or output file paths are invalid
/// * If the source or output file extensions are not recognized audio formats
/// * If the audio conversion fails
/// * If saving the converted audio file fails
/// * If copying metadata tags fails
///
/// # Panics
///
/// * If the logging system fails to initialize
/// * If the source or output paths lack a valid file extension
/// * If the file extension cannot be converted to a UTF-8 string
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init(Some("moosicbox_aconverter.log"), None)
        .expect("Failed to initialize FreeLog");

    let args = Args::parse();

    let source = PathBuf::from_str(&args.file)?;
    let output = PathBuf::from_str(&args.output)?;

    let source_extension = source.extension().unwrap().to_str().unwrap();
    let source_encoding = from_extension_to_audio_format(source_extension)
        .ok_or_else(|| format!("Invalid source extension '{source_extension}'"))?;

    let output_encoding = args.encoding.map_or_else(
        || {
            let extension = output.extension().unwrap().to_str().unwrap();
            Ok(from_extension_to_audio_format(extension)
                .or_else(|| AudioFormat::from_str(&extension.to_uppercase()).ok())
                .ok_or_else(|| format!("Invalid output extension '{extension}'"))?)
        },
        |x| {
            AudioFormat::from_str(&x.to_uppercase())
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        },
    )?;

    log::debug!(
        "Converting file ({}) => ({}) with {output_encoding:?} encoding",
        source.display(),
        output.display()
    );

    let bytes = get_audio_bytes(
        TrackSource::LocalFilePath {
            path: source.to_str().unwrap().to_string(),
            format: source_encoding,
            track_id: None,
            source: TrackApiSource::Local,
        },
        output_encoding,
        None,
        None,
        None,
    )
    .await?;

    log::debug!("Saving file ({})", output.display());

    save_bytes_stream_to_file(
        bytes.stream.map(|x| match x {
            Ok(Ok(x)) => Ok(x),
            Ok(Err(err)) | Err(err) => Err(err),
        }),
        &output,
        None,
    )
    .await?;

    tag_track_file(&source, &output)?;

    Ok(())
}

/// Error type for audio tag operations.
#[derive(Debug, Error)]
pub enum TagTrackFileError {
    /// Error reading or writing audio tags.
    #[error(transparent)]
    Tag(#[from] moosicbox_audiotags::Error),
}

/// Copies audio metadata tags from one file to another.
///
/// Reads tags (title, track number, album, artist, album artist, and date) from the input file
/// and writes them to the output file.
///
/// # Errors
///
/// * If the tags fail to read from the input file
/// * If the tags fail to write to the output file
///
/// # Panics
///
/// * If the output path cannot be converted to a UTF-8 string
pub fn tag_track_file(input_path: &Path, output_path: &Path) -> Result<(), TagTrackFileError> {
    log::debug!(
        "Reading source tags from input_path={}",
        input_path.display()
    );

    let input_tag = Tag::new().read_from_path(input_path)?;

    log::debug!("Copying tags to output_path={}", output_path.display());

    let mut output_tag = Tag::new().read_from_path(output_path)?;

    if let Some(title) = input_tag.title() {
        output_tag.set_title(title);
    }
    if let Some(number) = input_tag.track_number() {
        output_tag.set_track_number(number);
    }
    if let Some(album_title) = input_tag.album_title() {
        output_tag.set_album_title(album_title);
    }
    if let Some(artist) = input_tag.artist() {
        output_tag.set_artist(artist);
    }
    if let Some(album_artist) = input_tag.album_artist() {
        output_tag.set_album_artist(album_artist);
    }
    if let Some(date) = input_tag.date() {
        output_tag.set_date(date);
    }

    output_tag.write_to_path(output_path.to_str().unwrap())?;

    Ok(())
}
