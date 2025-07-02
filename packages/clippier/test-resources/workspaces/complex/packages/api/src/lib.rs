//! API layer for the test workspace

use models::{create_user, User};
use shared_utils::format_response;

pub async fn handle_user_creation(name: String, email: String) -> String {
    let user = create_user(name, email);
    format_response(&format!("Created user: {}", user.name))
}

#[cfg(feature = "server")]
pub mod server {
    pub async fn start_server() -> Result<(), Box<dyn std::error::Error>> {
        println!("Starting API server...");
        Ok(())
    }
}
