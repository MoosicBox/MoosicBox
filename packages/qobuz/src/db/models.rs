use moosicbox_database::Row;
use moosicbox_json_utils::{
    ParseError, ToValueType,
    database::ToValue as _,
    serde_json::{ToNestedValue, ToValue},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzAppSecret {
    pub id: u32,
    pub timezone: String,
    pub secret: String,
    pub created: String,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzAppConfig {
    pub id: u32,
    pub bundle_version: String,
    pub app_id: String,
    pub created: String,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
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
