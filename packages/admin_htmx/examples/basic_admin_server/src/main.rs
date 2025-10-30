#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic Admin Server Example
//!
//! This example demonstrates how to set up a minimal web server with the
//! `MoosicBox` admin HTMX interface. It shows how to configure the required
//! database connections and integrate the admin endpoints into an Actix-web
//! application.

use actix_web::{App, HttpServer, middleware, web};
use moosicbox_admin_htmx::api;
use std::sync::Arc;
use switchy_database::{Database, config, profiles::PROFILES, turso::TursoDatabase};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Initialize the logger to see request logs
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("🎵 MoosicBox Admin Example Server");
    println!("=================================\n");

    // Step 1: Set up the configuration database
    // This database stores server-wide settings
    println!("Setting up configuration database...");
    let config_db_path = "config.db";
    let config_db = TursoDatabase::new(config_db_path)
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    // Register the config database globally so it can be accessed by endpoints
    config::init(Arc::new(Box::new(config_db) as Box<dyn Database>));

    println!("  ✓ Configuration database created at: {config_db_path}");

    // Step 2: Set up a default profile database
    // Profiles allow managing multiple library configurations
    println!("\nSetting up default profile database...");
    let default_profile = "default";
    let profile_db_path = "default_profile.db";

    let profile_db = TursoDatabase::new(profile_db_path)
        .await
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    // Register the profile database with the PROFILES manager
    PROFILES.add(
        default_profile.to_string(),
        Arc::new(Box::new(profile_db) as Box<dyn Database>),
    );

    println!("  ✓ Profile '{default_profile}' created at: {profile_db_path}");

    // Step 3: Start the HTTP server
    println!("\nStarting HTTP server...");
    let server_addr = "127.0.0.1:8080";

    println!("\n✅ Server ready!");
    println!("📍 Admin interface: http://{server_addr}/admin");
    println!("\nPress Ctrl+C to stop the server\n");

    // Create and run the Actix-web server
    HttpServer::new(move || {
        App::new()
            // Add logging middleware to see incoming requests
            .wrap(middleware::Logger::default())
            // Bind all admin endpoints under the /admin path
            .service(api::bind_services(web::scope("/admin")))
    })
    .bind(server_addr)?
    .run()
    .await
}
