#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "actix")]
pub mod actix;
#[cfg(feature = "reqwest")]
pub mod reqwest;

use std::num::NonZeroU16;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

#[derive(Debug, Clone, Copy, EnumString, AsRefStr)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "SCREAMING_SNAKE_CASE"))]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Connect,
    Trace,
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StatusCode(NonZeroU16);

impl From<NonZeroU16> for StatusCode {
    fn from(value: NonZeroU16) -> Self {
        Self(value)
    }
}

#[derive(Debug, thiserror::Error)]
pub struct TryFromU16StatusCodeError;

impl std::fmt::Display for TryFromU16StatusCodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TryFromU16StatusCodeError")
    }
}

impl TryFrom<u16> for StatusCode {
    type Error = TryFromU16StatusCodeError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Ok(Self(
            NonZeroU16::new(value).ok_or(TryFromU16StatusCodeError)?,
        ))
    }
}

impl StatusCode {
    #[must_use]
    pub const fn as_u16(&self) -> u16 {
        self.0.get()
    }
}

impl StatusCode {
    pub const OK: Self = Self(NonZeroU16::new(200).unwrap());
    pub const PARTIAL_CONTENT: Self = Self(NonZeroU16::new(206).unwrap());
    pub const TEMPORARY_REDIRECT: Self = Self(NonZeroU16::new(307).unwrap());
    pub const PERMANENT_REDIRECT: Self = Self(NonZeroU16::new(308).unwrap());
    pub const BAD_REQUEST: Self = Self(NonZeroU16::new(400).unwrap());
    pub const UNAUTHORIZED: Self = Self(NonZeroU16::new(401).unwrap());
    pub const NOT_FOUND: Self = Self(NonZeroU16::new(404).unwrap());
    pub const INTERNAL_SERVER_ERROR: Self = Self(NonZeroU16::new(500).unwrap());
}

impl StatusCode {
    /// Check if status is within 100-199.
    #[inline]
    #[must_use]
    pub fn is_informational(&self) -> bool {
        (100..200).contains(&self.0.get())
    }

    /// Check if status is within 200-299.
    #[inline]
    #[must_use]
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.0.get())
    }

    /// Check if status is within 300-399.
    #[inline]
    #[must_use]
    pub fn is_redirection(&self) -> bool {
        (300..400).contains(&self.0.get())
    }

    /// Check if status is within 400-499.
    #[inline]
    #[must_use]
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.0.get())
    }

    /// Check if status is within 500-599.
    #[inline]
    #[must_use]
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.0.get())
    }
}

impl From<StatusCode> for u16 {
    fn from(value: StatusCode) -> Self {
        value.0.get()
    }
}

impl std::fmt::Display for StatusCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.get() {
            401 => f.write_str("401 Unauthorized"),
            code => f.write_str(&code.to_string()),
        }
    }
}
