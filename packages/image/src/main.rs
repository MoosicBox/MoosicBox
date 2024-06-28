#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    io::{Seek as _, Write as _},
    path::Path,
    str::FromStr,
};

use clap::Parser;
use moosicbox_image::Encoding;
use thiserror::Error;

mod image;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(index = 1)]
    file: String,

    #[arg(short, long)]
    output: String,

    #[arg(short, long)]
    encoding: Option<String>,

    #[arg(short, long)]
    width: Option<u32>,

    #[arg(short, long)]
    height: Option<u32>,

    #[arg(short, long, default_value_t = 80)]
    quality: u8,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let encoding = args
        .encoding
        .map(|x| Encoding::from_str(&x.to_uppercase()))
        .transpose()
        .expect("Invalid encoding {}");

    try_resize_local_file(
        args.width,
        args.height,
        &args.file,
        &args.output,
        encoding,
        args.quality,
    )
    .await
    .expect("Failed to compress image");
}

#[derive(Error, Debug)]
pub enum ResizeLocalFileError {
    #[error(transparent)]
    ResizeImage(#[from] image::ResizeImageError),
    #[error(transparent)]
    Image(#[from] ::image::error::ImageError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error("Failed")]
    Failed,
}

async fn try_resize_local_file(
    width: Option<u32>,
    height: Option<u32>,
    path: &str,
    output: &str,
    encoding: Option<Encoding>,
    quality: u8,
) -> Result<(), ResizeLocalFileError> {
    let (width, height) = if let (Some(width), Some(height)) = (width, height) {
        (width, height)
    } else {
        let file_path = Path::new(path);
        let reader = ::image::io::Reader::open(file_path)?;
        let dimensions = reader.into_dimensions()?;

        if let Some(width) = width {
            (
                width,
                ((dimensions.1 as f64) * ((width as f64) / (dimensions.0 as f64))).round() as u32,
            )
        } else if let Some(height) = height {
            (
                ((dimensions.0 as f64) * ((height as f64) / (dimensions.1 as f64))).round() as u32,
                height,
            )
        } else {
            (
                width.unwrap_or(dimensions.0),
                height.unwrap_or(dimensions.1),
            )
        }
    };

    let output = Path::new(output);

    let encoding = encoding.unwrap_or(
        output
            .extension()
            .and_then(|ext| ext.to_ascii_uppercase().to_str().map(|x| x.to_string()))
            .and_then(|ext| Encoding::from_str(&ext).ok())
            .unwrap_or_else(|| {
                log::debug!("Defaulting encoding to Jpeg");
                Encoding::Jpeg
            }),
    );

    log::debug!("Resizing local image file path={path} width={width} height={height} encoding={encoding} quality={quality}");

    let bytes = image::try_resize_local_file_async(width, height, path, encoding, quality)
        .await?
        .ok_or(ResizeLocalFileError::Failed)?;

    save_bytes_to_file(&bytes, output, None)?;

    Ok(())
}

fn save_bytes_to_file(bytes: &[u8], path: &Path, start: Option<u64>) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(path.parent().expect("No parent directory"))?;

    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(!start.is_some_and(|start| start > 0))
        .open(path)?;

    let mut writer = std::io::BufWriter::new(file);

    if let Some(start) = start {
        writer.seek(std::io::SeekFrom::Start(start))?;
    }

    writer.write_all(bytes)
}
