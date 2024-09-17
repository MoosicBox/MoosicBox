use std::sync::{Arc, LazyLock, RwLock};

use crate::Database;

pub static PROFILES: LazyLock<DatabaseProfiles> = LazyLock::new(DatabaseProfiles::default);

type Profiles = RwLock<Vec<(String, Arc<Box<dyn Database>>)>>;

#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct DatabaseProfiles {
    profiles: Profiles,
}

impl DatabaseProfiles {
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    pub fn add(&self, profile: String, database: Arc<Box<dyn Database>>) {
        self.profiles.write().unwrap().push((profile, database));
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn get(&self, profile: &str) -> Option<Arc<Box<dyn Database>>> {
        self.profiles
            .read()
            .unwrap()
            .iter()
            .find_map(|(p, database)| {
                if p == profile {
                    Some(database.clone())
                } else {
                    None
                }
            })
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.profiles
            .read()
            .unwrap()
            .iter()
            .map(|(x, _)| x.to_owned())
            .collect()
    }
}

#[cfg(feature = "api")]
pub mod api {
    use std::{ops::Deref, sync::Arc};

    use actix_web::{dev::Payload, error::ErrorBadRequest, FromRequest, HttpRequest};
    use futures::future::{err, ok, Ready};

    use crate::Database;

    use super::PROFILES;

    pub struct LibraryDatabase {
        pub profile: String,
        pub database: Arc<Box<dyn Database>>,
    }

    impl From<&LibraryDatabase> for Arc<Box<dyn Database>> {
        fn from(value: &LibraryDatabase) -> Self {
            value.database.clone()
        }
    }

    impl From<LibraryDatabase> for Arc<Box<dyn Database>> {
        fn from(value: LibraryDatabase) -> Self {
            value.database
        }
    }

    impl Deref for LibraryDatabase {
        type Target = dyn Database;

        fn deref(&self) -> &Self::Target {
            &**self.database
        }
    }

    impl FromRequest for LibraryDatabase {
        type Error = actix_web::Error;
        type Future = Ready<Result<Self, actix_web::Error>>;

        fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
            let Some(profile_header_value) = req.headers().get("moosicbox-profile") else {
                return err(ErrorBadRequest("Missing moosicbox-profile header"));
            };
            let Ok(profile) = profile_header_value.to_str() else {
                return err(ErrorBadRequest("Invalid moosicbox-profile header"));
            };
            let Some(database) = PROFILES.get(profile) else {
                return err(ErrorBadRequest("Invalid profile"));
            };

            ok(Self {
                profile: profile.to_owned(),
                database,
            })
        }
    }
}
