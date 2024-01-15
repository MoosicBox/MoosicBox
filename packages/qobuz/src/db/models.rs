use moosicbox_core::sqlite::{
    db::SqliteValue,
    models::{AsId, AsModel, AsModelResult},
};
use moosicbox_json_utils::{rusqlite::ToValue, ParseError};
use rusqlite::Row;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzConfig {
    pub id: u32,
    pub access_token: String,
    pub refresh_token: String,
    pub client_name: String,
    pub expires_in: u32,
    pub issued_at: u64,
    pub scope: String,
    pub token_type: String,
    pub user: String,
    pub user_id: u64,
    pub app_id: String,
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
            refresh_token: self.to_value("refresh_token")?,
            client_name: self.to_value("client_name")?,
            expires_in: self.to_value("expires_in")?,
            issued_at: self.to_value("issued_at")?,
            scope: self.to_value("scope")?,
            token_type: self.to_value("token_type")?,
            user: self.to_value("user")?,
            user_id: self.to_value("user_id")?,
            app_id: self.to_value("user_id")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsId for QobuzConfig {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}
