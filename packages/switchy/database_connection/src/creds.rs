//! Database credential retrieval from environment variables and AWS SSM.
//!
//! This module provides functionality to retrieve database credentials from
//! multiple sources in order of precedence:
//!
//! 1. `DATABASE_URL` environment variable (full connection string)
//! 2. Individual environment variables (`DB_HOST`, `DB_NAME`, `DB_USER`, `DB_PASSWORD`)
//! 3. AWS Systems Manager (SSM) Parameter Store (for cloud deployments)
//!
//! # Examples
//!
//! ```rust,no_run
//! # #[cfg(feature = "creds")]
//! # {
//! # use switchy_database_connection::creds::get_db_creds;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Retrieve credentials from environment or AWS SSM
//! let creds = get_db_creds().await?;
//! # Ok(())
//! # }
//! # }
//! ```

#![allow(clippy::module_name_repetitions)]

use thiserror::Error;

use crate::{Credentials, CredentialsParseError};

/// Errors that can occur when retrieving database credentials
#[derive(Debug, Error)]
pub enum GetDbCredsError {
    /// Required connection options (host, name, or user) are missing or invalid
    #[error("Invalid Connection Options")]
    InvalidConnectionOptions,
    /// Error parsing credentials from `DATABASE_URL` environment variable
    #[error(transparent)]
    CredentialsParseError(#[from] CredentialsParseError),
    /// Failed to retrieve parameters from AWS SSM
    #[error("Failed to fetch SSM Parameters: {0:?}")]
    FailedSsmParameters(
        #[from]
        Box<
            aws_sdk_ssm::error::SdkError<
                aws_sdk_ssm::operation::get_parameters::GetParametersError,
            >,
        >,
    ),
    /// SSM parameters exist but contain invalid data
    #[error("Invalid SSM Parameters")]
    InvalidSsmParameters,
    /// Required SSM parameters are not available
    #[error("Missing SSM Parameters")]
    MissingSsmParameters,
    /// A specific SSM parameter is missing
    #[error("Missing SSM Parameter: {0}")]
    MissingSsmParameter(&'static str),
}

/// Retrieves database credentials from environment variables or AWS SSM.
///
/// Attempts to retrieve credentials in the following order:
/// 1. `DATABASE_URL` environment variable (parsed as connection string)
/// 2. Individual environment variables (`DB_HOST`, `DB_NAME`, `DB_USER`, `DB_PASSWORD`)
/// 3. AWS Systems Manager Parameter Store (requires AWS credentials)
///
/// # Errors
///
/// * If invalid connection options were given
/// * If failed to retrieve the credentials from the SSM parameters
#[allow(clippy::too_many_lines)]
pub async fn get_db_creds() -> Result<Credentials, GetDbCredsError> {
    log::trace!("get_db_creds");

    // First try DATABASE_URL
    if let Ok(database_url) = switchy_env::var("DATABASE_URL") {
        log::debug!("get_db_creds: Using DATABASE_URL");
        return Credentials::from_url(&database_url)
            .map_err(GetDbCredsError::CredentialsParseError);
    }

    let env_db_host = switchy_env::var("DB_HOST").ok();
    let env_db_name = switchy_env::var("DB_NAME").ok();
    let env_db_user = switchy_env::var("DB_USER").ok();
    let env_db_password = switchy_env::var("DB_PASSWORD").ok();
    let env_db_port = switchy_env::var("DB_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok());

    Ok(
        if env_db_host.is_some() || env_db_name.is_some() || env_db_user.is_some() {
            log::debug!("get_db_creds: Using env var values host={env_db_host:?}");
            Credentials::new(
                env_db_host.ok_or(GetDbCredsError::InvalidConnectionOptions)?,
                env_db_port,
                env_db_name.ok_or(GetDbCredsError::InvalidConnectionOptions)?,
                env_db_user.ok_or(GetDbCredsError::InvalidConnectionOptions)?,
                env_db_password,
            )
        } else {
            use aws_config::{BehaviorVersion, Region};
            use aws_sdk_ssm::Client;
            use std::collections::BTreeMap;

            log::debug!("get_db_creds: Fetching creds from aws ssm");

            let config = aws_config::defaults(BehaviorVersion::latest())
                .region(Region::new("us-east-1"))
                .load()
                .await;

            let client = Client::new(&config);

            let ssm_db_name_param_name = switchy_env::var("SSM_DB_NAME_PARAM_NAME")
                .unwrap_or_else(|_| "moosicbox_db_name".to_string());
            let ssm_db_host_param_name = switchy_env::var("SSM_DB_HOST_PARAM_NAME")
                .unwrap_or_else(|_| "moosicbox_db_hostname".to_string());
            let ssm_db_user_param_name = switchy_env::var("SSM_DB_USER_PARAM_NAME")
                .unwrap_or_else(|_| "moosicbox_db_user".to_string());
            let ssm_db_password_param_name = switchy_env::var("SSM_DB_PASSWORD_PARAM_NAME")
                .unwrap_or_else(|_| "moosicbox_db_password".to_string());

            let ssm_db_name_param_name = ssm_db_name_param_name.as_str();
            let ssm_db_host_param_name = ssm_db_host_param_name.as_str();
            let ssm_db_user_param_name = ssm_db_user_param_name.as_str();
            let ssm_db_password_param_name = ssm_db_password_param_name.as_str();

            let params = match client
                .get_parameters()
                .set_with_decryption(Some(true))
                .names(ssm_db_name_param_name)
                .names(ssm_db_host_param_name)
                .names(ssm_db_password_param_name)
                .names(ssm_db_user_param_name)
                .send()
                .await
            {
                Ok(params) => params,
                Err(err) => return Err(GetDbCredsError::FailedSsmParameters(Box::new(err))),
            };
            let params = params
                .parameters
                .ok_or(GetDbCredsError::InvalidSsmParameters)?;
            let params: BTreeMap<String, String> = params
                .iter()
                .map(|param| {
                    param
                        .name()
                        .map(str::to_string)
                        .ok_or(GetDbCredsError::InvalidSsmParameters)
                        .and_then(|name| {
                            param
                                .value()
                                .map(str::to_string)
                                .ok_or(GetDbCredsError::InvalidSsmParameters)
                                .map(|value| (name, value))
                        })
                })
                .collect::<Result<_, _>>()?;

            let host = params
                .get(ssm_db_host_param_name)
                .cloned()
                .ok_or(GetDbCredsError::MissingSsmParameter("No hostname"))?;
            let name = params
                .get(ssm_db_name_param_name)
                .cloned()
                .ok_or(GetDbCredsError::MissingSsmParameter("No db_name"))?;
            let user = params
                .get(ssm_db_user_param_name)
                .cloned()
                .ok_or(GetDbCredsError::MissingSsmParameter("No db_user"))?;
            let password = params
                .get(ssm_db_password_param_name)
                .cloned()
                .ok_or(GetDbCredsError::MissingSsmParameter("No db_password"))?;

            let password = if password.is_empty() {
                None
            } else {
                Some(password)
            };

            log::debug!("get_db_creds: Fetching creds from aws ssm host={host}");

            Credentials::new(host, None, name, user, password)
        },
    )
}

#[cfg(all(test, feature = "simulator"))]
mod tests {
    use super::*;
    use serial_test::serial;
    use switchy_env::simulator::{remove_var, reset, set_var};

    /// Helper to set up test environment and ensure cleanup on drop
    struct TestEnv;

    impl TestEnv {
        fn new() -> Self {
            reset();
            Self
        }
    }

    impl Drop for TestEnv {
        fn drop(&mut self) {
            reset();
        }
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_db_creds_from_database_url() {
        let _env = TestEnv::new();

        // Set DATABASE_URL - this takes precedence over individual env vars
        set_var(
            "DATABASE_URL",
            "postgres://testuser:testpass@testhost:5432/testdb",
        );

        let creds = get_db_creds().await.expect("Failed to get credentials");

        assert_eq!(creds.host(), "testhost:5432");
        assert_eq!(creds.name(), "testdb");
        assert_eq!(creds.user(), "testuser");
        assert_eq!(creds.password(), Some("testpass"));
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_db_creds_from_database_url_without_password() {
        let _env = TestEnv::new();

        set_var("DATABASE_URL", "mysql://admin@dbserver:3306/proddb");

        let creds = get_db_creds().await.expect("Failed to get credentials");

        assert_eq!(creds.host(), "dbserver:3306");
        assert_eq!(creds.name(), "proddb");
        assert_eq!(creds.user(), "admin");
        assert_eq!(creds.password(), None);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_db_creds_from_database_url_invalid_format() {
        let _env = TestEnv::new();

        // Set an invalid DATABASE_URL
        set_var("DATABASE_URL", "not-a-valid-url");

        let result = get_db_creds().await;

        assert!(matches!(
            result,
            Err(GetDbCredsError::CredentialsParseError(
                CredentialsParseError::InvalidUrl
            ))
        ));
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_db_creds_from_database_url_unsupported_scheme() {
        let _env = TestEnv::new();

        set_var("DATABASE_URL", "mongodb://user:pass@host/db");

        let result = get_db_creds().await;

        assert!(matches!(
            result,
            Err(GetDbCredsError::CredentialsParseError(
                CredentialsParseError::UnsupportedScheme(_)
            ))
        ));
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_db_creds_from_individual_env_vars() {
        let _env = TestEnv::new();

        // Remove DATABASE_URL to force individual env var path
        remove_var("DATABASE_URL");

        set_var("DB_HOST", "myhost.example.com");
        set_var("DB_NAME", "mydb");
        set_var("DB_USER", "myuser");
        set_var("DB_PASSWORD", "mysecret");

        let creds = get_db_creds().await.expect("Failed to get credentials");

        assert_eq!(creds.host(), "myhost.example.com");
        assert_eq!(creds.name(), "mydb");
        assert_eq!(creds.user(), "myuser");
        assert_eq!(creds.password(), Some("mysecret"));
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_db_creds_from_individual_env_vars_without_password() {
        let _env = TestEnv::new();

        remove_var("DATABASE_URL");

        set_var("DB_HOST", "localhost");
        set_var("DB_NAME", "devdb");
        set_var("DB_USER", "devuser");
        remove_var("DB_PASSWORD");

        let creds = get_db_creds().await.expect("Failed to get credentials");

        assert_eq!(creds.host(), "localhost");
        assert_eq!(creds.name(), "devdb");
        assert_eq!(creds.user(), "devuser");
        assert_eq!(creds.password(), None);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_db_creds_missing_host_returns_error() {
        let _env = TestEnv::new();

        remove_var("DATABASE_URL");
        remove_var("DB_HOST");
        set_var("DB_NAME", "testdb");
        set_var("DB_USER", "testuser");

        let result = get_db_creds().await;

        assert!(matches!(
            result,
            Err(GetDbCredsError::InvalidConnectionOptions)
        ));
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_db_creds_missing_name_returns_error() {
        let _env = TestEnv::new();

        remove_var("DATABASE_URL");
        set_var("DB_HOST", "testhost");
        remove_var("DB_NAME");
        set_var("DB_USER", "testuser");

        let result = get_db_creds().await;

        assert!(matches!(
            result,
            Err(GetDbCredsError::InvalidConnectionOptions)
        ));
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_db_creds_missing_user_returns_error() {
        let _env = TestEnv::new();

        remove_var("DATABASE_URL");
        set_var("DB_HOST", "testhost");
        set_var("DB_NAME", "testdb");
        remove_var("DB_USER");

        let result = get_db_creds().await;

        assert!(matches!(
            result,
            Err(GetDbCredsError::InvalidConnectionOptions)
        ));
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_get_db_creds_database_url_takes_precedence() {
        let _env = TestEnv::new();

        // Set both DATABASE_URL and individual vars
        set_var(
            "DATABASE_URL",
            "postgres://urluser:urlpass@urlhost:5432/urldb",
        );
        set_var("DB_HOST", "envhost");
        set_var("DB_NAME", "envdb");
        set_var("DB_USER", "envuser");
        set_var("DB_PASSWORD", "envpass");

        let creds = get_db_creds().await.expect("Failed to get credentials");

        // Should use DATABASE_URL values, not individual env vars
        assert_eq!(creds.host(), "urlhost:5432");
        assert_eq!(creds.name(), "urldb");
        assert_eq!(creds.user(), "urluser");
        assert_eq!(creds.password(), Some("urlpass"));
    }
}
