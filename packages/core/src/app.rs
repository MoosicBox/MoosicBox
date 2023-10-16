use std::fmt;

use sqlite::Connection;

pub struct AppState {
    pub service_port: u16,
    pub proxy_url: String,
    pub proxy_client: awc::Client,
    pub image_client: awc::Client,
    pub db: Option<Db>,
}

pub struct Db {
    pub library: Connection,
}

impl fmt::Debug for Db {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Db")
            .field("library", &"{{db connection}}")
            .finish()
    }
}
