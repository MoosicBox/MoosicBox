//! Username and password authentication implementation.
//!
//! This module provides `UsernamePasswordAuth`, which implements a traditional username
//! and password authentication flow. The login handler is customizable via the builder pattern.

use std::{future::Future, pin::Pin, sync::Arc};

use crate::Error;

use super::Auth;

/// Builder for username/password authentication.
#[derive(Clone)]
pub struct UsernamePasswordAuthBuilder {
    handle_login: Option<
        Arc<
            dyn Fn(
                    String,
                    String,
                ) -> Pin<
                    Box<
                        dyn Future<Output = Result<bool, Box<dyn std::error::Error + Send>>> + Send,
                    >,
                > + Send
                + Sync,
        >,
    >,
}

impl Default for UsernamePasswordAuthBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl UsernamePasswordAuthBuilder {
    /// Creates a new builder.
    #[must_use]
    pub const fn new() -> Self {
        Self { handle_login: None }
    }

    /// Sets the login handler function.
    #[must_use]
    pub fn with_handler<
        Fut: Future<Output = Result<bool, Box<dyn std::error::Error + Send>>> + Send + 'static,
        Func: Fn(String, String) -> Fut + Send + Sync + 'static,
    >(
        mut self,
        handle_login: Func,
    ) -> Self {
        self.handle_login = Some(Arc::new(move |username, password| {
            Box::pin(handle_login(username, password))
        }));
        self
    }

    /// Builds the username/password authentication.
    ///
    /// # Errors
    ///
    /// * If the login handler was not configured
    pub fn build(self) -> Result<UsernamePasswordAuth, Error> {
        let handle_login = self
            .handle_login
            .ok_or_else(|| Error::Other(Box::new(std::io::Error::other("handle_login missing"))))?;

        Ok(UsernamePasswordAuth { handle_login })
    }
}

/// Username and password authentication configuration.
#[derive(Clone)]
pub struct UsernamePasswordAuth {
    handle_login: Arc<
        dyn Fn(
                String,
                String,
            ) -> Pin<
                Box<dyn Future<Output = Result<bool, Box<dyn std::error::Error + Send>>> + Send>,
            > + Send
            + Sync,
    >,
}

impl std::fmt::Debug for UsernamePasswordAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsernamePasswordAuth")
            .finish_non_exhaustive()
    }
}

impl From<UsernamePasswordAuth> for Auth {
    /// Converts `UsernamePasswordAuth` into `Auth::UsernamePassword`.
    fn from(value: UsernamePasswordAuth) -> Self {
        Self::UsernamePassword(value)
    }
}

impl UsernamePasswordAuth {
    /// Creates a new builder.
    #[must_use]
    pub const fn builder() -> UsernamePasswordAuthBuilder {
        UsernamePasswordAuthBuilder::new()
    }

    /// Attempts to log in with the given username and password.
    ///
    /// # Errors
    ///
    /// * If the login attempt fails
    #[allow(clippy::unused_async)]
    pub async fn login(
        &self,
        username: impl Into<String> + Send,
        password: impl Into<String> + Send,
    ) -> Result<bool, Box<dyn std::error::Error + Send>> {
        (self.handle_login)(username.into(), password.into()).await
    }
}

#[cfg(test)]
mod test {
    use super::{UsernamePasswordAuth, UsernamePasswordAuthBuilder};

    #[test_log::test]
    fn username_password_auth_builder_new_creates_empty_builder() {
        let builder = UsernamePasswordAuthBuilder::new();
        assert!(builder.handle_login.is_none());
    }

    #[test_log::test]
    fn username_password_auth_builder_build_fails_without_handler() {
        let builder = UsernamePasswordAuthBuilder::new();
        let result = builder.build();
        assert!(result.is_err());
    }

    #[test_log::test]
    fn username_password_auth_builder_with_handler_sets_handler() {
        let builder = UsernamePasswordAuthBuilder::new().with_handler(|_u, _p| async { Ok(true) });

        let result = builder.build();
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn username_password_auth_login_calls_handler() {
        let auth = UsernamePasswordAuth::builder()
            .with_handler(|username, password| async move {
                if username == "test_user" && password == "test_pass" {
                    Ok(true)
                } else {
                    Ok(false)
                }
            })
            .build()
            .unwrap();

        let result = auth.login("test_user", "test_pass").await.unwrap();
        assert!(result);

        let result = auth.login("wrong_user", "wrong_pass").await.unwrap();
        assert!(!result);
    }

    #[test_log::test(switchy_async::test)]
    async fn username_password_auth_login_propagates_handler_error() {
        let auth = UsernamePasswordAuth::builder()
            .with_handler(|_u, _p| async {
                Err(Box::new(std::io::Error::other("handler error"))
                    as Box<dyn std::error::Error + Send>)
            })
            .build()
            .unwrap();

        let result = auth.login("user", "pass").await;
        assert!(result.is_err());
    }
}
