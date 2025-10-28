//! Formatting utilities for displaying music metadata.
//!
//! This module provides traits and functions for formatting time values, audio quality information,
//! album types, and other music-related data for display in the UI.

use moosicbox_date_utils::chrono::{NaiveDateTime, parse_date_time};
use moosicbox_music_models::{
    AlbumType, AlbumVersionQuality, ApiSource, AudioFormat, TrackApiSource,
    api::ApiAlbumVersionQuality,
};
use rust_decimal::{Decimal, RoundingStrategy};
use rust_decimal_macros::dec;

/// Formats time values into human-readable strings.
///
/// Converts numeric time values (in seconds) to formatted strings like "1:23" or "1:23:45".
pub trait TimeFormat {
    /// Converts the time value to a formatted string.
    fn into_formatted(self) -> String;
}

impl TimeFormat for f32 {
    fn into_formatted(self) -> String {
        f64::from(self).into_formatted()
    }
}

impl TimeFormat for f64 {
    fn into_formatted(self) -> String {
        #[allow(clippy::cast_sign_loss)]
        #[allow(clippy::cast_possible_truncation)]
        (self.round() as u64).into_formatted()
    }
}

impl TimeFormat for u64 {
    fn into_formatted(self) -> String {
        let hours = self / 60 / 60;
        let minutes = self / 60;
        let seconds = self % 60;

        if hours > 0 {
            format!("{hours}:{minutes}:{seconds:0>2}")
        } else {
            format!("{minutes}:{seconds:0>2}")
        }
    }
}

/// Formats API source values into display strings.
pub trait ApiSourceFormat {
    /// Converts the API source to a formatted string.
    fn into_formatted(self) -> String;
}

impl ApiSourceFormat for ApiSource {
    fn into_formatted(self) -> String {
        self.into()
    }
}

/// Formats track API source values into display strings.
pub trait TrackApiSourceFormat {
    /// Converts the track API source to a formatted string.
    fn into_formatted(self) -> String;
}

impl TrackApiSourceFormat for TrackApiSource {
    fn into_formatted(self) -> String {
        self.into()
    }
}

/// Formats audio format values into human-readable strings.
pub trait AudioFormatFormat {
    /// Converts the audio format to a formatted string.
    fn into_formatted(self) -> String;
}

impl AudioFormatFormat for AudioFormat {
    fn into_formatted(self) -> String {
        #[allow(unreachable_patterns)]
        match self {
            #[cfg(feature = "aac")]
            Self::Aac => "AAC".to_string(),
            #[cfg(feature = "flac")]
            Self::Flac => "FLAC".to_string(),
            #[cfg(feature = "mp3")]
            Self::Mp3 => "MP3".to_string(),
            #[cfg(feature = "opus")]
            Self::Opus => "OPUS".to_string(),
            Self::Source => "N/A".to_string(),
            #[cfg(not(feature = "_any-format"))]
            _ => {
                unimplemented!("Audio format is not enabled");
            }
        }
    }
}

/// Formats album version quality information into display strings.
///
/// Includes format, sample rate, and bit depth information.
pub trait AlbumVersionQualityFormat {
    /// Converts the album version quality to a formatted string.
    fn into_formatted(self) -> String;
}

impl AlbumVersionQualityFormat for AlbumVersionQuality {
    fn into_formatted(self) -> String {
        match self.source {
            TrackApiSource::Local => {
                let mut formatted = self.format.expect("Missing format").into_formatted();

                if let Some(sample_rate) = self.sample_rate {
                    if !formatted.is_empty() {
                        formatted.push(' ');
                    }
                    let sample_rate = Decimal::from(sample_rate) / dec!(1000);
                    let sample_rate = sample_rate
                        .round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
                    formatted.push_str(&sample_rate.normalize().to_string());
                    formatted.push_str(" kHz");
                }
                if let Some(bit_depth) = self.bit_depth {
                    if !formatted.is_empty() {
                        formatted.push(' ');
                    }
                    formatted.push_str(&bit_depth.to_string());
                    formatted.push_str("-bit");
                }

                formatted
            }
            TrackApiSource::Api(..) => self.source.into_formatted(),
        }
    }
}

impl AlbumVersionQualityFormat for ApiAlbumVersionQuality {
    fn into_formatted(self) -> String {
        let quality: AlbumVersionQuality = self.into();
        quality.into_formatted()
    }
}

/// Displays a list of album version qualities as a single formatted string.
///
/// Qualities are separated by " / " and truncated with a count indicator if they exceed `max_characters`.
#[must_use]
pub fn display_album_version_qualities<T: AlbumVersionQualityFormat>(
    mut qualities: impl Iterator<Item = T>,
    max_characters: Option<usize>,
) -> String {
    const SEPARATOR: &str = " / ";

    let mut formatted = String::new();

    if let Some(first) = qualities.next() {
        formatted.push_str(&first.into_formatted());
    }

    while let Some(quality) = qualities.next() {
        let display = quality.into_formatted();

        if max_characters.is_some_and(|max| formatted.len() + display.len() + SEPARATOR.len() > max)
        {
            formatted.push_str(" (+");
            formatted.push_str(&(qualities.count() + 1).to_string());
            formatted.push(')');
            break;
        }

        formatted.push_str(SEPARATOR);
        formatted.push_str(&display);
    }

    formatted
}

/// Formats album type values into human-readable category names.
pub trait AlbumTypeFormat {
    /// Converts the album type to a formatted string.
    fn into_formatted(self) -> String;
}

impl AlbumTypeFormat for AlbumType {
    fn into_formatted(self) -> String {
        match self {
            Self::Lp | Self::Download => "Albums".to_string(),
            Self::Live => "Live Albums".to_string(),
            Self::Compilations => "Compilations".to_string(),
            Self::EpsAndSingles => "EPs and Singles".to_string(),
            Self::Other => "Other Albums".to_string(),
        }
    }
}

/// Formats a date string into a specific format.
///
/// Returns "n/a" if the date string cannot be parsed.
#[must_use]
pub fn format_date_string(date_string: &str, format: &str) -> String {
    // January 08, 2025
    let Ok(date) = parse_date_time(date_string) else {
        return "n/a".to_string();
    };
    format_date(&date, format)
}

/// Formats a date into a string using the specified format.
#[must_use]
pub fn format_date(date: &NaiveDateTime, format: &str) -> String {
    // January 08, 2025
    date.format(format).to_string()
}

/// Formats a byte size into a human-readable string.
#[must_use]
pub fn format_size(size: u64) -> String {
    bytesize::ByteSize::b(size).to_string()
}

/// Converts a name to a CSS class-friendly format.
///
/// Converts to lowercase and replaces non-alphanumeric characters with hyphens.
#[must_use]
pub fn classify_name<T: AsRef<str>>(class: T) -> String {
    let class = class.as_ref();
    class
        .to_ascii_lowercase()
        .replace(|c: char| !c.is_ascii_alphanumeric(), "-")
}
