use moosicbox_music_api::{MusicApi, SourceToMusicApi as _, profiles::PROFILES};
use switchy::database::profiles::LibraryDatabase;

/// # Errors
///
/// * If there was a database error
pub async fn enable_scan(
    music_api: &dyn MusicApi,
    db: &LibraryDatabase,
) -> Result<(), moosicbox_music_api::Error> {
    moosicbox_scan::enable_scan_origin(db, &music_api.source().into())
        .await
        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
}

/// # Errors
///
/// * If there was a database error
pub async fn scan_enabled(
    music_api: &dyn MusicApi,
    db: &LibraryDatabase,
) -> Result<bool, moosicbox_music_api::Error> {
    moosicbox_scan::is_scan_origin_enabled(db, &music_api.source().into())
        .await
        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
}

/// # Errors
///
/// * If there was a database error
/// * If the scan fails
///
/// # Panics
///
/// * If the profile is missing
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
