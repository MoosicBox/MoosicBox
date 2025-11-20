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

#[cfg(test)]
mod tests {
    use super::*;
    use moosicbox_music_models::AudioFormat;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_time_format_u64_seconds_only() {
        let seconds: u64 = 45;
        assert_eq!(seconds.into_formatted(), "0:45");
    }

    #[test]
    fn test_time_format_u64_minutes_and_seconds() {
        let seconds: u64 = 135; // 2:15
        assert_eq!(seconds.into_formatted(), "2:15");
    }

    #[test]
    fn test_time_format_u64_hours_minutes_seconds() {
        let seconds: u64 = 3665; // 1:01:05
        assert_eq!(seconds.into_formatted(), "1:61:05");
    }

    #[test]
    fn test_time_format_u64_zero() {
        let seconds: u64 = 0;
        assert_eq!(seconds.into_formatted(), "0:00");
    }

    #[test]
    fn test_time_format_u64_exactly_one_minute() {
        let seconds: u64 = 60;
        assert_eq!(seconds.into_formatted(), "1:00");
    }

    #[test]
    fn test_time_format_u64_exactly_one_hour() {
        let seconds: u64 = 3600;
        assert_eq!(seconds.into_formatted(), "1:60:00");
    }

    #[test]
    fn test_time_format_f64_rounds_correctly() {
        let seconds: f64 = 45.4;
        assert_eq!(seconds.into_formatted(), "0:45");

        let seconds: f64 = 45.6;
        assert_eq!(seconds.into_formatted(), "0:46");
    }

    #[test]
    fn test_time_format_f32_rounds_correctly() {
        let seconds: f32 = 45.4;
        assert_eq!(seconds.into_formatted(), "0:45");

        let seconds: f32 = 45.6;
        assert_eq!(seconds.into_formatted(), "0:46");
    }

    #[test]
    fn test_api_source_format_library() {
        let source = ApiSource::library();
        let formatted = source.into_formatted();
        assert!(formatted.contains("Library") || formatted.contains("LIBRARY"));
    }

    #[test]
    fn test_track_api_source_format_local() {
        let source = TrackApiSource::Local;
        assert_eq!(source.into_formatted(), "LOCAL");
    }

    #[cfg(feature = "aac")]
    #[test]
    fn test_audio_format_aac() {
        let format = AudioFormat::Aac;
        assert_eq!(format.into_formatted(), "AAC");
    }

    #[cfg(feature = "flac")]
    #[test]
    fn test_audio_format_flac() {
        let format = AudioFormat::Flac;
        assert_eq!(format.into_formatted(), "FLAC");
    }

    #[cfg(feature = "mp3")]
    #[test]
    fn test_audio_format_mp3() {
        let format = AudioFormat::Mp3;
        assert_eq!(format.into_formatted(), "MP3");
    }

    #[cfg(feature = "opus")]
    #[test]
    fn test_audio_format_opus() {
        let format = AudioFormat::Opus;
        assert_eq!(format.into_formatted(), "OPUS");
    }

    #[test]
    fn test_audio_format_source() {
        let format = AudioFormat::Source;
        assert_eq!(format.into_formatted(), "N/A");
    }

    #[cfg(feature = "flac")]
    #[test]
    fn test_album_version_quality_format_local_with_all_fields() {
        let quality = AlbumVersionQuality {
            format: Some(AudioFormat::Flac),
            bit_depth: Some(24),
            sample_rate: Some(96000),
            channels: None,
            source: TrackApiSource::Local,
        };
        assert_eq!(quality.into_formatted(), "FLAC 96 kHz 24-bit");
    }

    #[cfg(feature = "flac")]
    #[test]
    fn test_album_version_quality_format_local_format_only() {
        let quality = AlbumVersionQuality {
            format: Some(AudioFormat::Flac),
            bit_depth: None,
            sample_rate: None,
            channels: None,
            source: TrackApiSource::Local,
        };
        assert_eq!(quality.into_formatted(), "FLAC");
    }

    #[cfg(feature = "flac")]
    #[test]
    fn test_album_version_quality_format_local_with_sample_rate() {
        let quality = AlbumVersionQuality {
            format: Some(AudioFormat::Flac),
            bit_depth: None,
            sample_rate: Some(44100),
            channels: None,
            source: TrackApiSource::Local,
        };
        assert_eq!(quality.into_formatted(), "FLAC 44.1 kHz");
    }

    #[cfg(feature = "flac")]
    #[test]
    fn test_album_version_quality_format_local_with_bit_depth() {
        let quality = AlbumVersionQuality {
            format: Some(AudioFormat::Flac),
            bit_depth: Some(16),
            sample_rate: None,
            channels: None,
            source: TrackApiSource::Local,
        };
        assert_eq!(quality.into_formatted(), "FLAC 16-bit");
    }

    #[test]
    fn test_album_version_quality_format_api_source() {
        let quality = AlbumVersionQuality {
            format: None,
            bit_depth: None,
            sample_rate: None,
            channels: None,
            source: TrackApiSource::Api(ApiSource::library()),
        };
        let formatted = quality.into_formatted();
        assert!(
            formatted.contains("Library")
                || formatted.contains("LIBRARY")
                || formatted.contains("API")
        );
    }

    #[cfg(feature = "flac")]
    #[test]
    fn test_display_album_version_qualities_single() {
        let qualities = vec![AlbumVersionQuality {
            format: Some(AudioFormat::Flac),
            bit_depth: Some(16),
            sample_rate: Some(44100),
            channels: None,
            source: TrackApiSource::Local,
        }];
        let result = display_album_version_qualities(qualities.into_iter(), None);
        assert_eq!(result, "FLAC 44.1 kHz 16-bit");
    }

    #[cfg(feature = "flac")]
    #[test]
    fn test_display_album_version_qualities_multiple() {
        let qualities = vec![
            AlbumVersionQuality {
                format: Some(AudioFormat::Flac),
                bit_depth: Some(16),
                sample_rate: Some(44100),
                channels: None,
                source: TrackApiSource::Local,
            },
            AlbumVersionQuality {
                format: Some(AudioFormat::Flac),
                bit_depth: Some(24),
                sample_rate: Some(96000),
                channels: None,
                source: TrackApiSource::Local,
            },
        ];
        let result = display_album_version_qualities(qualities.into_iter(), None);
        assert_eq!(result, "FLAC 44.1 kHz 16-bit / FLAC 96 kHz 24-bit");
    }

    #[cfg(feature = "flac")]
    #[test]
    fn test_display_album_version_qualities_truncated() {
        let qualities = vec![
            AlbumVersionQuality {
                format: Some(AudioFormat::Flac),
                bit_depth: Some(16),
                sample_rate: Some(44100),
                channels: None,
                source: TrackApiSource::Local,
            },
            AlbumVersionQuality {
                format: Some(AudioFormat::Flac),
                bit_depth: Some(24),
                sample_rate: Some(96000),
                channels: None,
                source: TrackApiSource::Local,
            },
            AlbumVersionQuality {
                format: Some(AudioFormat::Flac),
                bit_depth: Some(24),
                sample_rate: Some(192_000),
                channels: None,
                source: TrackApiSource::Local,
            },
        ];
        let result = display_album_version_qualities(qualities.into_iter(), Some(30));
        assert_eq!(result, "FLAC 44.1 kHz 16-bit (+2)");
    }

    #[test]
    fn test_display_album_version_qualities_empty() {
        let qualities: Vec<AlbumVersionQuality> = vec![];
        let result = display_album_version_qualities(qualities.into_iter(), None);
        assert_eq!(result, "");
    }

    #[test]
    fn test_album_type_format_lp() {
        let album_type = AlbumType::Lp;
        assert_eq!(album_type.into_formatted(), "Albums");
    }

    #[test]
    fn test_album_type_format_download() {
        let album_type = AlbumType::Download;
        assert_eq!(album_type.into_formatted(), "Albums");
    }

    #[test]
    fn test_album_type_format_live() {
        let album_type = AlbumType::Live;
        assert_eq!(album_type.into_formatted(), "Live Albums");
    }

    #[test]
    fn test_album_type_format_compilations() {
        let album_type = AlbumType::Compilations;
        assert_eq!(album_type.into_formatted(), "Compilations");
    }

    #[test]
    fn test_album_type_format_eps_and_singles() {
        let album_type = AlbumType::EpsAndSingles;
        assert_eq!(album_type.into_formatted(), "EPs and Singles");
    }

    #[test]
    fn test_album_type_format_other() {
        let album_type = AlbumType::Other;
        assert_eq!(album_type.into_formatted(), "Other Albums");
    }

    #[test]
    fn test_format_date_string_valid() {
        let date_string = "2025-01-08";
        let result = format_date_string(date_string, "%B %d, %Y");
        assert_eq!(result, "January 08, 2025");
    }

    #[test]
    fn test_format_date_string_invalid() {
        let date_string = "invalid-date";
        let result = format_date_string(date_string, "%B %d, %Y");
        assert_eq!(result, "n/a");
    }

    #[test]
    fn test_format_date_string_different_format() {
        let date_string = "2025-01-08";
        let result = format_date_string(date_string, "%Y-%m-%d");
        assert_eq!(result, "2025-01-08");
    }

    #[test]
    fn test_format_size_bytes() {
        let size = 500;
        assert_eq!(format_size(size), "500 B");
    }

    #[test]
    fn test_format_size_kilobytes() {
        let size = 1500;
        let formatted = format_size(size);
        assert!(
            formatted.contains("1.") && (formatted.contains("KB") || formatted.contains("KiB"))
        );
    }

    #[test]
    fn test_format_size_megabytes() {
        let size = 1_500_000;
        let formatted = format_size(size);
        assert!(
            formatted.contains("1.") && (formatted.contains("MB") || formatted.contains("MiB"))
        );
    }

    #[test]
    fn test_format_size_gigabytes() {
        let size = 1_500_000_000;
        let formatted = format_size(size);
        assert!(
            formatted.contains("1.") && (formatted.contains("GB") || formatted.contains("GiB"))
        );
    }

    #[test]
    fn test_format_size_zero() {
        let size = 0;
        assert_eq!(format_size(size), "0 B");
    }

    #[test]
    fn test_classify_name_simple() {
        assert_eq!(classify_name("Test"), "test");
    }

    #[test]
    fn test_classify_name_with_spaces() {
        assert_eq!(classify_name("Test Name"), "test-name");
    }

    #[test]
    fn test_classify_name_with_special_chars() {
        assert_eq!(classify_name("Test@Name#123"), "test-name-123");
    }

    #[test]
    fn test_classify_name_with_hyphens() {
        assert_eq!(classify_name("Test-Name"), "test-name");
    }

    #[test]
    fn test_classify_name_empty() {
        assert_eq!(classify_name(""), "");
    }

    #[test]
    fn test_classify_name_already_lowercase() {
        assert_eq!(classify_name("test123"), "test123");
    }

    #[test]
    fn test_classify_name_multiple_special_chars() {
        assert_eq!(classify_name("Test!!!Name???"), "test---name---");
    }
}
