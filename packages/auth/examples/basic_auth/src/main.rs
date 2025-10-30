#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic Authentication Example
//!
//! This example demonstrates the core authentication functionality in `moosicbox_auth`:
//! - Client ID and access token generation
//! - Token storage and retrieval from database
//! - Signature token fetching
//!
//! Note: This example uses an in-memory database and simulated server responses
//! for demonstration purposes. In a real application, you would connect to an
//! actual authentication server.

use moosicbox_auth::{fetch_signature_token, get_client_id_and_access_token};
use std::sync::Arc;
use switchy_database::{
    Database, DatabaseValue, config, query::FilterableQuery, turso::TursoDatabase,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("MoosicBox Authentication - Basic Usage Example");
    println!("================================================\n");

    // Step 1: Initialize the database
    println!("1. Setting up in-memory database...");
    let db = TursoDatabase::new(":memory:").await?;
    println!("   ✓ Database created\n");

    // Step 2: Create the client_access_tokens table
    // This table stores client credentials for authentication
    println!("2. Creating client_access_tokens table...");
    db.exec_raw(
        "CREATE TABLE client_access_tokens (
            client_id TEXT NOT NULL,
            token TEXT NOT NULL,
            expires INTEGER,
            updated INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
            PRIMARY KEY (client_id, token)
        )",
    )
    .await?;
    println!("   ✓ Table created\n");

    // Step 3: Initialize the global database configuration
    // This allows moosicbox_auth to access the database
    println!("3. Initializing global database configuration...");
    let db_boxed: Box<dyn switchy_database::Database> = Box::new(db);
    let db_arc = Arc::new(db_boxed);
    config::init(db_arc.clone());
    let config_db = config::ConfigDatabase::from(db_arc);
    println!("   ✓ Database configuration initialized\n");

    // Step 4: Get or create client credentials
    // This is the primary use case - obtaining authentication credentials
    println!("4. Getting client ID and access token...");
    println!("   Note: In this example, client registration will fail since we're");
    println!("   not connecting to a real server. This demonstrates the workflow.");

    // Attempt to get credentials (this will try to register with a fake server)
    let auth_host = "https://example.com/api";
    match get_client_id_and_access_token(&config_db, auth_host).await {
        Ok((client_id, access_token)) => {
            println!("   ✓ Client credentials obtained:");
            println!("     - Client ID: {client_id}");
            println!("     - Access Token: {access_token}\n");

            // Step 5: Fetch signature token (optional)
            // Signature tokens are used for signing requests
            println!("5. Attempting to fetch signature token...");
            match fetch_signature_token(auth_host, &client_id, &access_token).await {
                Ok(Some(signature_token)) => {
                    println!("   ✓ Signature token obtained:");
                    println!("     - Token: {signature_token}\n");
                }
                Ok(None) => {
                    println!("   ℹ No signature token available (server returned none)\n");
                }
                Err(e) => {
                    println!("   ✗ Failed to fetch signature token: {e}");
                    println!("     This is expected when not connected to a real server\n");
                }
            }
        }
        Err(e) => {
            println!("   ✗ Failed to get client credentials: {e}");
            println!("     This is expected in this example since we're not connecting");
            println!("     to a real authentication server.\n");

            // Demonstrate manual token storage
            println!("   Demonstrating manual credential storage...");
            manual_credential_demo(&config_db).await?;
        }
    }

    println!("\n✓ Example completed!");
    println!("\nKey Takeaways:");
    println!("  1. Use get_client_id_and_access_token() to obtain credentials");
    println!("  2. Credentials are automatically stored in the database");
    println!("  3. Subsequent calls return existing credentials if valid");
    println!("  4. Use fetch_signature_token() for request signing");
    println!("  5. All functions work with ConfigDatabase for easy integration");

    Ok(())
}

/// Demonstrates manual credential storage and retrieval
async fn manual_credential_demo(
    db: &switchy_database::config::ConfigDatabase,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("   Creating sample credentials...");
    let sample_client_id = "demo-client-123";
    let sample_token = "demo-token-abc";

    // Insert credentials directly into database
    db.upsert("client_access_tokens")
        .where_eq("client_id", sample_client_id)
        .where_eq("token", sample_token)
        .value("client_id", sample_client_id)
        .value("token", sample_token)
        .value("expires", DatabaseValue::Null)
        .execute_first(&**db)
        .await?;

    println!("   ✓ Sample credentials stored:");
    println!("     - Client ID: {sample_client_id}");
    println!("     - Access Token: {sample_token}\n");

    // Retrieve credentials to verify storage
    println!("   Verifying credential retrieval...");
    let result = db
        .select("client_access_tokens")
        .where_eq("client_id", sample_client_id)
        .execute_first(&**db)
        .await?;

    if let Some(row) = result {
        let retrieved_id = row
            .get("client_id")
            .and_then(|v| v.as_str().map(String::from));
        let retrieved_token = row.get("token").and_then(|v| v.as_str().map(String::from));
        println!("   ✓ Retrieved credentials match:");
        println!(
            "     - Client ID: {}",
            retrieved_id.as_deref().unwrap_or("N/A")
        );
        println!(
            "     - Access Token: {}",
            retrieved_token.as_deref().unwrap_or("N/A")
        );
    } else {
        println!("   ✗ Failed to retrieve credentials");
    }

    Ok(())
}
