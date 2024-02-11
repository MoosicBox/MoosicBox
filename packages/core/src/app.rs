use std::sync::Arc;

use moosicbox_database::Database;

pub struct AppState {
    pub tunnel_host: Option<String>,
    pub service_port: u16,
    pub database: Arc<Box<dyn Database>>,
}
