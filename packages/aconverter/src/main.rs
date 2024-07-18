#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    path::{Path, PathBuf},
    str::FromStr,
};

use audiotags::Tag;
use clap::Parser;
use futures::StreamExt as _;
use moosicbox_core::{
    sqlite::models::TrackApiSource,
    types::{from_extension_to_audio_format, AudioFormat},
};
use moosicbox_files::{files::track::get_audio_bytes, save_bytes_stream_to_file};
use moosicbox_music_api::TrackSource;
use thiserror::Error;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(index = 1)]
    file: String,

    #[arg(short, long)]
    output: String,

    #[arg(short, long)]
    encoding: Option<String>,

    #[arg(long)]
    width: Option<u32>,

    #[arg(long)]
    height: Option<u32>,

    #[arg(short, long, default_value_t = 80)]
    quality: u8,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    moosicbox_logging::init("moosicbox_aconverter.log").expect("Failed to initialize FreeLog");

    let args = Args::parse();

    let source = PathBuf::from_str(&args.file)?;
    let output = PathBuf::from_str(&args.output)?;

    let source_extension = source.extension().unwrap().to_str().unwrap();
    let source_encoding = from_extension_to_audio_format(source_extension)
        .ok_or_else(|| format!("Invalid source extension '{source_extension}'"))?;

    let output_encoding = args
        .encoding
        .map(|x| {
            AudioFormat::from_str(&x.to_uppercase())
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
        })
        .unwrap_or_else(|| {
            let extension = output.extension().unwrap().to_str().unwrap();
            Ok(from_extension_to_audio_format(extension)
                .or_else(|| AudioFormat::from_str(&extension.to_uppercase()).ok())
                .ok_or_else(|| format!("Invalid output extension '{extension}'"))?)
        })?;

    log::debug!(
        "Converting file ({:?}) => ({:?}) with {:?} encoding",
        source,
        output,
        output_encoding
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

    log::debug!("Saving file ({output:?})");

    save_bytes_stream_to_file(
        bytes.stream.map(|x| match x {
            Ok(Ok(x)) => Ok(x),
            Ok(Err(err)) | Err(err) => Err(err),
        }),
        &output,
        None,
    )
    .await?;

    tag_track_file(&source, &output).await?;

    Ok(())
}

#[derive(Debug, Error)]
pub enum TagTrackFileError {
    #[error(transparent)]
    Tag(#[from] audiotags::Error),
}

pub async fn tag_track_file(
    input_path: &Path,
    output_path: &Path,
) -> Result<(), TagTrackFileError> {
    log::debug!("Reading source tags from input_path={input_path:?}");

    let input_tag = Tag::new().read_from_path(input_path)?;

    log::debug!("Copying tags to output_path={output_path:?}");

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
