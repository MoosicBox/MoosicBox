use std::{collections::HashMap, str::from_utf8, sync::Mutex};

use aws_config::BehaviorVersion;
use aws_sdk_ssm::{config::Region, Client};
use mysql::{
    prelude::{FromRow, Queryable},
    FromRowError, Row,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Connection {
    pub tunnel_ws_id: String,
    pub client_id: Option<String>,
    pub created: String,
    pub updated: String,
}

impl FromRow for Connection {
    fn from_row_opt(row: Row) -> std::result::Result<Self, FromRowError>
    where
        Self: Sized,
    {
        Ok(Connection {
            tunnel_ws_id: get_value_str(get_column_value(&row, "tunnel_ws_id")).into(),
            client_id: get_value_str_opt(get_column_value(&row, "client_id")).map(|s| s.into()),
            created: get_value_str(get_column_value(&row, "created")).into(),
            updated: get_value_str(get_column_value(&row, "updated")).into(),
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

fn get_value_str_opt(value: &mysql::Value) -> Option<&str> {
    match value {
        mysql::Value::NULL => None,
        mysql::Value::Bytes(bytes) => {
            Some(from_utf8(bytes).expect("Failed to decode bytes to string"))
        }
        _ => unreachable!(),
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
            ON DUPLICATE KEY UPDATE `tunnel_ws_id` = ?, `updated` = (DATE_FORMAT(NOW(), '%Y-%m-%dT%H:%i:%f'))",
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
