//! Database models for `YouTube` Music configuration.
//!
//! Contains the [`YtConfig`] type for persisting OAuth credentials and user settings.

use moosicbox_json_utils::{
    MissingValue, ParseError, ToValueType,
    database::{AsModel, AsModelResult, ToValue},
};
use serde::{Deserialize, Serialize};
use switchy_database::{AsId, DatabaseValue, Row};

/// `YouTube` Music API configuration stored in the database.
///
/// Contains OAuth tokens and user information for authenticating with `YouTube` Music.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtConfig {
    /// Database record ID
    pub id: u32,
    /// `YouTube` Music client ID
    pub client_id: String,
    /// OAuth access token
    pub access_token: String,
    /// OAuth refresh token
    pub refresh_token: String,
    /// Client application name
    pub client_name: String,
    /// Token expiration time in seconds
    pub expires_in: u32,
    /// Unix timestamp when the token was issued
    pub issued_at: u64,
    /// OAuth scope permissions
    pub scope: String,
    /// Token type (e.g., "Bearer")
    pub token_type: String,
    /// JSON-encoded user information
    pub user: String,
    /// `YouTube` Music user ID
    pub user_id: u64,
    /// Timestamp when the record was created
    pub created: String,
    /// Timestamp when the record was last updated
    pub updated: String,
}

impl MissingValue<YtConfig> for &switchy_database::Row {}
impl ToValueType<YtConfig> for &Row {
    fn to_value_type(self) -> Result<YtConfig, ParseError> {
        Ok(YtConfig {
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

impl AsModelResult<YtConfig, ParseError> for Row {
    fn as_model(&self) -> Result<YtConfig, ParseError> {
        self.to_value_type()
    }
}

impl AsModel<YtConfig> for Row {
    fn as_model(&self) -> YtConfig {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsId for YtConfig {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Int64(i64::from(self.id))
    }
}
