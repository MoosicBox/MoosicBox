//! Authentication types and handlers for music APIs.
//!
//! This module provides authentication configurations for different auth methods:
//! * Poll-based authentication (requires `auth-poll` feature)
//! * Username/password authentication (requires `auth-username-password` feature)
//!
//! The [`ApiAuth`] type manages authentication state and credentials validation.

use std::{
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::{Arc, atomic::AtomicBool},
};

use crate::Error;

/// Poll-based authentication implementation.
#[cfg(feature = "auth-poll")]
pub mod poll;

/// Username and password authentication implementation.
#[cfg(feature = "auth-username-password")]
pub mod username_password;

/// Authentication configuration for a music API.
#[derive(Debug, Clone)]
pub enum Auth {
    /// Poll-based authentication.
    #[cfg(feature = "auth-poll")]
    Poll(poll::PollAuth),
    /// Username and password authentication.
    #[cfg(feature = "auth-username-password")]
    UsernamePassword(username_password::UsernamePasswordAuth),
    /// No authentication.
    None,
}

impl<T> From<Option<T>> for Auth
where
    T: Into<Self>,
{
    /// Converts an `Option<T>` into `Auth`, using `Auth::None` if the option is `None`.
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::None, Into::into)
    }
}

/// Extension trait for accessing specific authentication types.
pub trait AuthExt {
    /// Returns a reference to poll authentication if applicable.
    #[cfg(feature = "auth-poll")]
    fn as_poll(&self) -> Option<&poll::PollAuth>;
    /// Consumes self and returns poll authentication if applicable.
    #[cfg(feature = "auth-poll")]
    fn into_poll(self) -> Option<poll::PollAuth>;
    /// Returns a reference to username/password authentication if applicable.
    #[cfg(feature = "auth-username-password")]
    fn as_username_password(&self) -> Option<&username_password::UsernamePasswordAuth>;
    /// Consumes self and returns username/password authentication if applicable.
    #[cfg(feature = "auth-username-password")]
    fn into_username_password(self) -> Option<username_password::UsernamePasswordAuth>;
}

impl Auth {
    /// Returns a reference to poll authentication if applicable.
    #[cfg(feature = "auth-poll")]
    #[must_use]
    pub fn as_poll(&self) -> Option<&poll::PollAuth> {
        <Self as AuthExt>::as_poll(self)
    }

    /// Consumes self and returns poll authentication if applicable.
    #[cfg(feature = "auth-poll")]
    #[must_use]
    pub fn into_poll(self) -> Option<poll::PollAuth> {
        <Self as AuthExt>::into_poll(self)
    }

    /// Returns a reference to username/password authentication if applicable.
    #[cfg(feature = "auth-username-password")]
    #[must_use]
    pub fn as_username_password(&self) -> Option<&username_password::UsernamePasswordAuth> {
        <Self as AuthExt>::as_username_password(self)
    }

    /// Consumes self and returns username/password authentication if applicable.
    #[cfg(feature = "auth-username-password")]
    #[must_use]
    pub fn into_username_password(self) -> Option<username_password::UsernamePasswordAuth> {
        <Self as AuthExt>::into_username_password(self)
    }
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

/// Builder for constructing `ApiAuth` instances.
#[derive(Clone)]
pub struct ApiAuthBuilder {
    auth: Option<Auth>,
    logged_in: Option<bool>,
    validate_credentials: Option<
        Arc<
            dyn Fn() -> Pin<
                    Box<
                        dyn Future<Output = Result<bool, Box<dyn std::error::Error + Send>>> + Send,
                    >,
                > + Send
                + Sync,
        >,
    >,
}

impl std::fmt::Debug for ApiAuthBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiAuthBuilder")
            .field("auth", &self.auth)
            .field("logged_in", &self.logged_in)
            .finish_non_exhaustive()
    }
}

impl Default for ApiAuthBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ApiAuthBuilder {
    /// Creates a new builder.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            auth: None,
            logged_in: None,
            validate_credentials: None,
        }
    }

    /// Configures the builder to use no authentication.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn without_auth(mut self) -> Self {
        self.auth = Some(Auth::None);
        self
    }

    /// Sets the authentication configuration.
    #[must_use]
    pub fn with_auth(mut self, auth: impl Into<Auth>) -> Self {
        self.auth = Some(auth.into());
        self
    }

    /// Sets the authentication configuration (mutable version).
    #[must_use]
    pub fn auth(&mut self, auth: impl Into<Auth>) -> &mut Self {
        self.auth = Some(auth.into());
        self
    }

    /// Sets the initial logged-in state.
    #[must_use]
    pub const fn with_logged_in(mut self, logged_in: bool) -> Self {
        self.logged_in = Some(logged_in);
        self
    }

    /// Sets a function to validate credentials.
    #[must_use]
    pub fn with_validate_credentials<
        Fut: Future<Output = Result<bool, Box<dyn std::error::Error + Send>>> + Send + 'static,
        Func: Fn() -> Fut + Send + Sync + 'static,
    >(
        mut self,
        validate_credentials: Func,
    ) -> Self {
        self.validate_credentials = Some(Arc::new(move || Box::pin(validate_credentials())));
        self
    }

    /// Builds the `ApiAuth` instance.
    ///
    /// # Panics
    ///
    /// * If `auth` was not configured
    #[must_use]
    pub fn build(self) -> ApiAuth {
        let auth = self.auth.unwrap();
        let logged_in = Arc::new(AtomicBool::new(self.logged_in.unwrap_or(false)));

        ApiAuth {
            logged_in,
            auth,
            validate_credentials: self.validate_credentials,
        }
    }
}

/// Authentication handler for a music API.
#[derive(Clone)]
pub struct ApiAuth {
    logged_in: Arc<AtomicBool>,
    auth: Auth,
    validate_credentials: Option<
        Arc<
            dyn Fn() -> Pin<
                    Box<
                        dyn Future<Output = Result<bool, Box<dyn std::error::Error + Send>>> + Send,
                    >,
                > + Send
                + Sync,
        >,
    >,
}

impl std::fmt::Debug for ApiAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiAuth")
            .field("logged_in", &self.logged_in)
            .field("auth", &self.auth)
            .finish_non_exhaustive()
    }
}

impl ApiAuth {
    /// Creates a new builder for `ApiAuth`.
    #[must_use]
    pub const fn builder() -> ApiAuthBuilder {
        ApiAuthBuilder::new()
    }

    /// Returns whether the user is currently logged in.
    ///
    /// # Errors
    ///
    /// * If the authentication status check fails
    #[allow(clippy::unused_async)]
    pub async fn is_logged_in(&self) -> Result<bool, Error> {
        Ok(self.logged_in.load(std::sync::atomic::Ordering::SeqCst))
    }

    /// Sets the logged-in state.
    pub fn set_logged_in(&self, logged_in: bool) {
        self.logged_in
            .store(logged_in, std::sync::atomic::Ordering::SeqCst);
    }

    /// Validates the configured credentials.
    ///
    /// # Errors
    ///
    /// * If credential validation fails
    pub async fn validate_credentials(&self) -> Result<bool, Box<dyn std::error::Error + Send>> {
        if let Some(validate_credentials) = &self.validate_credentials {
            match validate_credentials().await {
                Ok(valid) => self.set_logged_in(valid),
                Err(e) => {
                    self.set_logged_in(false);
                    return Err(e);
                }
            }
        }

        Ok(false)
    }

    /// Attempts to log in using the provided function.
    ///
    /// # Errors
    ///
    /// * If the login attempt fails
    pub async fn attempt_login<
        Fut: Future<Output = Result<bool, Box<dyn std::error::Error + Send>>> + Send + 'static,
        Func: Fn(&Auth) -> Fut + Send + Sync + 'static,
    >(
        &self,
        func: Func,
    ) -> Result<bool, Box<dyn std::error::Error + Send>> {
        let logged_in = func(&self.auth).await?;

        self.logged_in
            .store(logged_in, std::sync::atomic::Ordering::SeqCst);

        Ok(logged_in)
    }

    /// Returns a reference to poll authentication if applicable.
    #[cfg(feature = "auth-poll")]
    #[must_use]
    pub fn as_poll(&self) -> Option<&poll::PollAuth> {
        <Self as AuthExt>::as_poll(self)
    }

    /// Consumes self and returns poll authentication if applicable.
    #[cfg(feature = "auth-poll")]
    #[must_use]
    pub fn into_poll(self) -> Option<poll::PollAuth> {
        <Self as AuthExt>::into_poll(self)
    }

    /// Returns a reference to username/password authentication if applicable.
    #[cfg(feature = "auth-username-password")]
    #[must_use]
    pub fn as_username_password(&self) -> Option<&username_password::UsernamePasswordAuth> {
        <Self as AuthExt>::as_username_password(self)
    }

    /// Consumes self and returns username/password authentication if applicable.
    #[cfg(feature = "auth-username-password")]
    #[must_use]
    pub fn into_username_password(self) -> Option<username_password::UsernamePasswordAuth> {
        <Self as AuthExt>::into_username_password(self)
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

    /// Returns a reference to the inner `Auth`.
    fn deref(&self) -> &Self::Target {
        &self.auth
    }
}

impl DerefMut for ApiAuth {
    /// Returns a mutable reference to the inner `Auth`.
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.auth
    }
}
