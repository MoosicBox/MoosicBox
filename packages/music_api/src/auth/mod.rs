use std::{
    ops::{Deref, DerefMut},
    pin::Pin,
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

impl<T> From<Option<T>> for Auth
where
    T: Into<Self>,
{
    fn from(value: Option<T>) -> Self {
        value.map_or(Self::None, Into::into)
    }
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
    #[must_use]
    pub const fn new() -> Self {
        Self {
            auth: None,
            logged_in: None,
            validate_credentials: None,
        }
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn without_auth(mut self) -> Self {
        self.auth = Some(Auth::None);
        self
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

    /// # Panics
    ///
    /// * If `auth` is `None`
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

    /// # Errors
    ///
    /// * If the authentication validation fails
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

    #[cfg(feature = "auth-poll")]
    #[must_use]
    pub fn as_poll(&self) -> Option<&poll::PollAuth> {
        <Self as AuthExt>::as_poll(self)
    }

    #[cfg(feature = "auth-poll")]
    #[must_use]
    pub fn into_poll(self) -> Option<poll::PollAuth> {
        <Self as AuthExt>::into_poll(self)
    }

    #[cfg(feature = "auth-username-password")]
    #[must_use]
    pub fn as_username_password(&self) -> Option<&username_password::UsernamePasswordAuth> {
        <Self as AuthExt>::as_username_password(self)
    }

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

    fn deref(&self) -> &Self::Target {
        &self.auth
    }
}

impl DerefMut for ApiAuth {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.auth
    }
}
