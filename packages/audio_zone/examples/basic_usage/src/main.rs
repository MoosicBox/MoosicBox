#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `moosicbox_audio_zone`
//!
//! This example demonstrates the fundamental operations for managing audio zones:
//! - Creating an in-memory database for testing
//! - Creating audio zones
//! - Listing audio zones
//! - Retrieving specific zones
//! - Updating zones
//! - Deleting zones

use std::sync::Arc;

use moosicbox_audio_zone::{
    create_audio_zone, delete_audio_zone, get_zone, models::CreateAudioZone,
    models::UpdateAudioZone, update_audio_zone, zones,
};
use switchy_database::{
    Database, DatabaseValue,
    config::ConfigDatabase,
    schema::{Column, DataType, create_table},
    simulator::SimulationDatabase,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize the logger to see debug output
    env_logger::init();

    println!("=== MoosicBox Audio Zone Basic Usage Example ===\n");

    // Step 1: Create an in-memory database for testing
    println!("Step 1: Creating in-memory database...");
    let db: Box<dyn Database> = Box::new(SimulationDatabase::new()?);
    let db_arc = Arc::new(db);

    // Initialize the schema
    create_schema(&db_arc).await?;
    println!("Database schema created.\n");

    // Convert to ConfigDatabase for the audio_zone API
    let config_db: ConfigDatabase = db_arc.into();

    // Step 2: Create a new audio zone
    println!("Step 2: Creating a new audio zone...");
    let new_zone = CreateAudioZone {
        name: "Living Room".to_string(),
    };

    let created_zone = create_audio_zone(&config_db, &new_zone).await?;
    println!(
        "Created audio zone: '{}' (ID: {})\n",
        created_zone.name, created_zone.id
    );

    // Step 3: Create another audio zone
    println!("Step 3: Creating another audio zone...");
    let kitchen_zone = CreateAudioZone {
        name: "Kitchen".to_string(),
    };

    let kitchen = create_audio_zone(&config_db, &kitchen_zone).await?;
    println!(
        "Created audio zone: '{}' (ID: {})\n",
        kitchen.name, kitchen.id
    );

    // Step 4: List all audio zones
    println!("Step 4: Listing all audio zones...");
    let all_zones = zones(&config_db).await?;
    println!("Found {} audio zones:", all_zones.len());
    for zone in &all_zones {
        println!(
            "  - {} (ID: {}, {} players)",
            zone.name,
            zone.id,
            zone.players.len()
        );
    }
    println!();

    // Step 5: Get a specific audio zone by ID
    println!("Step 5: Retrieving audio zone by ID...");
    if let Some(zone) = get_zone(&config_db, created_zone.id).await? {
        println!(
            "Retrieved zone: '{}' (ID: {}, {} players)\n",
            zone.name,
            zone.id,
            zone.players.len()
        );
    }

    // Step 6: Update an audio zone
    println!("Step 6: Updating audio zone name...");
    let update = UpdateAudioZone {
        id: created_zone.id,
        name: Some("Updated Living Room".to_string()),
        players: None,
    };

    let updated_zone = update_audio_zone(&config_db, update).await?;
    println!(
        "Updated zone name: '{}' (ID: {})\n",
        updated_zone.name, updated_zone.id
    );

    // Step 7: Verify the update by listing zones again
    println!("Step 7: Verifying update...");
    let all_zones = zones(&config_db).await?;
    for zone in &all_zones {
        println!("  - {} (ID: {})", zone.name, zone.id);
    }
    println!();

    // Step 8: Delete an audio zone
    println!("Step 8: Deleting audio zone...");
    if let Some(deleted) = delete_audio_zone(&config_db, kitchen.id).await? {
        println!("Deleted zone: '{}' (ID: {})\n", deleted.name, deleted.id);
    }

    // Step 9: List zones after deletion
    println!("Step 9: Listing zones after deletion...");
    let remaining_zones = zones(&config_db).await?;
    println!("Remaining zones: {}", remaining_zones.len());
    for zone in &remaining_zones {
        println!("  - {} (ID: {})", zone.name, zone.id);
    }

    println!("\n=== Example completed successfully! ===");

    Ok(())
}

/// Creates the required database schema for audio zones
///
/// This creates the `audio_zones` table and associated tables
/// needed for audio zone management.
async fn create_schema(db: &Arc<Box<dyn Database>>) -> Result<(), Box<dyn std::error::Error>> {
    // Create audio_zones table
    create_table("audio_zones")
        .column(Column {
            name: "id".to_string(),
            nullable: false,
            auto_increment: true,
            data_type: DataType::BigInt,
            default: None,
        })
        .column(Column {
            name: "name".to_string(),
            nullable: false,
            auto_increment: false,
            data_type: DataType::Text,
            default: None,
        })
        .primary_key("id")
        .execute(&***db)
        .await?;

    // Create players table (referenced by audio zones)
    create_table("players")
        .column(Column {
            name: "id".to_string(),
            nullable: false,
            auto_increment: true,
            data_type: DataType::BigInt,
            default: None,
        })
        .column(Column {
            name: "audio_output_id".to_string(),
            nullable: false,
            auto_increment: false,
            data_type: DataType::Text,
            default: None,
        })
        .column(Column {
            name: "name".to_string(),
            nullable: false,
            auto_increment: false,
            data_type: DataType::Text,
            default: None,
        })
        .column(Column {
            name: "playing".to_string(),
            nullable: false,
            auto_increment: false,
            data_type: DataType::Bool,
            default: Some(DatabaseValue::Bool(false)),
        })
        .column(Column {
            name: "created".to_string(),
            nullable: false,
            auto_increment: false,
            data_type: DataType::Text,
            default: Some(DatabaseValue::Now),
        })
        .column(Column {
            name: "updated".to_string(),
            nullable: false,
            auto_increment: false,
            data_type: DataType::Text,
            default: Some(DatabaseValue::Now),
        })
        .primary_key("id")
        .execute(&***db)
        .await?;

    // Create audio_zone_players junction table for many-to-many relationship
    create_table("audio_zone_players")
        .column(Column {
            name: "audio_zone_id".to_string(),
            nullable: false,
            auto_increment: false,
            data_type: DataType::BigInt,
            default: None,
        })
        .column(Column {
            name: "player_id".to_string(),
            nullable: false,
            auto_increment: false,
            data_type: DataType::BigInt,
            default: None,
        })
        .execute(&***db)
        .await?;

    Ok(())
}
