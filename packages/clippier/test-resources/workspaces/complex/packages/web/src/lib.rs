//! Web frontend for the test workspace

use api::handle_user_creation;
use shared_utils::format_response;

pub async fn render_user_form() -> String {
    format_response("Rendering user creation form")
}

pub async fn handle_form_submission(name: String, email: String) -> String {
    handle_user_creation(name, email).await
}

#[cfg(feature = "frontend")]
pub mod frontend {
    pub fn build_assets() -> Result<(), Box<dyn std::error::Error>> {
        println!("Building frontend assets...");
        Ok(())
    }
}

#[cfg(feature = "ssr")]
pub mod ssr {
    pub fn render_server_side() -> String {
        "Server-side rendered content".to_string()
    }
}
