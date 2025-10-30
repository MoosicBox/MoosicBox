#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions, clippy::similar_names)]

//! Basic usage example demonstrating core features of the `switchy_random` crate.
//!
//! This example shows:
//! - Creating random number generators
//! - Generating different types of random values
//! - Using seeded RNGs for reproducibility
//! - Working with ranges and distributions
//! - Boolean generation with probabilities
//! - Non-uniform distributions

use switchy_random::Rng;

fn main() {
    println!("=== Switchy Random - Basic Usage Example ===\n");

    // 1. Basic random number generation
    println!("1. Basic Random Number Generation");
    println!("----------------------------------");
    let rng = Rng::new();

    let random_u32 = rng.next_u32();
    let random_i32 = rng.next_i32();
    let random_u64 = rng.next_u64();

    println!("Random u32: {random_u32}");
    println!("Random i32: {random_i32}");
    println!("Random u64: {random_u64}");

    // Fill a byte array with random data
    let mut bytes = [0_u8; 16];
    rng.fill(&mut bytes);
    println!("Random bytes: {bytes:?}");
    println!();

    // 2. Seeded random generation for reproducibility
    println!("2. Seeded Random Generation (Reproducible)");
    println!("-------------------------------------------");
    let seeded_rng1 = Rng::from_seed(12345_u64);
    let value1 = seeded_rng1.next_u32();
    let value2 = seeded_rng1.next_u32();

    println!("Seeded RNG (seed=12345) - First value:  {value1}");
    println!("Seeded RNG (seed=12345) - Second value: {value2}");

    // Create another RNG with the same seed - should produce same sequence
    let seeded_rng2 = Rng::from_seed(12345_u64);
    let value1_repeat = seeded_rng2.next_u32();
    let value2_repeat = seeded_rng2.next_u32();

    println!("New RNG (seed=12345)    - First value:  {value1_repeat}");
    println!("New RNG (seed=12345)    - Second value: {value2_repeat}");
    println!(
        "Values match: {} == {}: {}",
        value1,
        value1_repeat,
        value1 == value1_repeat
    );
    println!();

    // 3. Range-based random generation
    println!("3. Range-Based Random Generation");
    println!("----------------------------------");
    let dice_roll = rng.gen_range(1..=6);
    let percentage = rng.gen_range(0.0..100.0);
    let random_year = rng.gen_range(2000..=2025);

    println!("Dice roll (1-6): {dice_roll}");
    println!("Percentage (0-100): {percentage:.2}");
    println!("Random year (2000-2025): {random_year}");
    println!();

    // 4. Random values with standard distribution
    println!("4. Standard Distribution Sampling");
    println!("----------------------------------");
    let uniform_float: f64 = rng.random();
    let uniform_int: i32 = rng.random();

    println!("Uniform float: {uniform_float}");
    println!("Uniform int: {uniform_int}");
    println!();

    // 5. Boolean generation with probabilities
    println!("5. Boolean Generation with Probabilities");
    println!("------------------------------------------");
    let coin_flip = rng.gen_bool(0.5); // 50% chance
    let likely = rng.gen_bool(0.8); // 80% chance
    let unlikely = rng.gen_bool(0.2); // 20% chance
    let almost_certain = rng.gen_ratio(9, 10); // 90% chance (9/10)

    println!("Coin flip (50%): {coin_flip}");
    println!("Likely event (80%): {likely}");
    println!("Unlikely event (20%): {unlikely}");
    println!("Almost certain (90%): {almost_certain}");
    println!();

    // 6. Non-uniform distributions
    println!("6. Non-Uniform Distributions");
    println!("-----------------------------");
    // Generate multiple values to show distribution
    println!("Standard uniform distribution (10 samples):");
    for _ in 0..10 {
        let value = rng.gen_range(0.0..1.0);
        print!("{value:.3} ");
    }
    println!();

    println!("\nNon-uniform distribution with power=2.0 (favors lower values):");
    for _ in 0..10 {
        let value: f64 = rng.gen_range_dist(0.0..1.0, 2.0);
        print!("{value:.3} ");
    }
    println!();

    println!("\nNon-uniform distribution with power=0.5 (favors higher values):");
    for _ in 0..10 {
        let value: f64 = rng.gen_range_dist(0.0..1.0, 0.5);
        print!("{value:.3} ");
    }
    println!();

    // 7. Thread-safe usage demonstration
    println!("\n7. Thread-Safe Usage");
    println!("---------------------");
    let shared_rng = Rng::from_seed(54321_u64);

    // Clone the RNG - this shares the underlying generator
    let rng_clone = shared_rng.clone();

    println!("Original RNG: {}", shared_rng.next_u32());
    println!("Cloned RNG:   {}", rng_clone.next_u32());
    println!("Original RNG: {}", shared_rng.next_u32());
    println!(
        "\nNote: Both RNGs share the same underlying generator, so they affect each other's state."
    );

    println!("\n=== Example Complete ===");
}
