//! Database model types for library configuration and authentication.
//!
//! This module defines the data structures used to represent library configuration
//! and authentication credentials stored in the database.

use moosicbox_json_utils::{
    MissingValue, ParseError, ToValueType,
    database::{AsModel, AsModelResult, ToValue},
};
use serde::{Deserialize, Serialize};
use switchy_database::{AsId, DatabaseValue, Row};

/// Library authentication configuration for external music APIs.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct LibraryConfig {
    /// Configuration ID.
    pub id: u32,
    /// Client ID for API authentication.
    pub client_id: String,
    /// Access token for API requests.
    pub access_token: String,
    /// Refresh token for obtaining new access tokens.
    pub refresh_token: String,
    /// Client application name.
    pub client_name: String,
    /// Token expiration duration in seconds.
    pub expires_in: u32,
    /// Timestamp when the token was issued.
    pub issued_at: u64,
    /// OAuth scope granted to the token.
    pub scope: String,
    /// Type of token (e.g., "Bearer").
    pub token_type: String,
    /// Username.
    pub user: String,
    /// User ID.
    pub user_id: u64,
    /// Timestamp when the configuration was created.
    pub created: String,
    /// Timestamp when the configuration was last updated.
    pub updated: String,
}

impl MissingValue<LibraryConfig> for &switchy_database::Row {}
impl ToValueType<LibraryConfig> for &Row {
    fn to_value_type(self) -> Result<LibraryConfig, ParseError> {
        Ok(LibraryConfig {
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

impl AsModelResult<LibraryConfig, ParseError> for Row {
    fn as_model(&self) -> Result<LibraryConfig, ParseError> {
        self.to_value_type()
    }
}

impl AsModel<LibraryConfig> for Row {
    fn as_model(&self) -> LibraryConfig {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsId for LibraryConfig {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Int64(i64::from(self.id))
    }
}
