//! Database model types for Qobuz configuration persistence.
//!
//! Contains structs representing stored Qobuz authentication tokens,
//! application configuration, and user settings.

use moosicbox_json_utils::{
    ParseError, ToValueType,
    database::ToValue as _,
    serde_json::{ToNestedValue, ToValue},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use switchy::database::Row;

/// Qobuz app secret for a specific timezone, used for signing API requests.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzAppSecret {
    /// Database record identifier.
    pub id: u32,
    /// Timezone identifier (e.g., "berlin", "london").
    pub timezone: String,
    /// Secret key for request signing.
    pub secret: String,
    /// Timestamp when the record was created.
    pub created: String,
    /// Timestamp when the record was last updated.
    pub updated: String,
}

impl ToValueType<QobuzAppSecret> for &Row {
    fn to_value_type(self) -> Result<QobuzAppSecret, ParseError> {
        Ok(QobuzAppSecret {
            id: self.to_value("id")?,
            timezone: self.to_value("timezone")?,
            secret: self.to_value("secret")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl ToValueType<QobuzAppSecret> for &Value {
    fn to_value_type(self) -> Result<QobuzAppSecret, ParseError> {
        Ok(QobuzAppSecret {
            id: self.to_value("id")?,
            timezone: self.to_value("timezone")?,
            secret: self.to_value("secret")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

/// Qobuz application configuration extracted from the web bundle.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzAppConfig {
    /// Database record identifier.
    pub id: u32,
    /// Version of the Qobuz web bundle (e.g., "7.1.3-b011").
    pub bundle_version: String,
    /// Application ID for API requests.
    pub app_id: String,
    /// Timestamp when the record was created.
    pub created: String,
    /// Timestamp when the record was last updated.
    pub updated: String,
}

impl ToValueType<QobuzAppConfig> for &Row {
    fn to_value_type(self) -> Result<QobuzAppConfig, ParseError> {
        Ok(QobuzAppConfig {
            id: self.to_value("id")?,
            bundle_version: self.to_value("bundle_version")?,
            app_id: self.to_value("app_id")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl ToValueType<QobuzAppConfig> for &Value {
    fn to_value_type(self) -> Result<QobuzAppConfig, ParseError> {
        Ok(QobuzAppConfig {
            id: self.to_value("id")?,
            bundle_version: self.to_value("bundle_version")?,
            app_id: self.to_value("app_id")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

/// User authentication configuration for Qobuz.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzConfig {
    /// Database record identifier.
    pub id: u32,
    /// User access token for authenticated API requests.
    pub access_token: String,
    /// Unique user identifier.
    pub user_id: u64,
    /// User's email address.
    pub user_email: String,
    /// User's public identifier.
    pub user_public_id: String,
    /// Timestamp when the record was created.
    pub created: String,
    /// Timestamp when the record was last updated.
    pub updated: String,
}

impl ToValueType<QobuzConfig> for &Row {
    fn to_value_type(self) -> Result<QobuzConfig, ParseError> {
        Ok(QobuzConfig {
            id: self.to_value("id")?,
            access_token: self.to_value("access_token")?,
            user_id: self.to_value("user_id")?,
            user_email: self.to_value("user_email")?,
            user_public_id: self.to_value("user_public_id")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl ToValueType<QobuzConfig> for &Value {
    fn to_value_type(self) -> Result<QobuzConfig, ParseError> {
        Ok(QobuzConfig {
            id: self.to_value("id")?,
            access_token: self.to_value("user_auth_token")?,
            user_id: self.to_nested_value(&["user", "id"])?,
            user_email: self.to_nested_value(&["user", "email"])?,
            user_public_id: self.to_nested_value(&["user", "publicId"])?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}
