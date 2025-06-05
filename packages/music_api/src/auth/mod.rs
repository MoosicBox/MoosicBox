use std::{
    ops::{Deref, DerefMut},
    sync::{Arc, atomic::AtomicBool},
};

use crate::Error;

#[cfg(feature = "auth-poll")]
pub mod poll;

#[cfg(feature = "auth-username-password")]
pub mod username_password;

#[derive(Debug, Clone)]
pub enum Auth {
    #[cfg(feature = "auth-poll")]
    Poll(poll::PollAuth),
    #[cfg(feature = "auth-username-password")]
    UsernamePassword(username_password::UsernamePasswordAuth),
    None,
}

pub trait AuthExt {
    #[cfg(feature = "auth-poll")]
    fn as_poll(&self) -> Option<&poll::PollAuth>;
    #[cfg(feature = "auth-poll")]
    fn into_poll(self) -> Option<poll::PollAuth>;
    #[cfg(feature = "auth-username-password")]
    fn as_username_password(&self) -> Option<&username_password::UsernamePasswordAuth>;
    #[cfg(feature = "auth-username-password")]
    fn into_username_password(self) -> Option<username_password::UsernamePasswordAuth>;
}

impl AuthExt for Auth {
    #[cfg(feature = "auth-poll")]
    fn as_poll(&self) -> Option<&poll::PollAuth> {
        let Self::Poll(x) = self else {
            return None;
        };

        Some(x)
    }

    #[cfg(feature = "auth-poll")]
    fn into_poll(self) -> Option<poll::PollAuth> {
        let Self::Poll(x) = self else {
            return None;
        };

        Some(x)
    }

    #[cfg(feature = "auth-username-password")]
    fn as_username_password(&self) -> Option<&username_password::UsernamePasswordAuth> {
        let Self::UsernamePassword(x) = self else {
            return None;
        };

        Some(x)
    }

    #[cfg(feature = "auth-username-password")]
    fn into_username_password(self) -> Option<username_password::UsernamePasswordAuth> {
        let Self::UsernamePassword(x) = self else {
            return None;
        };

        Some(x)
    }
}

#[derive(Debug, Clone)]
pub struct ApiAuthBuilder {
    auth: Option<Auth>,
    logged_in: Option<bool>,
}

impl Default for ApiAuthBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiAuthBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            auth: None,
            logged_in: None,
        }
    }

    #[must_use]
    pub fn with_auth(mut self, auth: impl Into<Auth>) -> Self {
        self.auth = Some(auth.into());
        self
    }

    #[must_use]
    pub fn auth(&mut self, auth: impl Into<Auth>) -> &mut Self {
        self.auth = Some(auth.into());
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
    pub fn build(self) -> ApiAuth {
        let auth = self.auth.unwrap();

        ApiAuth::new(auth, self.logged_in.unwrap_or(false))
    }
}

#[derive(Debug, Clone)]
pub struct ApiAuth {
    logged_in: Arc<AtomicBool>,
    auth: Auth,
}

impl ApiAuth {
    #[must_use]
    pub fn new(auth: Auth, logged_in: bool) -> Self {
        Self {
            auth,
            logged_in: Arc::new(AtomicBool::new(logged_in)),
        }
    }

    #[must_use]
    pub const fn builder() -> ApiAuthBuilder {
        ApiAuthBuilder::new()
    }

    /// # Errors
    ///
    /// * If the authentication validation fails
    #[allow(clippy::unused_async)]
    pub async fn is_logged_in(&self) -> Result<bool, Error> {
        Ok(self.logged_in.load(std::sync::atomic::Ordering::SeqCst))
    }

    pub fn set_logged_in(&self, logged_in: bool) {
        self.logged_in
            .store(logged_in, std::sync::atomic::Ordering::SeqCst);
    }
}

impl AuthExt for ApiAuth {
    #[cfg(feature = "auth-poll")]
    fn as_poll(&self) -> Option<&poll::PollAuth> {
        self.auth.as_poll()
    }

    #[cfg(feature = "auth-poll")]
    fn into_poll(self) -> Option<poll::PollAuth> {
        self.auth.into_poll()
    }

    #[cfg(feature = "auth-username-password")]
    fn as_username_password(&self) -> Option<&username_password::UsernamePasswordAuth> {
        self.auth.as_username_password()
    }

    #[cfg(feature = "auth-username-password")]
    fn into_username_password(self) -> Option<username_password::UsernamePasswordAuth> {
        self.auth.into_username_password()
    }
}

impl Deref for ApiAuth {
    type Target = Auth;

    fn deref(&self) -> &Self::Target {
        &self.auth
    }
}

impl DerefMut for ApiAuth {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.auth
    }
}
