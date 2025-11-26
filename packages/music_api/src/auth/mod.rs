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

#[cfg(test)]
mod test {
    use super::{ApiAuth, Auth};

    #[test_log::test(switchy_async::test)]
    async fn api_auth_builder_builds_with_no_auth() {
        let auth = ApiAuth::builder().without_auth().build();

        assert!(matches!(*auth, Auth::None));
    }

    #[test_log::test(switchy_async::test)]
    async fn api_auth_builder_sets_logged_in_state() {
        let auth = ApiAuth::builder()
            .without_auth()
            .with_logged_in(true)
            .build();

        let is_logged_in = auth.is_logged_in().await.unwrap();
        assert!(is_logged_in);
    }

    #[test_log::test(switchy_async::test)]
    async fn api_auth_set_logged_in_updates_state() {
        let auth = ApiAuth::builder()
            .without_auth()
            .with_logged_in(false)
            .build();

        assert!(!auth.is_logged_in().await.unwrap());

        auth.set_logged_in(true);
        assert!(auth.is_logged_in().await.unwrap());

        auth.set_logged_in(false);
        assert!(!auth.is_logged_in().await.unwrap());
    }

    #[test_log::test(switchy_async::test)]
    async fn api_auth_validate_credentials_returns_false_when_no_validator() {
        let auth = ApiAuth::builder().without_auth().build();

        let result = auth.validate_credentials().await.unwrap();
        assert!(!result);
    }

    #[test_log::test(switchy_async::test)]
    async fn api_auth_validate_credentials_calls_validator_and_updates_state() {
        let auth = ApiAuth::builder()
            .without_auth()
            .with_validate_credentials(|| async { Ok(true) })
            .build();

        assert!(!auth.is_logged_in().await.unwrap());

        auth.validate_credentials().await.unwrap();

        assert!(auth.is_logged_in().await.unwrap());
    }

    #[test_log::test(switchy_async::test)]
    async fn api_auth_validate_credentials_sets_logged_out_on_error() {
        let auth = ApiAuth::builder()
            .without_auth()
            .with_logged_in(true)
            .with_validate_credentials(|| async {
                Err(Box::new(std::io::Error::other("validation failed"))
                    as Box<dyn std::error::Error + Send>)
            })
            .build();

        assert!(auth.is_logged_in().await.unwrap());

        let result = auth.validate_credentials().await;
        assert!(result.is_err());
        assert!(!auth.is_logged_in().await.unwrap());
    }

    #[test_log::test(switchy_async::test)]
    async fn api_auth_attempt_login_updates_logged_in_state_on_success() {
        let auth = ApiAuth::builder().without_auth().build();

        let result = auth.attempt_login(|_| async { Ok(true) }).await.unwrap();

        assert!(result);
        assert!(auth.is_logged_in().await.unwrap());
    }

    #[test_log::test(switchy_async::test)]
    async fn api_auth_attempt_login_sets_logged_out_on_failure() {
        let auth = ApiAuth::builder()
            .without_auth()
            .with_logged_in(true)
            .build();

        let result = auth.attempt_login(|_| async { Ok(false) }).await.unwrap();

        assert!(!result);
        assert!(!auth.is_logged_in().await.unwrap());
    }

    #[test_log::test(switchy_async::test)]
    async fn api_auth_attempt_login_propagates_error() {
        let auth = ApiAuth::builder().without_auth().build();

        let result = auth
            .attempt_login(|_| async {
                Err(Box::new(std::io::Error::other("login failed"))
                    as Box<dyn std::error::Error + Send>)
            })
            .await;

        assert!(result.is_err());
    }

    #[test_log::test]
    fn auth_from_option_none_converts_to_auth_none() {
        let auth: Auth = None::<Auth>.into();
        assert!(matches!(auth, Auth::None));
    }

    #[cfg(feature = "auth-poll")]
    #[test_log::test]
    fn auth_as_poll_returns_some_for_poll_variant() {
        use super::poll::PollAuth;

        let poll = PollAuth::new();
        let auth = Auth::Poll(poll);

        assert!(auth.as_poll().is_some());
    }

    #[cfg(feature = "auth-poll")]
    #[test_log::test]
    fn auth_as_poll_returns_none_for_other_variants() {
        let auth = Auth::None;
        assert!(auth.as_poll().is_none());
    }

    #[cfg(feature = "auth-username-password")]
    #[test_log::test]
    fn auth_as_username_password_returns_some_for_username_password_variant() {
        use super::username_password::UsernamePasswordAuth;

        let up_auth = UsernamePasswordAuth::builder()
            .with_handler(|_u, _p| async { Ok(true) })
            .build()
            .unwrap();
        let auth = Auth::UsernamePassword(up_auth);

        assert!(auth.as_username_password().is_some());
    }

    #[cfg(feature = "auth-username-password")]
    #[test_log::test]
    fn auth_as_username_password_returns_none_for_other_variants() {
        let auth = Auth::None;
        assert!(auth.as_username_password().is_none());
    }

    #[cfg(feature = "auth-poll")]
    #[test_log::test]
    fn auth_into_poll_returns_some_for_poll_variant() {
        use super::poll::PollAuth;

        let poll = PollAuth::new();
        let auth = Auth::Poll(poll);

        assert!(auth.into_poll().is_some());
    }

    #[cfg(feature = "auth-poll")]
    #[test_log::test]
    fn auth_into_poll_returns_none_for_other_variants() {
        let auth = Auth::None;
        assert!(auth.into_poll().is_none());
    }

    #[cfg(feature = "auth-username-password")]
    #[test_log::test]
    fn auth_into_username_password_returns_some_for_username_password_variant() {
        use super::username_password::UsernamePasswordAuth;

        let up_auth = UsernamePasswordAuth::builder()
            .with_handler(|_u, _p| async { Ok(true) })
            .build()
            .unwrap();
        let auth = Auth::UsernamePassword(up_auth);

        assert!(auth.into_username_password().is_some());
    }

    #[cfg(feature = "auth-username-password")]
    #[test_log::test]
    fn auth_into_username_password_returns_none_for_other_variants() {
        let auth = Auth::None;
        assert!(auth.into_username_password().is_none());
    }

    #[cfg(feature = "auth-poll")]
    #[test_log::test]
    fn api_auth_into_poll_returns_some_for_poll_variant() {
        use super::poll::PollAuth;

        let poll = PollAuth::new();
        let api_auth = ApiAuth::builder().with_auth(poll).build();

        assert!(api_auth.into_poll().is_some());
    }

    #[cfg(feature = "auth-poll")]
    #[test_log::test]
    fn api_auth_into_poll_returns_none_for_other_variants() {
        let api_auth = ApiAuth::builder().without_auth().build();
        assert!(api_auth.into_poll().is_none());
    }

    #[cfg(feature = "auth-username-password")]
    #[test_log::test]
    fn api_auth_into_username_password_returns_some_for_username_password_variant() {
        use super::username_password::UsernamePasswordAuth;

        let up_auth = UsernamePasswordAuth::builder()
            .with_handler(|_u, _p| async { Ok(true) })
            .build()
            .unwrap();
        let api_auth = ApiAuth::builder().with_auth(up_auth).build();

        assert!(api_auth.into_username_password().is_some());
    }

    #[cfg(feature = "auth-username-password")]
    #[test_log::test]
    fn api_auth_into_username_password_returns_none_for_other_variants() {
        let api_auth = ApiAuth::builder().without_auth().build();
        assert!(api_auth.into_username_password().is_none());
    }

    #[cfg(feature = "auth-poll")]
    #[test_log::test]
    fn api_auth_as_poll_returns_some_for_poll_variant() {
        use super::poll::PollAuth;

        let poll = PollAuth::new();
        let api_auth = ApiAuth::builder().with_auth(poll).build();

        assert!(api_auth.as_poll().is_some());
    }

    #[cfg(feature = "auth-poll")]
    #[test_log::test]
    fn api_auth_as_poll_returns_none_for_other_variants() {
        let api_auth = ApiAuth::builder().without_auth().build();
        assert!(api_auth.as_poll().is_none());
    }

    #[cfg(feature = "auth-username-password")]
    #[test_log::test]
    fn api_auth_as_username_password_returns_some_for_username_password_variant() {
        use super::username_password::UsernamePasswordAuth;

        let up_auth = UsernamePasswordAuth::builder()
            .with_handler(|_u, _p| async { Ok(true) })
            .build()
            .unwrap();
        let api_auth = ApiAuth::builder().with_auth(up_auth).build();

        assert!(api_auth.as_username_password().is_some());
    }

    #[cfg(feature = "auth-username-password")]
    #[test_log::test]
    fn api_auth_as_username_password_returns_none_for_other_variants() {
        let api_auth = ApiAuth::builder().without_auth().build();
        assert!(api_auth.as_username_password().is_none());
    }
}
