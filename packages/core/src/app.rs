use std::sync::{Arc, Mutex};

use moosicbox_database::DbConnection;

pub struct AppState {
    pub tunnel_host: Option<String>,
    pub service_port: u16,
    pub db: Option<Db>,
}

#[derive(Clone, Debug)]
pub struct Db {
    pub library: Arc<Mutex<DbConnection>>,
}
