use std::sync::{Arc, Mutex};

use moosicbox_database::{Database, DbConnection};

pub struct AppState {
    pub tunnel_host: Option<String>,
    pub service_port: u16,
    pub db: Option<Db>,
    pub database: Arc<Box<dyn Database>>,
}

#[derive(Clone, Debug)]
pub struct Db {
    pub library: Arc<Mutex<DbConnection>>,
}
