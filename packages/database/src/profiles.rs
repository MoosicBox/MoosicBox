use std::{
    ops::Deref,
    sync::{Arc, LazyLock, RwLock},
};

use crate::Database;

pub static PROFILES: LazyLock<DatabaseProfiles> = LazyLock::new(DatabaseProfiles::default);

#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct DatabaseProfiles {
    profiles: Arc<RwLock<Vec<(String, LibraryDatabase)>>>,
}

impl DatabaseProfiles {
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    pub fn add(&self, profile: String, database: Arc<Box<dyn Database>>) {
        self.profiles
            .write()
            .unwrap()
            .push((profile, LibraryDatabase { database }));
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    pub fn remove(&self, profile: &str) {
        self.profiles.write().unwrap().retain(|(p, _)| p != profile);
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned or the profile somehow wasn't added to the list of
    /// profiles
    #[must_use]
    pub fn fetch_add(&self, profile: &str, database: Arc<Box<dyn Database>>) -> LibraryDatabase {
        self.add(profile.to_owned(), database);
        self.get(profile).unwrap()
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn get(&self, profile: &str) -> Option<LibraryDatabase> {
        self.profiles.read().unwrap().iter().find_map(|(p, db)| {
            if p == profile {
                Some(db.clone())
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
            .map(|(profile, _)| profile.clone())
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
    use actix_web::{dev::Payload, error::ErrorBadRequest, FromRequest, HttpRequest};
    use futures::future::{err, ok, Ready};
    use qstring::QString;

    use super::{LibraryDatabase, PROFILES};

    fn from_query(req: &HttpRequest) -> Result<String, actix_web::Error> {
        let query_string = req.query_string();
        let query: Vec<_> = QString::from(query_string).into();
        let profile = query
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("moosicboxProfile"))
            .map(|(_, value)| value);

        let Some(profile) = profile else {
            return Err(ErrorBadRequest("Missing moosicboxProfile query param"));
        };

        Ok(profile.to_owned())
    }

    fn from_header(req: &HttpRequest) -> Result<&str, actix_web::Error> {
        let Some(profile_header_value) = req.headers().get("moosicbox-profile") else {
            return Err(ErrorBadRequest("Missing moosicbox-profile header"));
        };
        let Ok(profile) = profile_header_value.to_str() else {
            return Err(ErrorBadRequest("Invalid moosicbox-profile header"));
        };

        Ok(profile)
    }

    pub struct ProfileName(pub String);

    impl From<ProfileName> for String {
        fn from(value: ProfileName) -> Self {
            value.0
        }
    }

    impl ProfileName {
        /// # Errors
        ///
        /// Will error if request is missing profile header and query param
        pub fn from_request_inner(req: &HttpRequest) -> Result<Self, actix_web::Error> {
            let profile =
                from_query(req).or_else(|_| from_header(req).map(std::borrow::ToOwned::to_owned));

            let profile = match profile {
                Ok(profile) => profile,
                Err(e) => {
                    return Err(e);
                }
            };

            Ok(Self(profile))
        }
    }

    impl FromRequest for ProfileName {
        type Error = actix_web::Error;
        type Future = Ready<Result<Self, actix_web::Error>>;

        fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
            match Self::from_request_inner(req) {
                Ok(x) => ok(x),
                Err(x) => err(x),
            }
        }
    }

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
