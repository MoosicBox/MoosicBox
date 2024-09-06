use moosicbox_database::{Database, DatabaseError};
use nanoid::nanoid;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GetOrInitServerIdentityError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("Failed to get server identity")]
    Failed,
}

pub async fn get_or_init_server_identity(
    db: &dyn Database,
) -> Result<String, GetOrInitServerIdentityError> {
    if let Some(identity) = db.select("identity").execute_first(db).await? {
        identity
    } else {
        let id = nanoid!();

        db.insert("identity").value("id", id).execute(db).await?
    }
    .get("id")
    .and_then(|x| x.as_str().map(std::string::ToString::to_string))
    .ok_or(GetOrInitServerIdentityError::Failed)
}
