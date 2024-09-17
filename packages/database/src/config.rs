use std::{
    ops::Deref,
    sync::{Arc, OnceLock},
};

use crate::Database;

static DATABASE: OnceLock<Arc<Box<dyn Database>>> = OnceLock::new();

/// # Errors
///
/// Will error if database has already been initialized
pub fn init(database: Arc<Box<dyn Database>>) -> Result<(), Arc<Box<dyn Database>>> {
    DATABASE.set(database)
}

#[allow(clippy::module_name_repetitions)]
pub struct ConfigDatabase {
    pub database: Arc<Box<dyn Database>>,
}

impl From<&ConfigDatabase> for Arc<Box<dyn Database>> {
    fn from(value: &ConfigDatabase) -> Self {
        value.database.clone()
    }
}

impl From<ConfigDatabase> for Arc<Box<dyn Database>> {
    fn from(value: ConfigDatabase) -> Self {
        value.database
    }
}

impl Deref for ConfigDatabase {
    type Target = dyn Database;

    fn deref(&self) -> &Self::Target {
        &**self.database
    }
}

#[cfg(feature = "api")]
mod api {
    use actix_web::{dev::Payload, error::ErrorInternalServerError, FromRequest, HttpRequest};
    use futures::future::{err, ok, Ready};

    use super::DATABASE;

    impl FromRequest for super::ConfigDatabase {
        type Error = actix_web::Error;
        type Future = Ready<Result<Self, actix_web::Error>>;

        fn from_request(_req: &HttpRequest, _: &mut Payload) -> Self::Future {
            let Some(database) = DATABASE.get().cloned() else {
                return err(ErrorInternalServerError("Config database not initialized"));
            };

            ok(Self { database })
        }
    }
}
