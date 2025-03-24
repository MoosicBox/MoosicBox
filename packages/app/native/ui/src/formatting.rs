use moosicbox_date_utils::chrono::{NaiveDateTime, parse_date_time};
use moosicbox_music_models::{
    AlbumType, AlbumVersionQuality, ApiSource, AudioFormat, TrackApiSource,
    api::ApiAlbumVersionQuality,
};
use rust_decimal::{Decimal, RoundingStrategy};
use rust_decimal_macros::dec;

pub trait TimeFormat {
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

pub trait ApiSourceFormat {
    fn into_formatted(self) -> String;
}

impl ApiSourceFormat for ApiSource {
    fn into_formatted(self) -> String {
        match self {
            Self::Library => "Library".to_string(),
            #[cfg(feature = "tidal")]
            Self::Tidal => "Tidal".to_string(),
            #[cfg(feature = "qobuz")]
            Self::Qobuz => "Qobuz".to_string(),
            #[cfg(feature = "yt")]
            Self::Yt => "YouTube Music".to_string(),
        }
    }
}

pub trait TrackApiSourceFormat {
    fn into_formatted(self) -> &'static str;
}

impl TrackApiSourceFormat for TrackApiSource {
    fn into_formatted(self) -> &'static str {
        match self {
            Self::Local => "Local",
            #[cfg(feature = "tidal")]
            Self::Tidal => "Tidal",
            #[cfg(feature = "qobuz")]
            Self::Qobuz => "Qobuz",
            #[cfg(feature = "yt")]
            Self::Yt => "YouTube Music",
        }
    }
}

pub trait AudioFormatFormat {
    fn into_formatted(self) -> &'static str;
}

impl AudioFormatFormat for AudioFormat {
    fn into_formatted(self) -> &'static str {
        match self {
            #[cfg(feature = "aac")]
            Self::Aac => "AAC",
            #[cfg(feature = "flac")]
            Self::Flac => "FLAC",
            #[cfg(feature = "mp3")]
            Self::Mp3 => "MP3",
            #[cfg(feature = "opus")]
            Self::Opus => "OPUS",
            Self::Source => "N/A",
        }
    }
}

pub trait AlbumVersionQualityFormat {
    fn into_formatted(self) -> String;
}

impl AlbumVersionQualityFormat for AlbumVersionQuality {
    fn into_formatted(self) -> String {
        match self.source {
            TrackApiSource::Local => {
                let mut formatted = self
                    .format
                    .expect("Missing format")
                    .into_formatted()
                    .to_string();

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
            #[cfg(feature = "tidal")]
            TrackApiSource::Tidal => self.source.into_formatted().to_string(),
            #[cfg(feature = "qobuz")]
            TrackApiSource::Qobuz => self.source.into_formatted().to_string(),
            #[cfg(feature = "yt")]
            TrackApiSource::Yt => self.source.into_formatted().to_string(),
        }
    }
}

impl AlbumVersionQualityFormat for ApiAlbumVersionQuality {
    fn into_formatted(self) -> String {
        let quality: AlbumVersionQuality = self.into();
        quality.into_formatted()
    }
}

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

pub trait AlbumTypeFormat {
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

#[must_use]
pub fn format_date_string(date_string: &str, format: &str) -> String {
    // January 08, 2025
    let Ok(date) = parse_date_time(date_string) else {
        return "n/a".to_string();
    };
    format_date(&date, format)
}

#[must_use]
pub fn format_date(date: &NaiveDateTime, format: &str) -> String {
    // January 08, 2025
    date.format(format).to_string()
}
