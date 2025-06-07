#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeSet,
    sync::{Arc, LazyLock, RwLock},
};

#[cfg(feature = "events")]
pub mod events;

pub static PROFILES: LazyLock<Profiles> = LazyLock::new(Profiles::default);

#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct Profiles {
    profiles: Arc<RwLock<BTreeSet<String>>>,
}

impl Profiles {
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    pub fn add(&self, profile: String) {
        self.profiles.write().unwrap().insert(profile);
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    pub fn remove(&self, profile: &str) {
        self.profiles.write().unwrap().retain(|p| p != profile);
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned or the profile somehow wasn't added to the list of
    /// profiles
    #[must_use]
    pub fn add_fetch(&self, profile: &str) -> String {
        self.add(profile.to_owned());
        self.get(profile).unwrap()
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn get(&self, profile: &str) -> Option<String> {
        self.profiles.read().unwrap().iter().find_map(|p| {
            if p == profile {
                Some(p.to_string())
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
        self.profiles.read().unwrap().iter().cloned().collect()
    }
}

#[cfg(feature = "api")]
pub mod api {
    use actix_web::{FromRequest, HttpRequest, dev::Payload, error::ErrorBadRequest};
    use futures::future::{Ready, err, ok};
    use qstring::QString;

    use crate::PROFILES;

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

    #[derive(Debug)]
    pub struct ProfileNameUnverified(pub String);

    impl From<ProfileNameUnverified> for String {
        fn from(value: ProfileNameUnverified) -> Self {
            value.0
        }
    }

    impl ProfileNameUnverified {
        /// # Errors
        ///
        /// Will error if request is missing profile header and query param
        pub fn from_request_inner(req: &HttpRequest) -> Result<Self, actix_web::Error> {
            from_query(req)
                .or_else(|_| from_header(req).map(std::borrow::ToOwned::to_owned))
                .map(Self)
        }
    }

    impl FromRequest for ProfileNameUnverified {
        type Error = actix_web::Error;
        type Future = Ready<Result<Self, actix_web::Error>>;

        fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
            match Self::from_request_inner(req) {
                Ok(x) => ok(x),
                Err(x) => err(x),
            }
        }
    }

    pub struct ProfileName(pub String);

    impl From<ProfileName> for String {
        fn from(value: ProfileName) -> Self {
            value.0
        }
    }

    impl AsRef<str> for ProfileName {
        fn as_ref(&self) -> &str {
            &self.0
        }
    }

    impl ProfileName {
        /// # Errors
        ///
        /// Will error if request is missing profile header and query param or if the
        /// profile doesn't exist
        pub fn from_request_inner(req: &HttpRequest) -> Result<Self, actix_web::Error> {
            let profile = match ProfileNameUnverified::from_request_inner(req) {
                Ok(profile) => {
                    let profile = profile.0;
                    if !PROFILES.names().iter().any(|x| x == &profile) {
                        return Err(ErrorBadRequest(format!(
                            "Profile '{profile}' does not exist"
                        )));
                    }
                    profile
                }
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
}
