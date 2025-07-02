//! Data models for the test workspace

use core::core_function;

pub struct User {
    pub name: String,
    pub email: String,
}

pub fn create_user(name: String, email: String) -> User {
    println!("Using core: {}", core_function());
    User { name, email }
}

#[cfg(feature = "validation")]
pub mod validation {
    use super::User;

    pub fn validate_user(user: &User) -> bool {
        !user.name.is_empty() && user.email.contains('@')
    }
}
