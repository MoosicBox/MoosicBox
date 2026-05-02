use base64::Engine as _;
use serde::{Serialize, de::DeserializeOwned};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PayloadFormat {
    BmuxCodecV1,
}

impl PayloadFormat {
    #[must_use]
    pub const fn as_i16(self) -> i16 {
        match self {
            Self::BmuxCodecV1 => 1,
        }
    }

    /// # Errors
    ///
    /// * [`PayloadError::UnsupportedPayloadFormat`] - If `value` is unknown
    pub const fn try_from_i16(value: i16) -> Result<Self, PayloadError> {
        match value {
            1 => Ok(Self::BmuxCodecV1),
            _ => Err(PayloadError::UnsupportedPayloadFormat(value)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PayloadStorage {
    Base64Text,
}

impl PayloadStorage {
    #[must_use]
    pub const fn as_i16(self) -> i16 {
        match self {
            Self::Base64Text => 1,
        }
    }

    /// # Errors
    ///
    /// * [`PayloadError::UnsupportedPayloadStorage`] - If `value` is unknown
    pub const fn try_from_i16(value: i16) -> Result<Self, PayloadError> {
        match value {
            1 => Ok(Self::Base64Text),
            _ => Err(PayloadError::UnsupportedPayloadStorage(value)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PayloadBlob {
    pub data: String,
    pub format: PayloadFormat,
    pub storage: PayloadStorage,
}

impl PayloadBlob {
    #[must_use]
    pub const fn new(data: String, format: PayloadFormat, storage: PayloadStorage) -> Self {
        Self {
            data,
            format,
            storage,
        }
    }

    /// # Errors
    ///
    /// * [`PayloadError::Serialize`] - If value encoding fails
    pub fn from_serializable<T: Serialize>(value: &T) -> Result<Self, PayloadError> {
        let bytes = bmux_codec::to_vec(value).map_err(PayloadError::Serialize)?;
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);

        Ok(Self {
            data,
            format: PayloadFormat::BmuxCodecV1,
            storage: PayloadStorage::Base64Text,
        })
    }

    /// # Errors
    ///
    /// * [`PayloadError::UnsupportedPayloadStorage`] - If storage mode is not supported
    /// * [`PayloadError::Deserialize`] - If value decoding fails
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, PayloadError> {
        let bytes = match self.storage {
            PayloadStorage::Base64Text => base64::engine::general_purpose::STANDARD
                .decode(self.data.as_bytes())
                .map_err(PayloadError::from)?,
        };

        match self.format {
            PayloadFormat::BmuxCodecV1 => {
                bmux_codec::from_bytes(&bytes).map_err(PayloadError::from)
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PayloadError {
    #[error("Payload serialization failed: {0}")]
    Serialize(#[source] bmux_codec::Error),
    #[error("Payload deserialization failed: {0}")]
    Deserialize(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("Unsupported payload format value: {0}")]
    UnsupportedPayloadFormat(i16),
    #[error("Unsupported payload storage value: {0}")]
    UnsupportedPayloadStorage(i16),
}

impl From<base64::DecodeError> for PayloadError {
    fn from(value: base64::DecodeError) -> Self {
        Self::Deserialize(Box::new(value))
    }
}

impl From<bmux_codec::Error> for PayloadError {
    fn from(value: bmux_codec::Error) -> Self {
        Self::Deserialize(Box::new(value))
    }
}
