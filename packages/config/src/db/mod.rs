use moosicbox_database::{DatabaseError, config::ConfigDatabase, query::FilterableQuery as _};
use moosicbox_json_utils::{ToValueType as _, database::DatabaseFetchError};
use nanoid::nanoid;
use thiserror::Error;

pub mod models;

#[derive(Debug, Error)]
pub enum GetOrInitServerIdentityError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("Failed to get server identity")]
    Failed,
}

pub(crate) async fn get_server_identity(
    db: &ConfigDatabase,
) -> Result<Option<String>, DatabaseError> {
    Ok(db
        .select("identity")
        .execute_first(db)
        .await?
        .and_then(|x| {
            x.get("id")
                .and_then(|x| x.as_str().map(std::string::ToString::to_string))
        }))
}

pub(crate) async fn get_or_init_server_identity(
    db: &ConfigDatabase,
) -> Result<String, GetOrInitServerIdentityError> {
    if let Some(identity) = get_server_identity(db).await? {
        Ok(identity)
    } else {
        let id = nanoid!();

        db.insert("identity")
            .value("id", id)
            .execute(db)
            .await?
            .get("id")
            .and_then(|x| x.as_str().map(std::string::ToString::to_string))
            .ok_or(GetOrInitServerIdentityError::Failed)
    }
}

#[allow(unused)]
pub(crate) async fn upsert_profile(
    db: &ConfigDatabase,
    name: &str,
) -> Result<models::Profile, DatabaseFetchError> {
    Ok(db
        .upsert("profiles")
        .where_eq("name", name)
        .value("name", name)
        .execute_first(db)
        .await?
        .to_value_type()?)
}

pub(crate) async fn delete_profile(
    db: &ConfigDatabase,
    name: &str,
) -> Result<Vec<models::Profile>, DatabaseFetchError> {
    Ok(db
        .delete("profiles")
        .where_eq("name", name)
        .execute(db)
        .await?
        .to_value_type()?)
}

pub(crate) async fn create_profile(
    db: &ConfigDatabase,
    name: &str,
) -> Result<models::Profile, DatabaseFetchError> {
    Ok(db
        .insert("profiles")
        .value("name", name)
        .execute(db)
        .await?
        .to_value_type()?)
}

pub(crate) async fn get_profiles(
    db: &ConfigDatabase,
) -> Result<Vec<models::Profile>, DatabaseFetchError> {
    Ok(db.select("profiles").execute(db).await?.to_value_type()?)
}
