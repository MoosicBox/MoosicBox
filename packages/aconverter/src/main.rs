#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{path::PathBuf, str::FromStr};

use clap::Parser;
use moosicbox_core::types::{from_extension_to_audio_format, AudioFormat};
use moosicbox_files::{
    files::track::{get_audio_bytes, TrackSource},
    save_bytes_stream_to_file,
};

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
    env_logger::init();

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

    println!(
        "env_a={:?} env_b={:?}",
        std::env::var("env_a").ok(),
        std::env::var("env_b").ok()
    );

    let bytes = get_audio_bytes(
        TrackSource::LocalFilePath {
            path: source.to_str().unwrap().to_string(),
            format: source_encoding,
        },
        output_encoding,
        None,
        None,
        None,
    )
    .await?;

    save_bytes_stream_to_file(bytes.stream, &output, None).await?;

    Ok(())
}
