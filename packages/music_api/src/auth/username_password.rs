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
