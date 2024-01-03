use moosicbox_core::sqlite::{
    db::SqliteValue,
    models::{AsId, AsModel},
};
use rusqlite::Row;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct TidalConfig {
    pub id: u32,
    pub access_token: String,
    pub refresh_token: String,
    pub client_name: String,
    pub expires_in: u32,
    pub issued_at: u64,
    pub scope: String,
    pub token_type: String,
    pub user: String,
    pub user_id: u32,
    pub created: String,
    pub updated: String,
}

impl AsModel<TidalConfig> for Row<'_> {
    fn as_model(&self) -> TidalConfig {
        TidalConfig {
            id: self.get("id").unwrap(),
            access_token: self.get("access_token").unwrap(),
            refresh_token: self.get("refresh_token").unwrap(),
            client_name: self.get("client_name").unwrap(),
            expires_in: self.get("expires_in").unwrap(),
            issued_at: self.get("issued_at").unwrap(),
            scope: self.get("scope").unwrap(),
            token_type: self.get("token_type").unwrap(),
            user: self.get("user").unwrap(),
            user_id: self.get("user_id").unwrap(),
            created: self.get("created").unwrap(),
            updated: self.get("updated").unwrap(),
        }
    }
}

impl AsId for TidalConfig {
    fn as_id(&self) -> SqliteValue {
        SqliteValue::Number(self.id as i64)
    }
}
