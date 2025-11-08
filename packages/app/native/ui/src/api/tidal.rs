//! Tidal API integration functions.
//!
//! Provides utilities for managing Tidal authentication and library scanning.

use moosicbox_tidal::{device_authorization, device_authorization_token, db::models::TidalConfig};
use switchy_database::profiles::LibraryDatabase;
use log::error;

/// Retrieves the stored Tidal configuration from the database.
///
/// Returns `None` if the configuration cannot be retrieved or does not exist.
pub async fn get_tidal_config(db: &LibraryDatabase) -> Option<TidalConfig> {
    match moosicbox_tidal::db::get_tidal_config(db).await {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to get Tidal config: {}", e);
            None
        }
    }
}

/// Initiates the Tidal device authorization flow.
///
/// Returns a URL that the user should visit to complete authentication.
/// Starts a background task that polls for authorization completion.
///
/// Returns `None` if authentication cannot be started due to missing credentials,
/// network errors, or invalid responses from the Tidal API.
pub async fn start_auth(db: &LibraryDatabase) -> Option<String> {
    let client_id = match switchy_env::var("TIDAL_CLIENT_ID") {
        Ok(id) => id,
        Err(e) => {
            error!("Failed to get TIDAL_CLIENT_ID: {}", e);
            return None;
        }
    };

    let response = match device_authorization(client_id, true).await {
        Ok(resp) => resp,
        Err(e) => {
            error!("Failed to start Tidal device authorization: {}", e);
            return None;
        }
    };

    let device_code = match response
        .get("deviceCode")
        .and_then(|v| v.as_str())
    {
        Some(code) => code,
        None => {
            error!("Missing or invalid device code in Tidal response");
            return None;
        }
    };

    let url = match response
        .get("url")
        .and_then(|v| v.as_str())
    {
        Some(url) => url,
        None => {
            error!("Missing or invalid URL in Tidal response");
            return None;
        }
    };

    // Start polling for token
    tokio::spawn({
        let db = db.clone();
        let device_code = device_code.to_string();
        let url = url.to_string();
        async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            for _ in 0..60 {
                interval.tick().await;
                if let Ok(response) = device_authorization_token(
                    &db,
                    switchy_env::var("TIDAL_CLIENT_ID").unwrap(),
                    switchy_env::var("TIDAL_CLIENT_SECRET").unwrap(),
                    device_code.clone(),
                    Some(true),
                ).await {
                    if response.get("accessToken").is_some() {
                        break;
                    }
                }
            }
        }
    });

    Some(url.to_string())
}

/// Runs a library scan for Tidal content.
///
/// Returns `true` if the scan completed successfully, `false` otherwise.
pub async fn run_scan(db: &LibraryDatabase) -> bool {
    match moosicbox_scan::run_scan(
        Some(vec![moosicbox_scan::ScanOrigin::Tidal]),
        db,
        moosicbox_music_api::MusicApis::default(),
    ).await {
        Ok(_) => true,
        Err(e) => {
            error!("Failed to run Tidal scan: {}", e);
            false
        }
    }
}
