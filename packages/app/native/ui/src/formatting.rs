use moosicbox_music_models::{AlbumType, ApiSource};

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
