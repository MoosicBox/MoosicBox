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

    /// Extracts profile name from query parameters.
    ///
    /// # Errors
    ///
    /// * Missing `moosicboxProfile` query parameter
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

    /// Extracts profile name from HTTP headers.
    ///
    /// # Errors
    ///
    /// * Missing `moosicbox-profile` header
    /// * Invalid UTF-8 in `moosicbox-profile` header value
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
        /// * Missing both `moosicbox-profile` header and `moosicboxProfile` query parameter
        /// * Invalid UTF-8 in `moosicbox-profile` header value
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
        /// * Missing both `moosicbox-profile` header and `moosicboxProfile` query parameter
        /// * Invalid UTF-8 in `moosicbox-profile` header value
        /// * Profile name not found in the global registry
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_add_and_get_profile() {
        let profiles = Profiles::default();
        profiles.add("test_profile".to_string());

        let result = profiles.get("test_profile");
        assert_eq!(result, Some("test_profile".to_string()));
    }

    #[test_log::test]
    fn test_get_nonexistent_profile() {
        let profiles = Profiles::default();
        let result = profiles.get("nonexistent");
        assert_eq!(result, None);
    }

    #[test_log::test]
    fn test_remove_profile() {
        let profiles = Profiles::default();
        profiles.add("to_remove".to_string());

        assert!(profiles.get("to_remove").is_some());

        profiles.remove("to_remove");

        assert!(profiles.get("to_remove").is_none());
    }

    #[test_log::test]
    fn test_remove_nonexistent_profile() {
        let profiles = Profiles::default();
        // Should not panic when removing a profile that doesn't exist
        profiles.remove("nonexistent");
        assert!(profiles.get("nonexistent").is_none());
    }

    #[test_log::test]
    fn test_add_fetch() {
        let profiles = Profiles::default();
        let result = profiles.add_fetch("test_fetch");

        assert_eq!(result, "test_fetch");
        assert_eq!(profiles.get("test_fetch"), Some("test_fetch".to_string()));
    }

    #[test_log::test]
    fn test_names_returns_all_profiles() {
        let profiles = Profiles::default();
        profiles.add("profile1".to_string());
        profiles.add("profile2".to_string());
        profiles.add("profile3".to_string());

        let names = profiles.names();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"profile1".to_string()));
        assert!(names.contains(&"profile2".to_string()));
        assert!(names.contains(&"profile3".to_string()));
    }

    #[test_log::test]
    fn test_names_empty_when_no_profiles() {
        let profiles = Profiles::default();
        let names = profiles.names();
        assert!(names.is_empty());
    }

    #[test_log::test]
    fn test_profile_names_are_case_sensitive() {
        let profiles = Profiles::default();
        profiles.add("TestProfile".to_string());

        assert!(profiles.get("TestProfile").is_some());
        assert!(profiles.get("testprofile").is_none());
        assert!(profiles.get("TESTPROFILE").is_none());
    }

    #[test_log::test]
    fn test_duplicate_profile_add() {
        let profiles = Profiles::default();
        profiles.add("duplicate".to_string());
        profiles.add("duplicate".to_string());

        let names = profiles.names();
        // BTreeSet should deduplicate
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], "duplicate");
    }

    #[test_log::test]
    fn test_concurrent_reads() {
        use std::sync::Arc;
        use std::thread;

        let profiles = Arc::new(Profiles::default());
        profiles.add("concurrent_test".to_string());

        let mut handles = vec![];
        for _ in 0..10 {
            let profiles_clone = Arc::clone(&profiles);
            let handle = thread::spawn(move || {
                for _ in 0..100 {
                    let result = profiles_clone.get("concurrent_test");
                    assert_eq!(result, Some("concurrent_test".to_string()));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test_log::test]
    fn test_concurrent_adds() {
        use std::sync::Arc;
        use std::thread;

        let profiles = Arc::new(Profiles::default());
        let mut handles = vec![];

        for i in 0..10 {
            let profiles_clone = Arc::clone(&profiles);
            let handle = thread::spawn(move || {
                for j in 0..10 {
                    profiles_clone.add(format!("profile_{i}_{j}"));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Should have 100 unique profiles
        let names = profiles.names();
        assert_eq!(names.len(), 100);
    }

    #[test_log::test]
    fn test_concurrent_mixed_operations() {
        use std::sync::Arc;
        use std::thread;

        let profiles = Arc::new(Profiles::default());
        // Pre-populate some profiles
        for i in 0..10 {
            profiles.add(format!("initial_{i}"));
        }

        let mut handles = vec![];

        // Readers
        for _ in 0..5 {
            let profiles_clone = Arc::clone(&profiles);
            let handle = thread::spawn(move || {
                for i in 0..10 {
                    let _ = profiles_clone.get(&format!("initial_{i}"));
                }
            });
            handles.push(handle);
        }

        // Writers
        for i in 0..5 {
            let profiles_clone = Arc::clone(&profiles);
            let handle = thread::spawn(move || {
                for j in 0..10 {
                    profiles_clone.add(format!("new_{i}_{j}"));
                }
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let names = profiles.names();
        // Should have 10 initial + 50 new = 60 profiles
        assert_eq!(names.len(), 60);
    }

    #[cfg(feature = "api")]
    mod api_tests {
        use super::*;
        use actix_web::test::TestRequest;

        #[test_log::test]
        fn test_profile_name_unverified_from_query() {
            let req = TestRequest::default()
                .uri("/?moosicboxProfile=unverified_profile")
                .to_http_request();

            let result = super::super::api::ProfileNameUnverified::from_request_inner(&req);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().0, "unverified_profile");
        }

        #[test_log::test]
        fn test_profile_name_unverified_from_query_case_insensitive() {
            let test_cases = vec![
                "/?moosicboxProfile=test",
                "/?MoosicboxProfile=test",
                "/?MOOSICBOXPROFILE=test",
                "/?moosicboxprofile=test",
            ];

            for uri in test_cases {
                let req = TestRequest::default().uri(uri).to_http_request();
                let result = super::super::api::ProfileNameUnverified::from_request_inner(&req);
                assert!(result.is_ok(), "Failed for URI: {uri}");
                assert_eq!(result.unwrap().0, "test");
            }
        }

        #[test_log::test]
        fn test_profile_name_unverified_missing_both() {
            let req = TestRequest::default()
                .uri("/?other=value")
                .to_http_request();
            let result = super::super::api::ProfileNameUnverified::from_request_inner(&req);
            assert!(result.is_err());
        }

        #[test_log::test]
        fn test_profile_name_unverified_from_header() {
            let req = TestRequest::default()
                .insert_header(("moosicbox-profile", "header_profile"))
                .to_http_request();

            let result = super::super::api::ProfileNameUnverified::from_request_inner(&req);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().0, "header_profile");
        }

        #[test_log::test]
        fn test_profile_name_unverified_prefers_query_over_header() {
            let req = TestRequest::default()
                .uri("/?moosicboxProfile=query_profile")
                .insert_header(("moosicbox-profile", "header_profile"))
                .to_http_request();

            let result = super::super::api::ProfileNameUnverified::from_request_inner(&req);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().0, "query_profile");
        }

        #[test_log::test]
        fn test_profile_name_unverified_fallback_to_header() {
            let req = TestRequest::default()
                .insert_header(("moosicbox-profile", "header_profile"))
                .to_http_request();

            let result = super::super::api::ProfileNameUnverified::from_request_inner(&req);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().0, "header_profile");
        }

        #[test_log::test]
        fn test_profile_name_verified_with_existing_profile() {
            PROFILES.add("verified_test".to_string());

            let req = TestRequest::default()
                .uri("/?moosicboxProfile=verified_test")
                .to_http_request();

            let result = super::super::api::ProfileName::from_request_inner(&req);
            assert!(result.is_ok());
            assert_eq!(result.unwrap().0, "verified_test");

            // Cleanup
            PROFILES.remove("verified_test");
        }

        #[test_log::test]
        fn test_profile_name_verified_with_nonexistent_profile() {
            let req = TestRequest::default()
                .uri("/?moosicboxProfile=nonexistent")
                .to_http_request();

            let result = super::super::api::ProfileName::from_request_inner(&req);
            assert!(result.is_err());
        }

        #[test_log::test]
        fn test_profile_name_as_ref() {
            let profile = super::super::api::ProfileName("test".to_string());
            let as_str: &str = profile.as_ref();
            assert_eq!(as_str, "test");
        }

        #[test_log::test]
        fn test_profile_name_into_string() {
            let profile = super::super::api::ProfileName("test".to_string());
            let string: String = profile.into();
            assert_eq!(string, "test");
        }

        #[test_log::test]
        fn test_profile_name_unverified_into_string() {
            let profile = super::super::api::ProfileNameUnverified("test".to_string());
            let string: String = profile.into();
            assert_eq!(string, "test");
        }
    }
}
