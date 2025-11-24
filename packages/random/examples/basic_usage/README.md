# Basic Usage Example

A comprehensive example demonstrating the fundamental features of the switchy_random library.

## Summary

This example demonstrates how to use switchy_random for generating random numbers, working with seeds for reproducibility, generating values in specific ranges, and using probability-based generation.

## What This Example Demonstrates

- Creating random number generators with `Rng::new()`
- Generating random values of different types (u32, i32, u64, f32, f64)
- Using seeded RNGs for reproducible random sequences
- Generating random values within specific ranges using `gen_range()`
- Probability-based random generation with `gen_bool()` and `gen_ratio()`
- Filling buffers with random data using `fill_bytes()` and `fill()`
- Error handling with `try_fill()` for graceful failures

## Prerequisites

- Basic understanding of Rust syntax
- Familiarity with random number generation concepts
- Understanding of probability basics (optional, for probability section)

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/random/examples/basic_usage/Cargo.toml
```

Or from the example directory:

```bash
cd packages/random/examples/basic_usage
cargo run
```

## Expected Output

The example will produce output showing:

```
=== Switchy Random - Basic Usage Example ===

1. Basic Random Number Generation:
  Random u32: 3842114785
  Random i32: -1247382901
  Random u64: 12847362918374629183
  Random f32: 0.43829172
  Random f64: 0.7391829401829374

2. Seeded Random Generation (Reproducible):
  Generator 1 (seed=42): 2558267809, 2168738069, 3820341723
  Generator 2 (seed=42): 2558267809, 2168738069, 3820341723
  ✓ All values match - reproducible!

3. Range-Based Random Generation:
  Dice roll (1-6): 4
  Percentage (0-100): 67.42%
  Array index (0-9): 7
  10 dice rolls: 3, 6, 1, 4, 2, 5, 6, 3, 1, 4

4. Probability-Based Generation:
  Coin flip (50%): Heads
  Biased flip (75%): Yes
  1000 fair flips: 502 heads (50.2%)

5. Filling Buffers with Random Data:
  8 random bytes: 3f a2 e1 9c 4b d7 23 88
  5 random u32s: 2847291038, 1928374655, 3847291927, 1029384756, 2918374629
  ✓ Successfully filled 16-byte buffer

=== Example Complete ===
```

Note: The actual random values will differ on each run unless using a fixed seed.

## Code Walkthrough

### 1. Basic Random Number Generation

The example starts by creating a new RNG and generating various types of random values:

```rust
use switchy_random::Rng;

let rng = Rng::new();

// Generate random integers
let random_u32 = rng.next_u32();
let random_i32 = rng.next_i32();
let random_u64 = rng.next_u64();

// Generate random floating-point values
let random_f32: f32 = rng.random();
let random_f64: f64 = rng.random();
```

The `Rng::new()` creates a new generator with a random seed. The various `next_*()` methods generate specific types, while `random()` can generate any type that implements the `Standard` distribution.

### 2. Seeded Random Generation

For reproducible results (important for testing, simulations, or debugging), use a fixed seed:

```rust
let seed = 42_u64;
let rng1 = Rng::from_seed(seed);
let rng2 = Rng::from_seed(seed);

// Both generators will produce identical sequences
let value1 = rng1.next_u32();
let value2 = rng2.next_u32();
assert_eq!(value1, value2);
```

This is crucial for:

- **Testing**: Ensures test randomness is consistent across runs
- **Debugging**: Allows reproduction of specific random scenarios
- **Simulations**: Enables repeatable simulation results

### 3. Range-Based Generation

Generate random values within specific bounds using `gen_range()`:

```rust
// Inclusive range (1 to 6, like a dice)
let dice_roll = rng.gen_range(1..=6);

// Exclusive upper bound
let index = rng.gen_range(0..10); // 0 to 9

// Works with floating-point too
let percentage = rng.gen_range(0.0..100.0);
```

The range syntax follows Rust's standard range notation (`..` for exclusive, `..=` for inclusive).

### 4. Probability-Based Generation

Generate boolean values with specified probabilities:

```rust
// 50% chance of true
let coin_flip = rng.gen_bool(0.5);

// 75% chance of true (3/4 ratio)
let biased = rng.gen_ratio(3, 4);
```

- `gen_bool(p)`: Takes a probability as f64 (0.0 to 1.0)
- `gen_ratio(n, d)`: Takes numerator and denominator for exact ratios

### 5. Filling Buffers

Efficiently fill arrays or buffers with random data:

```rust
// Fill a byte array
let mut bytes = [0u8; 8];
rng.fill_bytes(&mut bytes);

// Fill a slice of any Fill type
let mut buffer = vec![0u32; 5];
rng.fill(&mut buffer[..]);

// Error handling version
match rng.try_fill(&mut buffer[..]) {
    Ok(()) => println!("Success!"),
    Err(e) => println!("Error: {e}"),
}
```

## Key Concepts

### Thread Safety

The `Rng` type is thread-safe and can be cloned to share across threads:

```rust
let rng = Rng::new();
let rng_clone = rng.clone(); // Both share the same underlying RNG
```

The internal state is protected by a mutex, ensuring safe concurrent access.

### Feature Backends

switchy_random supports different backends via features:

- **`rand`** (default): Standard random generation using `rand::rngs::SmallRng`
- **`simulator`** (default): Deterministic generation for simulations

The API remains identical regardless of which backend is active.

### Reproducibility

Seeded RNGs are deterministic - the same seed will always produce the same sequence of values. This is valuable for:

- Unit tests that need consistent behavior
- Debugging issues that involve randomness
- Simulations that must be reproducible
- Procedural generation with consistent results

## Testing the Example

Try modifying the example to explore different scenarios:

1. **Change the seed value** in `seeded_random_generation()` and observe how the sequence changes
2. **Adjust probability values** in `probability_generation()` to see different distributions
3. **Run the example multiple times** and note that unseeded values differ, but seeded values remain constant
4. **Modify range bounds** in `range_based_generation()` to generate different value ranges
5. **Increase the trial count** in the coin flip simulation to see the percentage converge to 50%

## Troubleshooting

### Issue: Values are not random enough

**Solution**: The default SmallRng is designed for speed, not cryptographic security. For security-sensitive applications, use a cryptographically secure RNG from the `rand` crate directly.

### Issue: Same values on every run

**Solution**: If you're seeing identical values across program runs, check if you've accidentally used a fixed seed. Use `Rng::new()` for entropy-based initialization.

### Issue: Range generation panics

**Solution**: Ensure your range is not empty. For example, `gen_range(5..5)` will panic. Use `gen_range(5..6)` or `gen_range(5..=5)` instead.

## Related Examples

- See the main README.md for additional usage patterns
- Check the library documentation for advanced features like non-uniform distributions
