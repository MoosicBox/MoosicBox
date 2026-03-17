//! Music library scanning operations.
//!
//! This module provides functions for enabling, checking, and performing
//! music library scans for different music API sources.

use moosicbox_music_api::{MusicApi, SourceToMusicApi as _, profiles::PROFILES};
use switchy::database::profiles::LibraryDatabase;

/// Enables scanning for the music API's source in the library database.
///
/// This function marks the music source as enabled for scanning, allowing
/// the library to be indexed and synchronized.
///
/// # Errors
///
/// * Returns [`moosicbox_music_api::Error::Other`] if enabling the scan origin fails
///   due to a database-level error.
pub async fn enable_scan(
    music_api: &dyn MusicApi,
    db: &LibraryDatabase,
) -> Result<(), moosicbox_music_api::Error> {
    moosicbox_scan::enable_scan_origin(db, &music_api.source().into())
        .await
        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
}

/// Checks whether scanning is enabled for the music API's source.
///
/// Returns `true` if the music source is configured to be scanned,
/// `false` otherwise.
///
/// # Errors
///
/// * Returns [`moosicbox_music_api::Error::Other`] if reading the scan-origin status
///   fails due to a database-level error.
pub async fn scan_enabled(
    music_api: &dyn MusicApi,
    db: &LibraryDatabase,
) -> Result<bool, moosicbox_music_api::Error> {
    moosicbox_scan::is_scan_origin_enabled(db, &music_api.source().into())
        .await
        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
}

/// Performs a music library scan for the music API's source.
///
/// This function scans the music API's library and updates the local database
/// with tracks, albums, artists, and other metadata from the source. It requires
/// the user to be authenticated for sources that need authentication.
///
/// # Errors
///
/// * Returns [`moosicbox_music_api::Error::Unauthorized`] when the source requires
///   authentication and the current auth session is not logged in.
/// * Returns [`moosicbox_music_api::Error::Other`] wrapping
///   [`moosicbox_music_api::Error::MusicApiNotFound`] when no music API exists for
///   the current source/profile combination.
/// * Returns [`moosicbox_music_api::Error::Other`] when building the scanner fails,
///   such as from database access errors.
/// * Returns [`moosicbox_music_api::Error::Other`] when the underlying scan operation
///   fails.
///
/// # Examples
///
/// ```no_run
/// # async fn run(
/// #     music_api: &dyn moosicbox_music_api::MusicApi,
/// #     db: &switchy::database::profiles::LibraryDatabase,
/// # ) -> Result<(), moosicbox_music_api::Error> {
/// moosicbox_music_api_helpers::scan::scan(music_api, db).await?;
/// # Ok(())
/// # }
/// ```
///
/// # Panics
///
/// * Panics if the hard-coded `master` profile is missing from
///   [`moosicbox_music_api::profiles::PROFILES`].
pub async fn scan(
    music_api: &dyn MusicApi,
    db: &LibraryDatabase,
) -> Result<(), moosicbox_music_api::Error> {
    const PROFILE: &str = "master";

    if let Some(auth) = music_api.auth()
        && !auth.is_logged_in().await?
    {
        return Err(moosicbox_music_api::Error::Unauthorized);
    }

    let source = music_api.source();

    let music_api = PROFILES.get(PROFILE).unwrap().get(source).ok_or_else(|| {
        moosicbox_music_api::Error::Other(Box::new(moosicbox_music_api::Error::MusicApiNotFound(
            source.clone(),
        )))
    })?;

    let scanner = moosicbox_scan::Scanner::from_origin(db, source.into())
        .await
        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

    scanner
        .scan_music_api(&**music_api, db)
        .await
        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

    Ok(())
}
