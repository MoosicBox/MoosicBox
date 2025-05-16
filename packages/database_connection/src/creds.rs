#![allow(clippy::module_name_repetitions)]

use thiserror::Error;

use crate::Credentials;

#[derive(Debug, Error)]
pub enum GetDbCredsError {
    #[error("Invalid Connection Options")]
    InvalidConnectionOptions,
    #[error("Failed to fetch SSM Parameters: {0:?}")]
    FailedSsmParameters(
        #[from]
        Box<
            aws_sdk_ssm::error::SdkError<
                aws_sdk_ssm::operation::get_parameters::GetParametersError,
            >,
        >,
    ),
    #[error("Invalid SSM Parameters")]
    InvalidSsmParameters,
    #[error("Missing SSM Parameters")]
    MissingSsmParameters,
    #[error("Missing SSM Parameter: {0}")]
    MissingSsmParameter(&'static str),
}

/// # Errors
///
/// * If invalid connection options were given
/// * If failed to retrieve the credentials from the SSM parameters
pub async fn get_db_creds() -> Result<Credentials, GetDbCredsError> {
    log::trace!("get_db_creds");

    let env_db_host = std::env::var("DB_HOST").ok();
    let env_db_name = std::env::var("DB_NAME").ok();
    let env_db_user = std::env::var("DB_USER").ok();
    let env_db_password = std::env::var("DB_PASSWORD").ok();

    Ok(
        if env_db_host.is_some() || env_db_name.is_some() || env_db_user.is_some() {
            log::debug!("get_db_creds: Using env var values host={env_db_host:?}");
            Credentials::new(
                env_db_host.ok_or(GetDbCredsError::InvalidConnectionOptions)?,
                env_db_name.ok_or(GetDbCredsError::InvalidConnectionOptions)?,
                env_db_user.ok_or(GetDbCredsError::InvalidConnectionOptions)?,
                env_db_password,
            )
        } else {
            use aws_config::{BehaviorVersion, Region};
            use aws_sdk_ssm::Client;
            use std::collections::HashMap;

            log::debug!("get_db_creds: Fetching creds from aws ssm");

            let config = aws_config::defaults(BehaviorVersion::latest())
                .region(Region::new("us-east-1"))
                .load()
                .await;

            let client = Client::new(&config);

            let ssm_db_name_param_name = std::env::var("SSM_DB_NAME_PARAM_NAME")
                .unwrap_or_else(|_| "moosicbox_db_name".to_string());
            let ssm_db_host_param_name = std::env::var("SSM_DB_HOST_PARAM_NAME")
                .unwrap_or_else(|_| "moosicbox_db_hostname".to_string());
            let ssm_db_user_param_name = std::env::var("SSM_DB_USER_PARAM_NAME")
                .unwrap_or_else(|_| "moosicbox_db_user".to_string());
            let ssm_db_password_param_name = std::env::var("SSM_DB_PASSWORD_PARAM_NAME")
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
            let params: HashMap<String, String> = params
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

            Credentials::new(host, name, user, password)
        },
    )
}
