pub struct AppState {
    pub service_port: u16,
    pub proxy_url: String,
    pub proxy_client: awc::Client,
}
