use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid packet structure: {0}")]
    InvalidPacket(String),

    #[error("Unsupported configuration: {0}")]
    Unsupported(String),

    #[error("Decoder initialization failed: {0}")]
    InitFailed(String),

    #[error("Decode operation failed: {0}")]
    DecodeFailed(String),

    #[error("Range decoder error: {0}")]
    RangeDecoder(String),

    #[error("SILK decoder error: {0}")]
    SilkDecoder(String),

    #[error("CELT decoder error: {0}")]
    CeltDecoder(String),

    #[error("Invalid sample rate: {0}")]
    InvalidSampleRate(String),

    #[error("Invalid resampler delay: {0}")]
    InvalidDelay(String),

    #[error("Unsupported mode: {0}")]
    UnsupportedMode(String),

    #[error("Invalid mode: {0}")]
    InvalidMode(String),
}
