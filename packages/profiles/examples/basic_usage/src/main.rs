#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `moosicbox_profiles`.
//!
//! This example demonstrates the core functionality of the profiles registry:
//! - Adding profiles to the global registry
//! - Retrieving profiles by name
//! - Listing all registered profiles
//! - Removing profiles from the registry

use moosicbox_profiles::PROFILES;

fn main() {
    println!("=== MoosicBox Profiles - Basic Usage Example ===\n");

    // Step 1: Add profiles to the global registry
    println!("1. Adding profiles to the registry...");
    PROFILES.add("user1".to_string());
    PROFILES.add("user2".to_string());
    PROFILES.add("admin".to_string());
    println!("   Added: user1, user2, admin\n");

    // Step 2: Retrieve a specific profile
    println!("2. Retrieving a specific profile...");
    match PROFILES.get("user1") {
        Some(profile) => println!("   Found profile: {profile}"),
        None => println!("   Profile not found"),
    }
    println!();

    // Step 3: Try to retrieve a non-existent profile
    println!("3. Attempting to retrieve non-existent profile...");
    match PROFILES.get("nonexistent") {
        Some(profile) => println!("   Found profile: {profile}"),
        None => println!("   Profile 'nonexistent' not found (as expected)"),
    }
    println!();

    // Step 4: List all registered profiles
    println!("4. Listing all registered profiles...");
    let all_profiles = PROFILES.names();
    println!("   Total profiles: {}", all_profiles.len());
    for profile in &all_profiles {
        println!("   - {profile}");
    }
    println!();

    // Step 5: Add and fetch a profile in one operation
    println!("5. Adding and fetching a profile in one operation...");
    let profile = PROFILES.add_fetch("guest");
    println!("   Added and retrieved: {profile}\n");

    // Step 6: Verify the new profile is in the list
    println!("6. Verifying all profiles after addition...");
    let updated_profiles = PROFILES.names();
    println!("   Total profiles: {}", updated_profiles.len());
    for profile in &updated_profiles {
        println!("   - {profile}");
    }
    println!();

    // Step 7: Remove a profile
    println!("7. Removing profile 'user2'...");
    PROFILES.remove("user2");
    println!("   Removed: user2\n");

    // Step 8: Verify the profile was removed
    println!("8. Verifying profiles after removal...");
    let final_profiles = PROFILES.names();
    println!("   Total profiles: {}", final_profiles.len());
    for profile in &final_profiles {
        println!("   - {profile}");
    }
    println!();

    // Step 9: Confirm user2 is no longer retrievable
    println!("9. Confirming removed profile is not retrievable...");
    match PROFILES.get("user2") {
        Some(profile) => println!("   ERROR: Still found profile: {profile}"),
        None => println!("   Confirmed: 'user2' is no longer in the registry"),
    }

    println!("\n=== Example completed successfully ===");
}
