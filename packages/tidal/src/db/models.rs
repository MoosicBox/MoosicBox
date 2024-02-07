use moosicbox_core::sqlite::{
    db::SqliteValue,
    models::{AsId, AsModel, AsModelResult},
};
use moosicbox_json_utils::{rusqlite::ToValue, ParseError, ToValueType};
use rusqlite::Row;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalConfig {
    pub id: u32,
    pub client_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub client_name: String,
    pub expires_in: u32,
    pub issued_at: u64,
    pub scope: String,
    pub token_type: String,
    pub user: String,
    pub user_id: u64,
    pub created: String,
    pub updated: String,
}

impl ToValueType<TidalConfig> for &Row<'_> {
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

impl AsModelResult<TidalConfig, ParseError> for Row<'_> {
    fn as_model(&self) -> Result<TidalConfig, ParseError> {
        self.to_value_type()
    }
}

impl AsModel<TidalConfig> for Row<'_> {
    fn as_model(&self) -> TidalConfig {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsId for TidalConfig {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}
