//! Profile management for `MoosicBox`.
//!
//! This crate provides a global registry for managing profile names, allowing applications
//! to track and switch between different user profiles or configurations.
//!
//! # Features
//!
//! * `events` - Enables profile update event listeners for reacting to profile changes
//! * `api` - Provides actix-web extractors for HTTP requests with profile information
//!
//! # Example
//!
//! ```rust
//! use moosicbox_profiles::PROFILES;
//!
//! // Add a profile to the global registry
//! PROFILES.add("user1".to_string());
//!
//! // Retrieve a profile
//! if let Some(profile) = PROFILES.get("user1") {
//!     println!("Found profile: {}", profile);
//! }
//!
//! // List all profiles
//! let all_profiles = PROFILES.names();
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeSet,
    sync::{Arc, LazyLock, RwLock},
};

/// Profile update event handling.
///
/// Provides listeners and event triggers for profile additions and removals.
#[cfg(feature = "events")]
pub mod events;

/// Global instance of the profiles registry.
pub static PROFILES: LazyLock<Profiles> = LazyLock::new(Profiles::default);

/// Registry for managing profile names.
#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct Profiles {
    profiles: Arc<RwLock<BTreeSet<String>>>,
}

impl Profiles {
    /// Adds a profile to the registry.
    ///
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    pub fn add(&self, profile: String) {
        self.profiles.write().unwrap().insert(profile);
    }

    /// Removes a profile from the registry.
    ///
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    pub fn remove(&self, profile: &str) {
        self.profiles.write().unwrap().retain(|p| p != profile);
    }

    /// Adds a profile to the registry and returns it.
    ///
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned or the profile somehow wasn't added to the list of
    /// profiles
    #[must_use]
    pub fn add_fetch(&self, profile: &str) -> String {
        self.add(profile.to_owned());
        self.get(profile).unwrap()
    }

    /// Retrieves a profile from the registry.
    ///
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn get(&self, profile: &str) -> Option<String> {
        self.profiles
            .read()
            .unwrap()
            .iter()
            .find_map(|p| if p == profile { Some(p.clone()) } else { None })
    }

    /// Returns all profile names in the registry.
    ///
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.profiles.read().unwrap().iter().cloned().collect()
    }
}

/// actix-web integration for profile extraction from HTTP requests.
///
/// Provides extractors for retrieving profile names from HTTP request headers
/// (`moosicbox-profile`) or query parameters (`moosicboxProfile`).
///
/// # Example
///
/// ```rust,ignore
/// use actix_web::{web, HttpResponse};
/// use moosicbox_profiles::api::ProfileName;
///
/// async fn handler(profile: ProfileName) -> HttpResponse {
///     HttpResponse::Ok().body(format!("Profile: {}", profile.as_ref()))
/// }
/// ```
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

    /// Profile name extracted from request without verification.
    ///
    /// This struct extracts a profile name from either HTTP request headers
    /// (`moosicbox-profile`) or query parameters (`moosicboxProfile`) but does not
    /// verify that the profile exists in the registry.
    ///
    /// Use [`ProfileName`] if you need to ensure the profile exists before proceeding.
    #[derive(Debug)]
    pub struct ProfileNameUnverified(
        /// The profile name string extracted from the request.
        pub String,
    );

    impl From<ProfileNameUnverified> for String {
        fn from(value: ProfileNameUnverified) -> Self {
            value.0
        }
    }

    impl ProfileNameUnverified {
        /// Extracts profile name from request headers or query parameters.
        ///
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

    /// Verified profile name extracted from request.
    ///
    /// This struct extracts a profile name from either HTTP request headers
    /// (`moosicbox-profile`) or query parameters (`moosicboxProfile`) and verifies
    /// that the profile exists in the global [`PROFILES`] registry.
    ///
    /// If you don't need verification, use [`ProfileNameUnverified`] instead.
    pub struct ProfileName(
        /// The verified profile name string that exists in the registry.
        pub String,
    );

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
        /// Extracts and verifies profile name from request.
        ///
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
