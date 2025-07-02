//! Shared utilities for the test workspace

#[cfg(feature = "json")]
use serde::{Deserialize, Serialize};

pub fn format_response(message: &str) -> String {
    format!("[RESPONSE] {}", message)
}

#[cfg(feature = "json")]
#[derive(Serialize, Deserialize)]
pub struct ApiResponse {
    pub success: bool,
    pub message: String,
}

#[cfg(feature = "json")]
pub fn create_json_response(success: bool, message: String) -> ApiResponse {
    ApiResponse { success, message }
}

#[cfg(feature = "logging")]
pub fn log_info(message: &str) {
    println!("[INFO] {}", message);
}

pub fn hash_string(input: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    hasher.finish()
}
