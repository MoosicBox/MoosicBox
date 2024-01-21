use moosicbox_core::sqlite::{
    db::SqliteValue,
    models::{AsId, AsModel, AsModelResult},
};
use moosicbox_json_utils::{
    rusqlite::ToValue as RusqliteToValue,
    serde_json::{ToNestedValue, ToValue},
    ParseError, ToValueType,
};
use rusqlite::Row;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzAppSecret {
    pub id: u32,
    pub timezone: String,
    pub secret: String,
    pub created: String,
    pub updated: String,
}

impl AsModel<QobuzAppSecret> for Row<'_> {
    fn as_model(&self) -> QobuzAppSecret {
        AsModelResult::as_model(self)
            .unwrap_or_else(|e| panic!("Failed to get QobuzAppSecret: {e:?}"))
    }
}

impl AsModelResult<QobuzAppSecret, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<QobuzAppSecret, ParseError> {
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

    fn missing_value(self, error: ParseError) -> Result<QobuzAppSecret, ParseError> {
        Err(error)
    }
}

impl AsId for QobuzAppSecret {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzAppConfig {
    pub id: u32,
    pub bundle_version: String,
    pub app_id: String,
    pub created: String,
    pub updated: String,
}

impl AsModel<QobuzAppConfig> for Row<'_> {
    fn as_model(&self) -> QobuzAppConfig {
        AsModelResult::as_model(self)
            .unwrap_or_else(|e| panic!("Failed to get QobuzAppConfig: {e:?}"))
    }
}

impl AsModelResult<QobuzAppConfig, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<QobuzAppConfig, ParseError> {
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

    fn missing_value(self, error: ParseError) -> Result<QobuzAppConfig, ParseError> {
        Err(error)
    }
}

impl AsId for QobuzAppConfig {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzConfig {
    pub id: u32,
    pub access_token: String,
    pub user_id: u64,
    pub user_email: String,
    pub user_public_id: String,
    pub created: String,
    pub updated: String,
}

impl AsModel<QobuzConfig> for Row<'_> {
    fn as_model(&self) -> QobuzConfig {
        AsModelResult::as_model(self).unwrap_or_else(|e| panic!("Failed to get QobuzConfig: {e:?}"))
    }
}

impl AsModelResult<QobuzConfig, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<QobuzConfig, ParseError> {
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

    fn missing_value(self, error: ParseError) -> Result<QobuzConfig, ParseError> {
        Err(error)
    }
}

impl AsId for QobuzConfig {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}
