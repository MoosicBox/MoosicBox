use moosicbox_database::Database;
use thiserror::Error;

#[cfg(not(feature = "postgres"))]
#[derive(Debug, Error)]
pub enum InitSqliteError {
    #[error(transparent)]
    Sqlite(#[from] ::rusqlite::Error),
}

#[cfg(not(feature = "postgres"))]
pub async fn init_sqlite() -> Result<Box<dyn Database>, InitSqliteError> {
    let library = ::rusqlite::Connection::open("library.db")?;
    library
        .busy_timeout(std::time::Duration::from_millis(10))
        .expect("Failed to set busy timeout");
    let library = std::sync::Arc::new(std::sync::Mutex::new(library));

    Ok(Box::new(
        moosicbox_database::rusqlite::RusqliteDatabase::new(library),
    ))
}

#[cfg(feature = "postgres")]
#[derive(Debug, Error)]
pub enum InitPostgresError {
    #[error(transparent)]
    Postgres(#[from] tokio_postgres::Error),
    #[error("Invalid Connection Options")]
    InvalidConnectionOptions,
}

#[cfg(feature = "postgres")]
pub async fn init_postgres() -> Result<
    (
        Box<dyn Database>,
        tokio_postgres::Connection<tokio_postgres::Socket, tokio_postgres::tls::NoTlsStream>,
    ),
    InitPostgresError,
> {
    use moosicbox_database::postgres::postgres::PostgresDatabase;
    use tokio_postgres::NoTls;

    let env_db_host = std::env::var("DB_HOST").ok();
    let env_db_name = std::env::var("DB_NAME").ok();
    let env_db_user = std::env::var("DB_USER").ok();
    let env_db_password = std::env::var("DB_PASSWORD").ok();

    let (db_host, db_name, db_user, db_password) =
        if env_db_host.is_some() || env_db_name.is_some() || env_db_user.is_some() {
            (
                env_db_host.ok_or(InitPostgresError::InvalidConnectionOptions)?,
                env_db_name.ok_or(InitPostgresError::InvalidConnectionOptions)?,
                env_db_user.ok_or(InitPostgresError::InvalidConnectionOptions)?,
                env_db_password,
            )
        } else {
            use aws_config::{BehaviorVersion, Region};
            use aws_sdk_ssm::Client;
            use std::collections::HashMap;

            let config = aws_config::defaults(BehaviorVersion::v2023_11_09())
                .region(Region::new("us-east-1"))
                .load()
                .await;

            let client = Client::new(&config);

            let ssm_db_name_param_name = std::env::var("SSM_DB_NAME_PARAM_NAME")
                .unwrap_or("moosicbox_server_db_name".to_string());
            let ssm_db_host_param_name = std::env::var("SSM_DB_HOST_PARAM_NAME")
                .unwrap_or("moosicbox_server_db_hostname".to_string());
            let ssm_db_user_param_name = std::env::var("SSM_DB_USER_PARAM_NAME")
                .unwrap_or("moosicbox_server_db_user".to_string());
            let ssm_db_password_param_name = std::env::var("SSM_DB_PASSWORD_PARAM_NAME")
                .unwrap_or("moosicbox_server_db_password".to_string());

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
                Err(err) => panic!("Failed to get parameters {err:?}"),
            };
            let params = params.parameters.expect("Failed to get params");
            let params: HashMap<String, String> = params
                .iter()
                .map(|param| {
                    (
                        param.name().unwrap().to_string(),
                        param.value().unwrap().to_string(),
                    )
                })
                .collect();

            let password = params
                .get(ssm_db_password_param_name)
                .cloned()
                .expect("No db_password")
                .to_string();

            let password = if password.is_empty() {
                None
            } else {
                Some(password)
            };

            (
                params
                    .get(ssm_db_host_param_name)
                    .cloned()
                    .expect("No hostname")
                    .to_string(),
                params
                    .get(ssm_db_name_param_name)
                    .cloned()
                    .expect("No db_name")
                    .to_string(),
                params
                    .get(ssm_db_user_param_name)
                    .cloned()
                    .expect("No db_user")
                    .to_string(),
                password,
            )
        };

    let mut config = tokio_postgres::Config::new();
    let mut config = config.host(&db_host).dbname(&db_name).user(&db_user);

    if let Some(db_password) = db_password {
        config = config.password(&db_password);
    }

    let (client, connection) = config.connect(NoTls).await?;

    Ok((Box::new(PostgresDatabase::new(client)), connection))
}
