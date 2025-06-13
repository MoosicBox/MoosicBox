use std::{
    collections::BTreeMap,
    ops::Deref,
    sync::{Arc, LazyLock, RwLock},
};

use crate::Database;

pub static PROFILES: LazyLock<DatabaseProfiles> = LazyLock::new(DatabaseProfiles::default);

#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct DatabaseProfiles {
    profiles: Arc<RwLock<BTreeMap<String, LibraryDatabase>>>,
}

impl DatabaseProfiles {
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    pub fn add(&self, profile: String, database: Arc<Box<dyn Database>>) {
        moosicbox_profiles::PROFILES.add(profile.clone());
        self.profiles
            .write()
            .unwrap()
            .insert(profile, LibraryDatabase { database });
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    pub fn remove(&self, profile: &str) {
        self.profiles.write().unwrap().retain(|p, _| p != profile);
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned or the profile somehow wasn't added to the list of
    /// profiles
    #[must_use]
    pub fn add_fetch(&self, profile: &str, database: Arc<Box<dyn Database>>) -> LibraryDatabase {
        self.add(profile.to_owned(), database);
        self.get(profile).unwrap()
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn get(&self, profile: &str) -> Option<LibraryDatabase> {
        self.profiles
            .read()
            .unwrap()
            .iter()
            .find_map(|(p, db)| if p == profile { Some(db.clone()) } else { None })
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.profiles
            .read()
            .unwrap().keys().map(|profile| profile.clone())
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct LibraryDatabase {
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

impl From<Arc<Box<dyn Database>>> for LibraryDatabase {
    fn from(value: Arc<Box<dyn Database>>) -> Self {
        Self { database: value }
    }
}

impl<'a> From<&'a LibraryDatabase> for &'a dyn Database {
    fn from(value: &'a LibraryDatabase) -> Self {
        &**value.database
    }
}

impl Deref for LibraryDatabase {
    type Target = dyn Database;

    fn deref(&self) -> &Self::Target {
        &**self.database
    }
}

#[cfg(feature = "api")]
pub mod api {
    use actix_web::{FromRequest, HttpRequest, dev::Payload, error::ErrorBadRequest};
    use futures::future::{Ready, err, ok};
    use moosicbox_profiles::api::ProfileName;

    use super::{LibraryDatabase, PROFILES};

    impl FromRequest for LibraryDatabase {
        type Error = actix_web::Error;
        type Future = Ready<Result<Self, actix_web::Error>>;

        fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
            let profile = ProfileName::from_request_inner(req);
            let profile = match profile {
                Ok(profile) => profile,
                Err(e) => {
                    return err(e);
                }
            };

            let Some(database) = PROFILES.get(&profile.0) else {
                return err(ErrorBadRequest("Invalid profile"));
            };

            ok(database)
        }
    }
}
