#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `switchy_random`
//!
//! This example demonstrates the core functionality of the `switchy_random` library:
//! - Creating random number generators
//! - Generating random values of different types
//! - Using seeded RNGs for reproducible results
//! - Generating values in specific ranges
//! - Working with probability distributions

fn main() {
    println!("=== Switchy Random - Basic Usage Example ===\n");

    // 1. Basic random number generation
    println!("1. Basic Random Number Generation:");
    basic_random_generation();
    println!();

    // 2. Seeded random generation for reproducibility
    println!("2. Seeded Random Generation (Reproducible):");
    seeded_random_generation();
    println!();

    // 3. Range-based random generation
    println!("3. Range-Based Random Generation:");
    range_based_generation();
    println!();

    // 4. Probability-based generation
    println!("4. Probability-Based Generation:");
    probability_generation();
    println!();

    // 5. Filling buffers with random data
    println!("5. Filling Buffers with Random Data:");
    buffer_filling();
    println!();

    println!("=== Example Complete ===");
}

/// Demonstrates basic random number generation with different types
#[allow(clippy::similar_names)]
fn basic_random_generation() {
    use switchy_random::Rng;

    // Create a new random number generator
    let rng = Rng::new();

    // Generate random integers
    let rand_u32 = rng.next_u32();
    let rand_i32 = rng.next_i32();
    let rand_u64 = rng.next_u64();

    println!("  Random u32: {rand_u32}");
    println!("  Random i32: {rand_i32}");
    println!("  Random u64: {rand_u64}");

    // Generate random floating-point values
    let rand_f32: f32 = rng.random();
    let rand_f64: f64 = rng.random();

    println!("  Random f32: {rand_f32}");
    println!("  Random f64: {rand_f64}");
}

/// Demonstrates seeded random generation for reproducible results
fn seeded_random_generation() {
    use switchy_random::Rng;

    // Create two generators with the same seed
    let seed = 42_u64;
    let rng1 = Rng::from_seed(seed);
    let rng2 = Rng::from_seed(seed);

    // Generate values from first generator
    let value1_a = rng1.next_u32();
    let value1_b = rng1.next_u32();
    let value1_c = rng1.next_u32();

    // Generate values from second generator (should match first)
    let value2_a = rng2.next_u32();
    let value2_b = rng2.next_u32();
    let value2_c = rng2.next_u32();

    println!("  Generator 1 (seed={seed}): {value1_a}, {value1_b}, {value1_c}");
    println!("  Generator 2 (seed={seed}): {value2_a}, {value2_b}, {value2_c}");

    // Verify they match
    assert_eq!(value1_a, value2_a);
    assert_eq!(value1_b, value2_b);
    assert_eq!(value1_c, value2_c);
    println!("  ✓ All values match - reproducible!");
}

/// Demonstrates generating random values within specific ranges
fn range_based_generation() {
    use switchy_random::Rng;

    let rng = Rng::new();

    // Generate a dice roll (1-6)
    let dice_roll = rng.gen_range(1..=6);
    println!("  Dice roll (1-6): {dice_roll}");

    // Generate a percentage (0.0-100.0)
    let percentage = rng.gen_range(0.0..100.0);
    println!("  Percentage (0-100): {percentage:.2}%");

    // Generate a random index for an array of size 10
    let index = rng.gen_range(0..10);
    println!("  Array index (0-9): {index}");

    // Generate multiple dice rolls to show distribution
    print!("  10 dice rolls: ");
    for i in 0..10 {
        let roll = rng.gen_range(1..=6);
        print!("{roll}");
        if i < 9 {
            print!(", ");
        }
    }
    println!();
}

/// Demonstrates probability-based random generation
fn probability_generation() {
    use switchy_random::Rng;

    let rng = Rng::new();

    // Generate a fair coin flip (50% probability)
    let coin_flip = rng.gen_bool(0.5);
    println!(
        "  Coin flip (50%): {}",
        if coin_flip { "Heads" } else { "Tails" }
    );

    // Generate a biased coin flip (75% probability of true)
    let biased_flip = rng.gen_ratio(3, 4);
    println!(
        "  Biased flip (75%): {}",
        if biased_flip { "Yes" } else { "No" }
    );

    // Simulate multiple flips to show distribution
    let mut heads_count = 0;
    let trials = 1000;
    for _ in 0..trials {
        if rng.gen_bool(0.5) {
            heads_count += 1;
        }
    }
    let heads_percentage = (f64::from(heads_count) / f64::from(trials)) * 100.0;
    println!("  1000 fair flips: {heads_count} heads ({heads_percentage:.1}%)");
}

/// Demonstrates filling buffers with random data
fn buffer_filling() {
    use switchy_random::{GenericRng as _, Rng};

    let rng = Rng::new();

    // Fill a small byte array
    let mut bytes = [0u8; 8];
    rng.fill_bytes(&mut bytes);
    print!("  8 random bytes: ");
    for (i, byte) in bytes.iter().enumerate() {
        print!("{byte:02x}");
        if i < bytes.len() - 1 {
            print!(" ");
        }
    }
    println!();

    // Fill a larger buffer using the Fill trait
    let mut buffer = [0u32; 5];
    rng.fill(&mut buffer[..]);
    print!("  5 random u32s: ");
    for (i, value) in buffer.iter().enumerate() {
        print!("{value}");
        if i < buffer.len() - 1 {
            print!(", ");
        }
    }
    println!();

    // Demonstrate error handling with try_fill
    let mut error_buffer = [0u8; 16];
    match rng.try_fill(&mut error_buffer[..]) {
        Ok(()) => println!("  ✓ Successfully filled 16-byte buffer"),
        Err(e) => println!("  ✗ Error filling buffer: {e}"),
    }
}
