use std::{
    ops::{Deref, DerefMut},
    sync::{Arc, atomic::AtomicBool},
};

use async_trait::async_trait;

#[cfg(feature = "auth-poll")]
pub mod poll;

#[cfg(feature = "auth-username-password")]
pub mod username_password;

#[async_trait]
pub trait Auth {
    async fn login(&self) -> Result<bool, Box<dyn std::error::Error>>;
}

#[derive(Debug, Clone)]
pub struct ApiAuthBuilder<T: Auth> {
    auth: Option<T>,
    logged_in: Option<bool>,
}

impl<T: Auth> Default for ApiAuthBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Auth> ApiAuthBuilder<T> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            auth: None,
            logged_in: None,
        }
    }

    #[must_use]
    pub fn with_auth(mut self, auth: T) -> Self {
        self.auth = Some(auth);
        self
    }

    #[must_use]
    pub fn auth(&mut self, auth: T) -> &mut Self {
        self.auth = Some(auth);
        self
    }

    #[must_use]
    pub const fn with_logged_in(mut self, logged_in: bool) -> Self {
        self.logged_in = Some(logged_in);
        self
    }

    /// # Panics
    ///
    /// * If `auth` is `None`
    #[must_use]
    pub fn build(self) -> ApiAuth<T> {
        let auth = self.auth.unwrap();

        ApiAuth::new(auth, self.logged_in.unwrap_or(false))
    }
}

#[derive(Debug, Clone)]
pub struct ApiAuth<T: Auth> {
    logged_in: Arc<AtomicBool>,
    auth: T,
}

impl<T: Auth> ApiAuth<T> {
    #[must_use]
    pub fn new(auth: T, logged_in: bool) -> Self {
        Self {
            auth,
            logged_in: Arc::new(AtomicBool::new(logged_in)),
        }
    }

    #[must_use]
    pub const fn builder() -> ApiAuthBuilder<T> {
        ApiAuthBuilder::new()
    }

    #[must_use]
    pub fn is_logged_in(&self) -> bool {
        self.logged_in.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn set_logged_in(&self, logged_in: bool) {
        self.logged_in
            .store(logged_in, std::sync::atomic::Ordering::SeqCst);
    }
}

impl<T: Auth> Deref for ApiAuth<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.auth
    }
}

impl<T: Auth> DerefMut for ApiAuth<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.auth
    }
}
