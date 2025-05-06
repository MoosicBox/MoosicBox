use gimbal_database::{AsId, DatabaseValue, Row};
use moosicbox_json_utils::{
    MissingValue, ParseError, ToValueType,
    database::{AsModel, AsModelResult, ToValue},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct YtConfig {
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

impl MissingValue<YtConfig> for &gimbal_database::Row {}
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
        DatabaseValue::Number(i64::from(self.id))
    }
}
