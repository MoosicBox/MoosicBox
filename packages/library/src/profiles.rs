use std::sync::{Arc, LazyLock, RwLock};

use moosicbox_database::profiles::LibraryDatabase;

use crate::LibraryMusicApi;

pub static PROFILES: LazyLock<LibraryMusicApiProfiles> =
    LazyLock::new(LibraryMusicApiProfiles::default);

#[allow(clippy::module_name_repetitions)]
#[derive(Default)]
pub struct LibraryMusicApiProfiles {
    profiles: Arc<RwLock<Vec<(String, LibraryMusicApi)>>>,
}

impl LibraryMusicApiProfiles {
    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    pub fn add(&self, profile: String, db: LibraryDatabase) {
        self.profiles
            .write()
            .unwrap()
            .push((profile, LibraryMusicApi { db }));
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned or the profile somehow wasn't added to the list of
    /// profiles
    #[must_use]
    pub fn fetch_add(&self, profile: &str, db: LibraryDatabase) -> LibraryMusicApi {
        self.add(profile.to_owned(), db);
        self.get(profile).unwrap()
    }

    /// # Panics
    ///
    /// Will panic if `RwLock` is poisoned
    #[must_use]
    pub fn get(&self, profile: &str) -> Option<LibraryMusicApi> {
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

#[cfg(feature = "api")]
pub mod api {
    use actix_web::{dev::Payload, error::ErrorBadRequest, FromRequest, HttpRequest};
    use futures::future::{err, ok, Ready};
    use moosicbox_database::profiles::api::ProfileName;

    use super::{LibraryMusicApi, PROFILES};

    impl FromRequest for LibraryMusicApi {
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

            let Some(music_apis) = PROFILES.get(&profile.0) else {
                return err(ErrorBadRequest("Invalid profile"));
            };

            ok(music_apis)
        }
    }
}
