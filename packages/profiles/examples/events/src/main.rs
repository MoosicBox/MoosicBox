#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Events example for `moosicbox_profiles`.
//!
//! This example demonstrates the event system for profile updates:
//! - Registering event listeners for profile changes
//! - Triggering profile update events
//! - Handling added and removed profiles in listeners
//! - Using multiple concurrent listeners

use moosicbox_profiles::{
    PROFILES,
    events::{on_profiles_updated_event, trigger_profiles_updated_event},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::time::{Duration, sleep};

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MoosicBox Profiles - Events Example ===\n");

    // Counter to track event invocations
    let event_count = Arc::new(AtomicUsize::new(0));
    let event_count_clone = Arc::clone(&event_count);

    // Step 1: Register a simple event listener
    println!("1. Registering first event listener...");
    on_profiles_updated_event(move |added, removed| {
        let count = event_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
        let added = added.to_vec();
        let removed = removed.to_vec();
        async move {
            println!("\n[Listener 1 - Event #{count}]");
            if !added.is_empty() {
                println!("  Profiles added: {added:?}");
            }
            if !removed.is_empty() {
                println!("  Profiles removed: {removed:?}");
            }
            Ok(())
        }
    })
    .await;
    println!("   ✓ Listener 1 registered\n");

    // Step 2: Register a second listener to demonstrate multiple listeners
    println!("2. Registering second event listener...");
    on_profiles_updated_event(|added, removed| {
        let added = added.to_vec();
        let removed = removed.to_vec();
        async move {
            println!("\n[Listener 2]");
            println!(
                "  Total changes: {} added, {} removed",
                added.len(),
                removed.len()
            );

            // Simulate some async work
            sleep(Duration::from_millis(100)).await;

            println!("  ✓ Listener 2 processing complete");
            Ok(())
        }
    })
    .await;
    println!("   ✓ Listener 2 registered\n");

    // Step 3: Register a listener that tracks cumulative changes
    let total_added = Arc::new(AtomicUsize::new(0));
    let total_removed = Arc::new(AtomicUsize::new(0));
    let total_added_clone = Arc::clone(&total_added);
    let total_removed_clone = Arc::clone(&total_removed);

    println!("3. Registering statistics listener...");
    on_profiles_updated_event(move |added, removed| {
        let added_count = added.len();
        let removed_count = removed.len();
        let total_added = Arc::clone(&total_added_clone);
        let total_removed = Arc::clone(&total_removed_clone);

        async move {
            total_added.fetch_add(added_count, Ordering::SeqCst);
            total_removed.fetch_add(removed_count, Ordering::SeqCst);

            let cumulative_added = total_added.load(Ordering::SeqCst);
            let cumulative_removed = total_removed.load(Ordering::SeqCst);

            println!("\n[Statistics Listener]");
            println!("  Cumulative totals:");
            println!("    Total added: {cumulative_added}");
            println!("    Total removed: {cumulative_removed}");
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let net_change = cumulative_added as i32 - cumulative_removed as i32;
            println!("    Net change: {net_change}");

            Ok(())
        }
    })
    .await;
    println!("   ✓ Statistics listener registered\n");

    // Give listeners a moment to register
    sleep(Duration::from_millis(50)).await;

    // Step 4: Trigger an event with added profiles
    println!("4. Triggering event: Adding profiles...");
    if let Err(errors) = trigger_profiles_updated_event(
        vec![
            "alice".to_string(),
            "bob".to_string(),
            "charlie".to_string(),
        ],
        vec![],
    )
    .await
    {
        eprintln!("Errors occurred: {} listener(s) failed", errors.len());
        for err in errors {
            eprintln!("  - {err}");
        }
        return Err("Event listeners failed".into());
    }

    // Actually add them to the registry for consistency
    PROFILES.add("alice".to_string());
    PROFILES.add("bob".to_string());
    PROFILES.add("charlie".to_string());

    sleep(Duration::from_millis(200)).await;

    // Step 5: Trigger an event with removed profiles
    println!("\n5. Triggering event: Removing profile 'bob'...");
    if let Err(errors) = trigger_profiles_updated_event(vec![], vec!["bob".to_string()]).await {
        eprintln!("Errors occurred: {} listener(s) failed", errors.len());
        for err in errors {
            eprintln!("  - {err}");
        }
        return Err("Event listeners failed".into());
    }

    PROFILES.remove("bob");

    sleep(Duration::from_millis(200)).await;

    // Step 6: Trigger an event with both additions and removals
    println!("\n6. Triggering event: Mixed changes...");
    if let Err(errors) = trigger_profiles_updated_event(
        vec!["david".to_string(), "eve".to_string()],
        vec!["charlie".to_string()],
    )
    .await
    {
        eprintln!("Errors occurred: {} listener(s) failed", errors.len());
        for err in errors {
            eprintln!("  - {err}");
        }
        return Err("Event listeners failed".into());
    }

    PROFILES.add("david".to_string());
    PROFILES.add("eve".to_string());
    PROFILES.remove("charlie");

    sleep(Duration::from_millis(200)).await;

    // Step 7: Show final state
    println!("\n7. Final profile registry state:");
    let final_profiles = PROFILES.names();
    println!("   Total profiles: {}", final_profiles.len());
    for profile in &final_profiles {
        println!("   - {profile}");
    }

    // Step 8: Summary statistics
    println!("\n8. Event processing summary:");
    println!(
        "   Total events triggered: {}",
        event_count.load(Ordering::SeqCst)
    );
    println!(
        "   Total profiles added: {}",
        total_added.load(Ordering::SeqCst)
    );
    println!(
        "   Total profiles removed: {}",
        total_removed.load(Ordering::SeqCst)
    );

    println!("\n=== Example completed successfully ===");

    Ok(())
}
