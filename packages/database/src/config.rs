use std::{
    ops::Deref,
    sync::{Arc, LazyLock, RwLock},
};

use crate::Database;

#[allow(clippy::type_complexity)]
static DATABASE: LazyLock<Arc<RwLock<Option<Arc<Box<dyn Database>>>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(None)));

/// # Panics
///
/// * If fails to get a writer to the `DATABASE` `RwLock`
pub fn init(database: Arc<Box<dyn Database>>) {
    *DATABASE.write().unwrap() = Some(database);
}

#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone)]
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

impl From<Arc<Box<dyn Database>>> for ConfigDatabase {
    fn from(value: Arc<Box<dyn Database>>) -> Self {
        Self { database: value }
    }
}

impl<'a> From<&'a ConfigDatabase> for &'a dyn Database {
    fn from(value: &'a ConfigDatabase) -> Self {
        &**value.database
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
    use actix_web::{FromRequest, HttpRequest, dev::Payload, error::ErrorInternalServerError};
    use futures::future::{Ready, err, ok};

    use super::DATABASE;

    impl FromRequest for super::ConfigDatabase {
        type Error = actix_web::Error;
        type Future = Ready<Result<Self, actix_web::Error>>;

        fn from_request(_req: &HttpRequest, _: &mut Payload) -> Self::Future {
            let Some(database) = DATABASE.read().unwrap().clone() else {
                return err(ErrorInternalServerError("Config database not initialized"));
            };

            ok(Self { database })
        }
    }
}
