//! Data models for Tidal database persistence.
//!
//! This module defines the database schema models for storing Tidal OAuth
//! configuration and authentication tokens.

use moosicbox_json_utils::{
    MissingValue, ParseError, ToValueType,
    database::{AsModel, AsModelResult, ToValue},
};
use serde::{Deserialize, Serialize};
use switchy::database::{AsId, DatabaseValue, Row};

/// Tidal OAuth configuration stored in the database.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalConfig {
    /// Database record ID.
    pub id: u32,
    /// Tidal OAuth client ID.
    pub client_id: String,
    /// OAuth access token for API authentication.
    pub access_token: String,
    /// OAuth refresh token for obtaining new access tokens.
    pub refresh_token: String,
    /// Client name associated with this OAuth application.
    pub client_name: String,
    /// Token expiration time in seconds.
    pub expires_in: u32,
    /// Unix timestamp when the token was issued.
    pub issued_at: u64,
    /// OAuth scope permissions (e.g., "`r_usr` `w_usr` `w_sub`").
    pub scope: String,
    /// OAuth token type (e.g., "`Bearer`").
    pub token_type: String,
    /// Serialized user information as JSON.
    pub user: String,
    /// Tidal user ID.
    pub user_id: u64,
    /// Database record creation timestamp.
    pub created: String,
    /// Database record last update timestamp.
    pub updated: String,
}

impl MissingValue<TidalConfig> for &switchy::database::Row {
    fn missing_value(&self, error: ParseError) -> Result<TidalConfig, ParseError> {
        Err(error)
    }
}
impl ToValueType<TidalConfig> for &Row {
    fn to_value_type(self) -> Result<TidalConfig, ParseError> {
        Ok(TidalConfig {
            id: self.to_value("id")?,
            client_id: self.to_value("client_id")?,
            access_token: self.to_value("access_token")?,
            refresh_token: self.to_value("refresh_token")?,
            client_name: self.to_value("client_name")?,
            expires_in: self.to_value("expires_in")?,
            issued_at: self.to_value("issued_at")?,
            scope: self.to_value("scope")?,
            token_type: self.to_value("token_type")?,
            user: self.to_value("user")?,
            user_id: self.to_value("user_id")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsModelResult<TidalConfig, ParseError> for Row {
    fn as_model(&self) -> Result<TidalConfig, ParseError> {
        self.to_value_type()
    }
}

impl AsModel<TidalConfig> for Row {
    fn as_model(&self) -> TidalConfig {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsId for TidalConfig {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Int64(i64::from(self.id))
    }
}
