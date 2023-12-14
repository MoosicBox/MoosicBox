use std::{
    fmt,
    sync::{Arc, Mutex},
};

use rusqlite::Connection;

pub struct AppState {
    pub tunnel_host: Option<String>,
    pub service_port: u16,
    pub db: Option<Db>,
}

#[derive(Clone, Debug)]
pub struct Db {
    pub library: Arc<Mutex<DbConnection>>,
}

pub struct DbConnection {
    pub inner: Connection,
}

impl From<Connection> for DbConnection {
    fn from(value: Connection) -> Self {
        DbConnection { inner: value }
    }
}

impl fmt::Debug for DbConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DbConnection")
    }
}
