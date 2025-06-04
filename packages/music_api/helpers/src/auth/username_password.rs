use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use super::Auth;

#[derive(Debug, Clone)]
pub struct UsernamePasswordAuth {
    username: Arc<RwLock<String>>,
    password: Arc<RwLock<String>>,
}

impl Default for UsernamePasswordAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl UsernamePasswordAuth {
    #[must_use]
    pub fn new() -> Self {
        Self {
            username: Arc::new(RwLock::new(String::new())),
            password: Arc::new(RwLock::new(String::new())),
        }
    }

    /// Sets the username.
    ///
    /// # Panics
    ///
    /// * If the `username` `RwLock` is poisoned.
    #[must_use]
    pub fn username(&self, username: impl Into<String>) -> &Self {
        *self.username.write().unwrap() = username.into();
        self
    }

    /// Sets the password.
    ///
    /// # Panics
    ///
    /// * If the `password` `RwLock` is poisoned.
    #[must_use]
    pub fn password(&self, password: impl Into<String>) -> &Self {
        *self.password.write().unwrap() = password.into();
        self
    }
}

#[async_trait]
impl Auth for UsernamePasswordAuth {
    async fn login(&self) -> Result<bool, Box<dyn std::error::Error>> {
        let (_username, _password) = {
            let username = self.username.read().unwrap();
            let password = self.password.read().unwrap();

            (username.clone(), password.clone())
        };

        Ok(false)
    }
}
