use std::{future::Future, pin::Pin, sync::Arc};

use crate::Error;

use super::Auth;

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
    #[must_use]
    pub const fn new() -> Self {
        Self { handle_login: None }
    }

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

    /// # Errors
    ///
    /// * If the `handle_login` is missing
    pub fn build(self) -> Result<UsernamePasswordAuth, Error> {
        let handle_login = self
            .handle_login
            .ok_or_else(|| Error::Other(Box::new(std::io::Error::other("handle_login missing"))))?;

        Ok(UsernamePasswordAuth { handle_login })
    }
}

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
    #[must_use]
    pub const fn builder() -> UsernamePasswordAuthBuilder {
        UsernamePasswordAuthBuilder::new()
    }

    /// # Errors
    ///
    /// * If the username password auth fails
    #[allow(clippy::unused_async)]
    pub async fn login(
        &self,
        username: impl Into<String> + Send,
        password: impl Into<String> + Send,
    ) -> Result<bool, Box<dyn std::error::Error + Send>> {
        (self.handle_login)(username.into(), password.into()).await
    }
}
