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
    sync::{Arc, RwLock, atomic::AtomicBool},
};

use crate::Error;

/// Observable authentication lifecycle for a music source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AuthState {
    /// No stored source configuration exists.
    #[default]
    NotConfigured,
    /// User interaction is required to authenticate.
    AuthenticationRequired,
    /// Stored or newly supplied credentials are being checked.
    Validating,
    /// Credentials are accepted.
    Authenticated,
    /// Previously accepted credentials have expired.
    Expired,
    /// Authentication failed for another actionable reason.
    Failed {
        /// User-presentable failure description.
        message: String,
    },
}

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
    configured: Option<bool>,
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
            configured: None,
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

    /// Sets whether stored source configuration exists.
    #[must_use]
    pub const fn with_configured(mut self, configured: bool) -> Self {
        self.configured = Some(configured);
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
        let logged_in = self.logged_in.unwrap_or(false);
        let state = if logged_in {
            AuthState::Authenticated
        } else if self.configured.unwrap_or(false) {
            AuthState::AuthenticationRequired
        } else {
            AuthState::NotConfigured
        };
        let logged_in = Arc::new(AtomicBool::new(logged_in));

        ApiAuth {
            logged_in,
            state: Arc::new(RwLock::new(state)),
            auth,
            validate_credentials: self.validate_credentials,
        }
    }
}

/// Authentication handler for a music API.
#[derive(Clone)]
pub struct ApiAuth {
    logged_in: Arc<AtomicBool>,
    state: Arc<RwLock<AuthState>>,
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
            .field("state", &self.state)
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

    /// Returns the observable source authentication state.
    ///
    /// # Panics
    ///
    /// * If the authentication state lock is poisoned
    #[must_use]
    pub fn state(&self) -> AuthState {
        self.state.read().unwrap().clone()
    }

    /// Replaces the observable source authentication state.
    ///
    /// # Panics
    ///
    /// * If the authentication state lock is poisoned
    pub fn set_state(&self, state: AuthState) {
        self.set_logged_in(matches!(state, AuthState::Authenticated));
        *self.state.write().unwrap() = state;
    }

    /// Sets the logged-in state.
    ///
    /// # Panics
    ///
    /// * If the authentication state lock is poisoned
    pub fn set_logged_in(&self, logged_in: bool) {
        self.logged_in
            .store(logged_in, std::sync::atomic::Ordering::SeqCst);
        *self.state.write().unwrap() = if logged_in {
            AuthState::Authenticated
        } else {
            AuthState::AuthenticationRequired
        };
    }

    /// Validates the configured credentials.
    ///
    /// # Errors
    ///
    /// * If credential validation fails
    pub async fn validate_credentials(&self) -> Result<bool, Box<dyn std::error::Error + Send>> {
        if let Some(validate_credentials) = &self.validate_credentials {
            self.set_state(AuthState::Validating);
            match validate_credentials().await {
                Ok(valid) => self.set_logged_in(valid),
                Err(e) => {
                    self.set_state(AuthState::Failed {
                        message: e.to_string(),
                    });
                    return Err(e);
                }
            }
        }

        Ok(self.logged_in.load(std::sync::atomic::Ordering::SeqCst))
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
        self.set_state(AuthState::Validating);
        match func(&self.auth).await {
            Ok(logged_in) => {
                self.set_logged_in(logged_in);
                Ok(logged_in)
            }
            Err(error) => {
                self.set_state(AuthState::Failed {
                    message: error.to_string(),
                });
                Err(error)
            }
        }
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
    use super::{ApiAuth, Auth, AuthState};

    #[test_log::test]
    fn api_auth_exposes_configuration_states() {
        let unconfigured = ApiAuth::builder().without_auth().build();
        assert_eq!(unconfigured.state(), AuthState::NotConfigured);

        let required = ApiAuth::builder()
            .without_auth()
            .with_configured(true)
            .build();
        assert_eq!(required.state(), AuthState::AuthenticationRequired);

        let authenticated = ApiAuth::builder()
            .without_auth()
            .with_configured(true)
            .with_logged_in(true)
            .build();
        assert_eq!(authenticated.state(), AuthState::Authenticated);
    }

    #[test_log::test]
    fn api_auth_supports_expired_and_failed_states() {
        let auth = ApiAuth::builder()
            .without_auth()
            .with_configured(true)
            .build();

        auth.set_state(AuthState::Expired);
        assert_eq!(auth.state(), AuthState::Expired);

        auth.set_state(AuthState::Failed {
            message: "rejected credentials".to_string(),
        });
        assert_eq!(
            auth.state(),
            AuthState::Failed {
                message: "rejected credentials".to_string()
            }
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn api_auth_validation_transitions_to_authenticated() {
        let auth = ApiAuth::builder()
            .without_auth()
            .with_configured(true)
            .with_validate_credentials(|| async { Ok(true) })
            .build();

        assert!(auth.validate_credentials().await.unwrap());
        assert_eq!(auth.state(), AuthState::Authenticated);
    }

    #[test_log::test(switchy_async::test)]
    async fn api_auth_validation_transitions_to_failed() {
        let auth = ApiAuth::builder()
            .without_auth()
            .with_configured(true)
            .with_validate_credentials(|| async {
                Err(Box::new(std::io::Error::other("rejected credentials"))
                    as Box<dyn std::error::Error + Send>)
            })
            .build();

        assert!(auth.validate_credentials().await.is_err());
        assert_eq!(
            auth.state(),
            AuthState::Failed {
                message: "rejected credentials".to_string()
            }
        );
    }

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
        assert_eq!(auth.state(), AuthState::Authenticated);
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
        assert_eq!(auth.state(), AuthState::AuthenticationRequired);
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
        assert!(matches!(auth.state(), AuthState::Failed { .. }));
    }

    #[test_log::test]
    fn auth_from_option_none_converts_to_auth_none() {
        let auth: Auth = None::<Auth>.into();
        assert!(matches!(auth, Auth::None));
    }

    #[test_log::test]
    fn api_auth_builder_auth_mutable_method_sets_auth() {
        let mut builder = super::ApiAuthBuilder::new();
        builder.auth(Auth::None);
        let api_auth = builder.build();

        assert!(matches!(*api_auth, Auth::None));
    }

    #[cfg(feature = "auth-poll")]
    #[test_log::test]
    fn auth_as_poll_returns_some_for_poll_variant() {
        {
            use super::poll::PollAuth;

            let poll = PollAuth::new();
            let auth = Auth::Poll(poll);

            assert!(auth.as_poll().is_some());
        }
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
        {
            use super::username_password::UsernamePasswordAuth;

            let up_auth = UsernamePasswordAuth::builder()
                .with_handler(|_u, _p| async { Ok(true) })
                .build()
                .unwrap();
            let auth = Auth::UsernamePassword(up_auth);

            assert!(auth.as_username_password().is_some());
        }
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
        {
            use super::poll::PollAuth;

            let poll = PollAuth::new();
            let auth = Auth::Poll(poll);

            assert!(auth.into_poll().is_some());
        }
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
        {
            use super::username_password::UsernamePasswordAuth;

            let up_auth = UsernamePasswordAuth::builder()
                .with_handler(|_u, _p| async { Ok(true) })
                .build()
                .unwrap();
            let auth = Auth::UsernamePassword(up_auth);

            assert!(auth.into_username_password().is_some());
        }
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
        {
            use super::poll::PollAuth;

            let poll = PollAuth::new();
            let api_auth = ApiAuth::builder().with_auth(poll).build();

            assert!(api_auth.into_poll().is_some());
        }
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
        {
            use super::username_password::UsernamePasswordAuth;

            let up_auth = UsernamePasswordAuth::builder()
                .with_handler(|_u, _p| async { Ok(true) })
                .build()
                .unwrap();
            let api_auth = ApiAuth::builder().with_auth(up_auth).build();

            assert!(api_auth.into_username_password().is_some());
        }
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
        {
            use super::poll::PollAuth;

            let poll = PollAuth::new();
            let api_auth = ApiAuth::builder().with_auth(poll).build();

            assert!(api_auth.as_poll().is_some());
        }
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
        {
            use super::username_password::UsernamePasswordAuth;

            let up_auth = UsernamePasswordAuth::builder()
                .with_handler(|_u, _p| async { Ok(true) })
                .build()
                .unwrap();
            let api_auth = ApiAuth::builder().with_auth(up_auth).build();

            assert!(api_auth.as_username_password().is_some());
        }
    }

    #[cfg(feature = "auth-username-password")]
    #[test_log::test]
    fn api_auth_as_username_password_returns_none_for_other_variants() {
        let api_auth = ApiAuth::builder().without_auth().build();
        assert!(api_auth.as_username_password().is_none());
    }

    #[cfg(feature = "auth-poll")]
    #[test_log::test]
    fn auth_from_option_some_converts_to_wrapped_auth() {
        {
            use super::poll::PollAuth;

            let poll = PollAuth::new();
            let auth: Auth = Some(Auth::Poll(poll)).into();

            assert!(matches!(auth, Auth::Poll(_)));
        }
    }

    #[test_log::test]
    fn api_auth_deref_returns_inner_auth() {
        let api_auth = ApiAuth::builder().without_auth().build();

        let auth_ref: &Auth = &api_auth;
        assert!(matches!(auth_ref, Auth::None));
    }

    #[cfg(feature = "auth-poll")]
    #[test_log::test]
    fn api_auth_deref_mut_allows_modifying_inner_auth() {
        {
            use super::poll::PollAuth;

            let mut api_auth = ApiAuth::builder().without_auth().build();

            // Verify starts as None
            assert!(matches!(*api_auth, Auth::None));

            // Modify through DerefMut
            *api_auth = Auth::Poll(PollAuth::new());

            // Verify changed to Poll
            assert!(matches!(*api_auth, Auth::Poll(_)));
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn api_auth_validate_credentials_sets_logged_in_to_false_when_validator_returns_false() {
        let auth = ApiAuth::builder()
            .without_auth()
            .with_logged_in(true)
            .with_validate_credentials(|| async { Ok(false) })
            .build();

        assert!(auth.is_logged_in().await.unwrap());

        auth.validate_credentials().await.unwrap();

        assert!(!auth.is_logged_in().await.unwrap());
    }
}
