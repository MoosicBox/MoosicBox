#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::branches_sharing_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Connection {
    pub name: String,
    pub api_url: String,
}

impl AsRef<Self> for Connection {
    fn as_ref(&self) -> &Self {
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MusicApiSettings {
    pub id: String,
    pub name: String,
    pub logged_in: bool,
    pub authentication_enabled: bool,
    pub run_scan_endpoint: Option<String>,
    pub auth_endpoint: Option<String>,
}

impl AsRef<Self> for MusicApiSettings {
    fn as_ref(&self) -> &Self {
        self
    }
}
