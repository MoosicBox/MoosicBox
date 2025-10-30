#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! API integration example for `moosicbox_profiles`.
//!
//! This example demonstrates how to use the actix-web integration features:
//! - Using `ProfileName` extractor for verified profiles
//! - Using `ProfileNameUnverified` extractor for unverified profiles
//! - Extracting profiles from HTTP headers
//! - Extracting profiles from query parameters
//! - Handling missing or invalid profile information

use actix_web::{App, HttpResponse, HttpServer, Result, web};
use moosicbox_profiles::{
    PROFILES,
    api::{ProfileName, ProfileNameUnverified},
};

/// Handler that requires a verified profile.
///
/// The `ProfileName` extractor validates that the profile exists in the registry.
/// If the profile is missing or doesn't exist, returns 400 Bad Request.
async fn verified_handler(profile: ProfileName) -> Result<HttpResponse> {
    let profile_name = profile.as_ref();
    println!("Verified handler called with profile: {profile_name}");
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "success",
        "profile": profile_name,
        "verified": true,
        "message": format!("Welcome, {profile_name}!")
    })))
}

/// Handler that accepts unverified profiles.
///
/// The `ProfileNameUnverified` extractor extracts the profile name from the request
/// without checking if it exists in the registry.
async fn unverified_handler(profile: ProfileNameUnverified) -> Result<HttpResponse> {
    let profile_name: String = profile.into();
    println!("Unverified handler called with profile: {profile_name}");
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "success",
        "profile": profile_name,
        "verified": false,
        "message": format!("Processing request for profile: {profile_name}")
    })))
}

/// Handler to list all registered profiles.
async fn list_profiles() -> Result<HttpResponse> {
    let profiles = PROFILES.names();
    println!("Listing all profiles: {} found", profiles.len());
    Ok(HttpResponse::Ok().json(serde_json::json!({
        "status": "success",
        "count": profiles.len(),
        "profiles": profiles
    })))
}

/// Handler to register a new profile.
async fn register_profile(profile: ProfileNameUnverified) -> Result<HttpResponse> {
    let profile_name: String = profile.into();
    println!("Registering new profile: {profile_name}");

    // Add the profile to the registry
    PROFILES.add(profile_name.clone());

    Ok(HttpResponse::Created().json(serde_json::json!({
        "status": "success",
        "message": "Profile registered successfully",
        "profile": profile_name
    })))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("=== MoosicBox Profiles - API Integration Example ===\n");

    // Pre-register some profiles for testing
    println!("Pre-registering test profiles...");
    PROFILES.add("alice".to_string());
    PROFILES.add("bob".to_string());
    PROFILES.add("admin".to_string());
    println!("Registered: alice, bob, admin\n");

    println!("Starting HTTP server on http://127.0.0.1:8080");
    println!("\nAvailable endpoints:");
    println!("  GET  /verified   - Requires verified profile (must exist in registry)");
    println!("  GET  /unverified - Accepts any profile name");
    println!("  GET  /profiles   - Lists all registered profiles");
    println!("  POST /register   - Registers a new profile");
    println!("\nProfile can be provided via:");
    println!("  - Header: moosicbox-profile: <name>");
    println!("  - Query param: ?moosicboxProfile=<name>");
    println!("\nExample requests:");
    println!("  curl -H 'moosicbox-profile: alice' http://127.0.0.1:8080/verified");
    println!("  curl 'http://127.0.0.1:8080/verified?moosicboxProfile=alice'");
    println!("  curl 'http://127.0.0.1:8080/unverified?moosicboxProfile=newuser'");
    println!("  curl http://127.0.0.1:8080/profiles");
    println!("  curl -X POST -H 'moosicbox-profile: charlie' http://127.0.0.1:8080/register");
    println!("\nPress Ctrl+C to stop the server\n");

    HttpServer::new(|| {
        App::new()
            .route("/verified", web::get().to(verified_handler))
            .route("/unverified", web::get().to(unverified_handler))
            .route("/profiles", web::get().to(list_profiles))
            .route("/register", web::post().to(register_profile))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
