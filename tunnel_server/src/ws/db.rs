use std::{collections::HashMap, str::from_utf8, sync::Mutex};

use aws_config::BehaviorVersion;
use aws_sdk_ssm::{config::Region, Client};
use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use mysql::{
    prelude::{FromRow, Queryable},
    FromRowError, Row,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Connection {
    pub client_id: String,
    pub tunnel_ws_id: String,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
}

impl FromRow for Connection {
    fn from_row_opt(row: Row) -> std::result::Result<Self, FromRowError>
    where
        Self: Sized,
    {
        Ok(Connection {
            client_id: get_value_str(get_column_value(&row, "client_id")).into(),
            tunnel_ws_id: get_value_str(get_column_value(&row, "tunnel_ws_id")).into(),
            created: get_value_datetime(get_column_value(&row, "created")),
            updated: get_value_datetime(get_column_value(&row, "updated")),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SignatureToken {
    pub token_hash: String,
    pub client_id: String,
    pub expires: NaiveDateTime,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
}

impl FromRow for SignatureToken {
    fn from_row_opt(row: Row) -> std::result::Result<Self, FromRowError>
    where
        Self: Sized,
    {
        Ok(SignatureToken {
            token_hash: get_value_str(get_column_value(&row, "token_hash")).into(),
            client_id: get_value_str(get_column_value(&row, "client_id")).into(),
            expires: get_value_datetime(get_column_value(&row, "expires")),
            created: get_value_datetime(get_column_value(&row, "created")),
            updated: get_value_datetime(get_column_value(&row, "updated")),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClientAccessToken {
    pub token_hash: String,
    pub client_id: String,
    pub expires: Option<NaiveDateTime>,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
}

impl FromRow for ClientAccessToken {
    fn from_row_opt(row: Row) -> std::result::Result<Self, FromRowError>
    where
        Self: Sized,
    {
        Ok(ClientAccessToken {
            token_hash: get_value_str(get_column_value(&row, "token_hash")).into(),
            client_id: get_value_str(get_column_value(&row, "client_id")).into(),
            expires: get_value_datetime_opt(get_column_value(&row, "expires")),
            created: get_value_datetime(get_column_value(&row, "created")),
            updated: get_value_datetime(get_column_value(&row, "updated")),
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MagicToken {
    pub magic_token_hash: String,
    pub client_id: String,
    pub expires: Option<NaiveDateTime>,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
}

impl FromRow for MagicToken {
    fn from_row_opt(row: Row) -> std::result::Result<Self, FromRowError>
    where
        Self: Sized,
    {
        Ok(MagicToken {
            magic_token_hash: get_value_str(get_column_value(&row, "magic_token_hash")).into(),
            client_id: get_value_str(get_column_value(&row, "client_id")).into(),
            expires: get_value_datetime_opt(get_column_value(&row, "expires")),
            created: get_value_datetime(get_column_value(&row, "created")),
            updated: get_value_datetime(get_column_value(&row, "updated")),
        })
    }
}

fn get_column_value<'a>(row: &'a Row, name: &'a str) -> &'a mysql::Value {
    return &row[row
        .columns_ref()
        .iter()
        .find(|c| c.org_name_str() == name)
        .unwrap()
        .name_str()
        .as_ref()];
}

fn get_value_str(value: &mysql::Value) -> &str {
    match value {
        mysql::Value::Bytes(bytes) => from_utf8(bytes).expect("Failed to decode bytes to string"),
        _ => unreachable!(),
    }
}

#[allow(dead_code)]
fn get_value_str_opt(value: &mysql::Value) -> Option<&str> {
    match value {
        mysql::Value::NULL => None,
        mysql::Value::Bytes(bytes) => {
            Some(from_utf8(bytes).expect("Failed to decode bytes to string"))
        }
        _ => unreachable!(),
    }
}

fn get_value_datetime(value: &mysql::Value) -> NaiveDateTime {
    match value {
        mysql::Value::Date(year, month, day, hour, minutes, seconds, micro_seconds) => {
            let date = NaiveDate::from_ymd_opt(*year as i32, *month as u32, *day as u32).unwrap();
            let time = NaiveTime::from_hms_micro_opt(
                *hour as u32,
                *minutes as u32,
                *seconds as u32,
                *micro_seconds,
            )
            .unwrap();
            NaiveDateTime::new(date, time)
        }
        _ => unreachable!(),
    }
}

fn get_value_datetime_opt(value: &mysql::Value) -> Option<NaiveDateTime> {
    match value {
        mysql::Value::NULL => None,
        _ => Some(get_value_datetime(value)),
    }
}

static DB: Lazy<Mutex<Option<mysql::Conn>>> = Lazy::new(|| Mutex::new(None));

pub async fn init() {
    let config = aws_config::defaults(BehaviorVersion::v2023_11_09())
        .region(Region::new("us-east-1"))
        .load()
        .await;

    let client = Client::new(&config);

    let params = match client
        .get_parameters()
        .set_with_decryption(Some(true))
        .names("moosicbox_db_name")
        .names("moosicbox_db_hostname")
        .names("moosicbox_db_password")
        .names("moosicbox_db_user")
        .send()
        .await
    {
        Ok(params) => params,
        Err(err) => panic!("Failed to get parameters {err:?}"),
    };

    let params = params.parameters.expect("Failed to get params");
    let params: HashMap<&str, &str> = params
        .iter()
        .map(|param| (param.name().unwrap(), param.value().unwrap()))
        .collect();

    let ssl_opts = mysql::SslOpts::default();
    let opts = mysql::OptsBuilder::new()
        .ssl_opts(ssl_opts)
        .db_name(params.get("moosicbox_db_name").cloned())
        .ip_or_hostname(params.get("moosicbox_db_hostname").cloned())
        .pass(params.get("moosicbox_db_password").cloned())
        .user(params.get("moosicbox_db_user").cloned());

    DB.lock()
        .unwrap_or_else(|e| e.into_inner())
        .replace(mysql::Conn::new(opts).unwrap());
}

pub fn upsert_connection(client_id: &str, tunnel_ws_id: &str) {
    DB.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .expect("DB not initialized")
        .exec_drop(
            "
            INSERT INTO `connections` (client_id, tunnel_ws_id) VALUES(?, ?)
            ON DUPLICATE KEY UPDATE `tunnel_ws_id` = ?, `updated` = NOW()",
            (client_id, tunnel_ws_id, tunnel_ws_id),
        )
        .unwrap();
}

pub fn select_connection(client_id: &str) -> Option<Connection> {
    DB.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .expect("DB not initialized")
        .exec_first(
            "SELECT * FROM connections WHERE client_id = ?",
            (client_id,),
        )
        .unwrap()
}

pub fn delete_connection(tunnel_ws_id: &str) {
    DB.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .expect("DB not initialized")
        .exec_drop(
            "DELETE FROM `connections` WHERE tunnel_ws_id = ?",
            (tunnel_ws_id,),
        )
        .unwrap();
}

pub fn insert_client_access_token(client_id: &str, token_hash: &str) {
    DB.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .expect("DB not initialized")
        .exec_drop(
            "
            INSERT INTO `client_access_tokens` (token_hash, client_id, expires)
            VALUES(?, ?, NULL)",
            (token_hash, client_id),
        )
        .unwrap();
}

pub fn valid_client_access_token(client_id: &str, token_hash: &str) -> bool {
    select_client_access_token(client_id, token_hash).is_some()
}

pub fn select_client_access_token(client_id: &str, token_hash: &str) -> Option<ClientAccessToken> {
    DB.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .expect("DB not initialized")
        .exec_first(
            "
            SELECT * FROM client_access_tokens
                WHERE client_id = ?
                    AND token_hash = ?
                    AND (expires IS NULL OR expires >= NOW())",
            (client_id, token_hash),
        )
        .unwrap()
}

pub fn insert_magic_token(client_id: &str, token_hash: &str) {
    DB.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .expect("DB not initialized")
        .exec_drop(
            "
            INSERT INTO `magic_tokens` (magic_token_hash, client_id, expires)
            VALUES(?, ?, NULL)",
            (token_hash, client_id),
        )
        .unwrap();
}

pub fn select_magic_token(token_hash: &str) -> Option<MagicToken> {
    DB.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .expect("DB not initialized")
        .exec_first(
            "SELECT * FROM magic_tokens WHERE magic_token_hash = ? AND (expires IS NULL OR expires >= NOW())",
            (token_hash,),
        )
        .unwrap()
}

pub fn insert_signature_token(client_id: &str, token_hash: &str) {
    DB.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .expect("DB not initialized")
        .exec_drop(
            "
            INSERT INTO `signature_tokens` (token_hash, client_id, expires)
            VALUES(?, ?, DATE_ADD(NOW(), INTERVAL 14 DAY))",
            (token_hash, client_id),
        )
        .unwrap();
}

pub fn valid_signature_token(client_id: &str, token_hash: &str) -> bool {
    select_signature_token(client_id, token_hash).is_some()
}

pub fn select_signature_token(client_id: &str, token_hash: &str) -> Option<SignatureToken> {
    DB.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .expect("DB not initialized")
        .exec_first(
            "SELECT * FROM signature_tokens WHERE client_id=? AND token_hash = ? AND expires >= NOW()",
            (client_id, token_hash,),
        )
        .unwrap()
}

#[allow(dead_code)]
pub fn select_signature_tokens(client_id: &str) -> Vec<SignatureToken> {
    DB.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .expect("DB not initialized")
        .exec(
            "SELECT * FROM signature_tokens WHERE client_id = ?",
            (client_id,),
        )
        .unwrap()
}

#[allow(dead_code)]
pub fn delete_signature_token(token_hash: &str) {
    DB.lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_mut()
        .expect("DB not initialized")
        .exec_drop(
            "DELETE FROM `signature_tokens` WHERE token_hash = ?",
            (token_hash,),
        )
        .unwrap();
}
