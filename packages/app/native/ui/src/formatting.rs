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

    mod time_format {
        use super::*;

        #[test_log::test]
        fn test_u64_formats_seconds_only() {
            assert_eq!(0_u64.into_formatted(), "0:00");
            assert_eq!(5_u64.into_formatted(), "0:05");
            assert_eq!(59_u64.into_formatted(), "0:59");
        }

        #[test_log::test]
        fn test_u64_formats_minutes_and_seconds() {
            assert_eq!(60_u64.into_formatted(), "1:00");
            assert_eq!(61_u64.into_formatted(), "1:01");
            assert_eq!(125_u64.into_formatted(), "2:05");
            assert_eq!(599_u64.into_formatted(), "9:59");
        }

        #[test_log::test]
        fn test_u64_formats_hours_minutes_seconds() {
            assert_eq!(3600_u64.into_formatted(), "1:60:00");
            assert_eq!(3661_u64.into_formatted(), "1:61:01");
            assert_eq!(7325_u64.into_formatted(), "2:122:05");
        }

        #[test_log::test]
        fn test_f64_rounds_to_nearest_integer() {
            assert_eq!(0.4_f64.into_formatted(), "0:00");
            assert_eq!(0.5_f64.into_formatted(), "0:01");
            assert_eq!(59.4_f64.into_formatted(), "0:59");
            assert_eq!(59.5_f64.into_formatted(), "1:00");
        }

        #[test_log::test]
        fn test_f32_rounds_to_nearest_integer() {
            assert_eq!(0.4_f32.into_formatted(), "0:00");
            assert_eq!(0.5_f32.into_formatted(), "0:01");
            assert_eq!(65.7_f32.into_formatted(), "1:06");
        }
    }

    mod classify_name_tests {
        use super::*;

        #[test_log::test]
        fn test_converts_to_lowercase() {
            assert_eq!(classify_name("LIBRARY"), "library");
            assert_eq!(classify_name("MySource"), "mysource");
        }

        #[test_log::test]
        fn test_replaces_spaces_with_hyphens() {
            assert_eq!(classify_name("my source"), "my-source");
            assert_eq!(classify_name("some long name"), "some-long-name");
        }

        #[test_log::test]
        fn test_replaces_special_characters_with_hyphens() {
            assert_eq!(classify_name("source@api"), "source-api");
            assert_eq!(classify_name("api.source"), "api-source");
            assert_eq!(classify_name("test_name"), "test-name");
        }

        #[test_log::test]
        fn test_preserves_alphanumeric() {
            assert_eq!(classify_name("api123"), "api123");
            assert_eq!(classify_name("source2go"), "source2go");
        }

        #[test_log::test]
        fn test_empty_string() {
            assert_eq!(classify_name(""), "");
        }
    }

    mod format_date_string_tests {
        use super::*;

        #[test_log::test]
        fn test_formats_valid_date() {
            let result = format_date_string("2025-01-08", "%B %d, %Y");
            assert_eq!(result, "January 08, 2025");
        }

        #[test_log::test]
        fn test_formats_year_only() {
            let result = format_date_string("2025-06-15", "%Y");
            assert_eq!(result, "2025");
        }

        #[test_log::test]
        fn test_invalid_date_returns_na() {
            assert_eq!(format_date_string("invalid", "%Y-%m-%d"), "n/a");
            assert_eq!(format_date_string("", "%Y"), "n/a");
        }
    }

    mod format_size_tests {
        use super::*;

        #[test_log::test]
        fn test_formats_bytes() {
            let result = format_size(500);
            assert_eq!(result, "500 B");
        }

        #[test_log::test]
        fn test_formats_kibibytes() {
            let result = format_size(1024);
            assert_eq!(result, "1.0 KiB");
        }

        #[test_log::test]
        fn test_formats_mebibytes() {
            let result = format_size(1024 * 1024);
            assert_eq!(result, "1.0 MiB");
        }

        #[test_log::test]
        fn test_formats_gibibytes() {
            let result = format_size(1024 * 1024 * 1024);
            assert_eq!(result, "1.0 GiB");
        }
    }

    mod album_type_format_tests {
        use super::*;

        #[test_log::test]
        fn test_lp_formats_to_albums() {
            assert_eq!(AlbumType::Lp.into_formatted(), "Albums");
        }

        #[test_log::test]
        fn test_download_formats_to_albums() {
            assert_eq!(AlbumType::Download.into_formatted(), "Albums");
        }

        #[test_log::test]
        fn test_live_formats_to_live_albums() {
            assert_eq!(AlbumType::Live.into_formatted(), "Live Albums");
        }

        #[test_log::test]
        fn test_compilations_formats_correctly() {
            assert_eq!(AlbumType::Compilations.into_formatted(), "Compilations");
        }

        #[test_log::test]
        fn test_eps_and_singles_formats_correctly() {
            assert_eq!(AlbumType::EpsAndSingles.into_formatted(), "EPs and Singles");
        }

        #[test_log::test]
        fn test_other_formats_to_other_albums() {
            assert_eq!(AlbumType::Other.into_formatted(), "Other Albums");
        }
    }

    mod display_album_version_qualities_tests {
        use super::*;
        use moosicbox_music_models::ApiSource;

        // Helper struct that implements AlbumVersionQualityFormat for testing
        struct MockQuality(String);

        impl AlbumVersionQualityFormat for MockQuality {
            fn into_formatted(self) -> String {
                self.0
            }
        }

        fn mock(s: &str) -> MockQuality {
            MockQuality(s.to_string())
        }

        #[test_log::test]
        fn test_empty_iterator_returns_empty_string() {
            let qualities: Vec<MockQuality> = vec![];
            let result = display_album_version_qualities(qualities.into_iter(), None);
            assert_eq!(result, "");
        }

        #[test_log::test]
        fn test_single_item_no_separator() {
            let qualities = vec![mock("FLAC 44.1 kHz")];
            let result = display_album_version_qualities(qualities.into_iter(), None);
            assert_eq!(result, "FLAC 44.1 kHz");
        }

        #[test_log::test]
        fn test_two_items_joined_with_separator() {
            let qualities = vec![mock("FLAC"), mock("MP3")];
            let result = display_album_version_qualities(qualities.into_iter(), None);
            assert_eq!(result, "FLAC / MP3");
        }

        #[test_log::test]
        fn test_three_items_joined_with_separator() {
            let qualities = vec![mock("FLAC"), mock("MP3"), mock("AAC")];
            let result = display_album_version_qualities(qualities.into_iter(), None);
            assert_eq!(result, "FLAC / MP3 / AAC");
        }

        #[test_log::test]
        fn test_no_truncation_when_max_characters_none() {
            let qualities = vec![
                mock("FLAC 96 kHz 24-bit"),
                mock("FLAC 48 kHz 16-bit"),
                mock("MP3 320kbps"),
            ];
            let result = display_album_version_qualities(qualities.into_iter(), None);
            assert_eq!(
                result,
                "FLAC 96 kHz 24-bit / FLAC 48 kHz 16-bit / MP3 320kbps"
            );
        }

        #[test_log::test]
        fn test_truncates_when_exceeding_max_characters() {
            // "FLAC" = 4 chars, " / " = 3 chars, "MP3" = 3 chars
            // Total for two = 4 + 3 + 3 = 10
            // If max is 9, should truncate at second item
            let qualities = vec![mock("FLAC"), mock("MP3"), mock("AAC")];
            let result = display_album_version_qualities(qualities.into_iter(), Some(9));
            // After "FLAC", adding " / MP3" would be 10 chars which exceeds 9
            // So we truncate with "(+2)" for the remaining 2 items
            assert_eq!(result, "FLAC (+2)");
        }

        #[test_log::test]
        fn test_truncation_shows_correct_remaining_count() {
            let qualities = vec![mock("A"), mock("B"), mock("C"), mock("D"), mock("E")];
            // "A" = 1 char, adding " / B" = 4 more = 5 total
            // With max_characters = 3, we can only fit "A"
            let result = display_album_version_qualities(qualities.into_iter(), Some(3));
            // Remaining: B, C, D, E = 4 items
            assert_eq!(result, "A (+4)");
        }

        #[test_log::test]
        fn test_truncation_at_last_item() {
            let qualities = vec![mock("AAA"), mock("BBB")];
            // "AAA" = 3 chars, " / BBB" = 6 more = 9 total
            // With max_characters = 8, truncates at second item
            let result = display_album_version_qualities(qualities.into_iter(), Some(8));
            // Only 1 remaining item (BBB)
            assert_eq!(result, "AAA (+1)");
        }

        #[test_log::test]
        fn test_fits_exactly_at_max_characters() {
            let qualities = vec![mock("AB"), mock("CD")];
            // "AB" = 2 chars, " / CD" = 5 more = 7 total
            let result = display_album_version_qualities(qualities.into_iter(), Some(7));
            assert_eq!(result, "AB / CD");
        }

        #[test_log::test]
        fn test_with_real_album_version_quality_api_source() {
            // Test with actual AlbumVersionQuality using API source
            let qualities = vec![
                AlbumVersionQuality {
                    source: TrackApiSource::Api(ApiSource::library()),
                    ..Default::default()
                },
                AlbumVersionQuality {
                    source: TrackApiSource::Api(ApiSource::library()),
                    ..Default::default()
                },
            ];
            let result = display_album_version_qualities(qualities.into_iter(), None);
            // API sources format as "API:Library"
            assert_eq!(result, "API:Library / API:Library");
        }

        #[test_log::test]
        fn test_large_max_characters_no_truncation() {
            let qualities = vec![mock("Short"), mock("Items")];
            let result = display_album_version_qualities(qualities.into_iter(), Some(1000));
            assert_eq!(result, "Short / Items");
        }

        #[test_log::test]
        fn test_max_characters_zero_truncates_immediately() {
            let qualities = vec![mock("A"), mock("B")];
            // With max 0, adding anything to first item should trigger truncation
            let result = display_album_version_qualities(qualities.into_iter(), Some(0));
            // First item is always added, then second would exceed limit
            assert_eq!(result, "A (+1)");
        }
    }

    mod album_version_quality_format_local_tests {
        use super::*;

        #[cfg(feature = "flac")]
        #[test_log::test]
        fn test_local_source_with_format_sample_rate_and_bit_depth() {
            let quality = AlbumVersionQuality {
                source: TrackApiSource::Local,
                format: Some(AudioFormat::Flac),
                sample_rate: Some(44_100),
                bit_depth: Some(16),
                channels: None,
            };
            let result = quality.into_formatted();
            assert_eq!(result, "FLAC 44.1 kHz 16-bit");
        }

        #[cfg(feature = "flac")]
        #[test_log::test]
        fn test_local_source_with_format_only() {
            let quality = AlbumVersionQuality {
                source: TrackApiSource::Local,
                format: Some(AudioFormat::Flac),
                sample_rate: None,
                bit_depth: None,
                channels: None,
            };
            let result = quality.into_formatted();
            assert_eq!(result, "FLAC");
        }

        #[cfg(feature = "flac")]
        #[test_log::test]
        fn test_local_source_with_format_and_sample_rate() {
            let quality = AlbumVersionQuality {
                source: TrackApiSource::Local,
                format: Some(AudioFormat::Flac),
                sample_rate: Some(96_000),
                bit_depth: None,
                channels: None,
            };
            let result = quality.into_formatted();
            assert_eq!(result, "FLAC 96 kHz");
        }

        #[cfg(feature = "flac")]
        #[test_log::test]
        fn test_local_source_with_format_and_bit_depth() {
            let quality = AlbumVersionQuality {
                source: TrackApiSource::Local,
                format: Some(AudioFormat::Flac),
                sample_rate: None,
                bit_depth: Some(24),
                channels: None,
            };
            let result = quality.into_formatted();
            assert_eq!(result, "FLAC 24-bit");
        }

        #[cfg(feature = "flac")]
        #[test_log::test]
        fn test_local_source_sample_rate_decimal_normalization() {
            // 48000 Hz = 48 kHz (should normalize to "48" not "48.00")
            let quality = AlbumVersionQuality {
                source: TrackApiSource::Local,
                format: Some(AudioFormat::Flac),
                sample_rate: Some(48_000),
                bit_depth: None,
                channels: None,
            };
            let result = quality.into_formatted();
            assert_eq!(result, "FLAC 48 kHz");
        }

        #[cfg(feature = "flac")]
        #[test_log::test]
        fn test_local_source_high_resolution_sample_rate() {
            // 192000 Hz = 192 kHz
            let quality = AlbumVersionQuality {
                source: TrackApiSource::Local,
                format: Some(AudioFormat::Flac),
                sample_rate: Some(192_000),
                bit_depth: Some(32),
                channels: None,
            };
            let result = quality.into_formatted();
            assert_eq!(result, "FLAC 192 kHz 32-bit");
        }

        #[cfg(feature = "flac")]
        #[test_log::test]
        fn test_local_source_non_standard_sample_rate() {
            // 88200 Hz = 88.2 kHz
            let quality = AlbumVersionQuality {
                source: TrackApiSource::Local,
                format: Some(AudioFormat::Flac),
                sample_rate: Some(88_200),
                bit_depth: None,
                channels: None,
            };
            let result = quality.into_formatted();
            assert_eq!(result, "FLAC 88.2 kHz");
        }

        #[test_log::test]
        fn test_api_source_formats_as_source_name() {
            let quality = AlbumVersionQuality {
                source: TrackApiSource::Api(ApiSource::library()),
                format: None,
                sample_rate: None,
                bit_depth: None,
                channels: None,
            };
            let result = quality.into_formatted();
            // API sources delegate to the source's format, not the audio format
            assert_eq!(result, "API:Library");
        }
    }

    mod audio_format_format_tests {
        use super::*;

        #[cfg(feature = "flac")]
        #[test_log::test]
        fn test_flac_format() {
            assert_eq!(AudioFormat::Flac.into_formatted(), "FLAC");
        }

        #[cfg(feature = "mp3")]
        #[test_log::test]
        fn test_mp3_format() {
            assert_eq!(AudioFormat::Mp3.into_formatted(), "MP3");
        }

        #[cfg(feature = "aac")]
        #[test_log::test]
        fn test_aac_format() {
            assert_eq!(AudioFormat::Aac.into_formatted(), "AAC");
        }

        #[cfg(feature = "opus")]
        #[test_log::test]
        fn test_opus_format() {
            assert_eq!(AudioFormat::Opus.into_formatted(), "OPUS");
        }

        #[test_log::test]
        fn test_source_format() {
            assert_eq!(AudioFormat::Source.into_formatted(), "N/A");
        }
    }
}
