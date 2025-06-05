use super::Auth;

#[derive(Debug, Clone)]
pub struct UsernamePasswordAuth;

impl From<UsernamePasswordAuth> for Auth {
    fn from(value: UsernamePasswordAuth) -> Self {
        Self::UsernamePassword(value)
    }
}

impl Default for UsernamePasswordAuth {
    fn default() -> Self {
        Self::new()
    }
}

impl UsernamePasswordAuth {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl UsernamePasswordAuth {
    /// # Errors
    ///
    /// * If the username password auth fails
    #[allow(clippy::unused_async)]
    pub async fn login(
        &self,
        _username: impl Into<String>,
        _password: impl Into<String>,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        Ok(false)
    }
}
