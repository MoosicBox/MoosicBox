//! CLI tool for the test workspace

use api::handle_user_creation;
use models::User;
use shared_utils::format_response;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("CLI Tool Starting...");

    let result = handle_user_creation(
        "test_user".to_string(),
        "test@example.com".to_string()
    ).await;

    println!("{}", format_response(&result));

    Ok(())
}

#[cfg(feature = "interactive")]
pub fn interactive_mode() {
    println!("Entering interactive mode...");
}

#[cfg(feature = "batch")]
pub async fn batch_mode() {
    println!("Running in batch mode...");
}
