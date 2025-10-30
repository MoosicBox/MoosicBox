# Basic Usage Example

## Summary

This example demonstrates the core features of the `switchy_random` crate, including basic random number generation, seeded RNGs for reproducibility, range-based generation, distribution sampling, and non-uniform distributions.

## What This Example Demonstrates

- Creating random number generators with `Rng::new()`
- Generating different types of random values (u32, i32, u64, bytes)
- Using seeded RNGs with `Rng::from_seed()` for reproducible random sequences
- Generating random values in specific ranges with `gen_range()`
- Sampling from standard distributions with `random()`
- Boolean generation with probabilities using `gen_bool()` and `gen_ratio()`
- Non-uniform distribution sampling with `gen_range_dist()`
- Thread-safe RNG usage with cloning and shared state

## Prerequisites

- Basic understanding of random number generation concepts
- Familiarity with Rust's range syntax (e.g., `1..=6`, `0.0..100.0`)
- Understanding of probability and distributions (helpful but not required)

## Running the Example

```bash
cargo run --manifest-path packages/random/examples/basic_usage/Cargo.toml
```

## Expected Output

You should see output similar to:

```
=== Switchy Random - Basic Usage Example ===

1. Basic Random Number Generation
----------------------------------
Random u32: 3842819289
Random i32: -1092847563
Random u64: 12834719283476
Random bytes: [23, 147, 89, 201, ...]

2. Seeded Random Generation (Reproducible)
-------------------------------------------
Seeded RNG (seed=12345) - First value:  3676720990
Seeded RNG (seed=12345) - Second value: 2807688456
New RNG (seed=12345)    - First value:  3676720990
New RNG (seed=12345)    - Second value: 2807688456
Values match: 3676720990 == 3676720990: true

3. Range-Based Random Generation
----------------------------------
Dice roll (1-6): 4
Percentage (0-100): 67.45
Random year (2000-2025): 2018

...
```

Note: The exact values will vary for unseeded RNGs but will be consistent for seeded RNGs with the same seed.

## Code Walkthrough

### 1. Creating a Random Number Generator

```rust
let rng = Rng::new();
```

Creates a new RNG initialized with entropy from the system. This is the simplest way to get started with random number generation.

### 2. Basic Random Values

```rust
let random_u32 = rng.next_u32();
let random_i32 = rng.next_i32();
let random_u64 = rng.next_u64();
```

The `next_*` methods generate random values of specific integer types across their full range.

### 3. Filling Byte Arrays

```rust
let mut bytes = [0_u8; 16];
rng.fill(&mut bytes);
```

The `fill()` method is useful for generating random binary data, such as for cryptographic keys, UUIDs, or initialization vectors.

### 4. Seeded RNGs for Reproducibility

```rust
let seeded_rng = Rng::from_seed(12345_u64);
```

Using a seed ensures that the random sequence is reproducible. This is crucial for:

- Testing scenarios where you need consistent behavior
- Simulations that need to be repeatable
- Debugging random-related issues

Two RNGs created with the same seed will produce identical sequences.

### 5. Range-Based Generation

```rust
let dice_roll = rng.gen_range(1..=6);
let percentage = rng.gen_range(0.0..100.0);
```

The `gen_range()` method generates random values within a specified range. It works with both inclusive (`..=`) and exclusive (`..`) ranges and supports both integer and floating-point types.

### 6. Boolean Generation with Probabilities

```rust
let coin_flip = rng.gen_bool(0.5);      // 50% chance of true
let biased = rng.gen_ratio(3, 4);       // 75% chance (3/4)
```

These methods are useful for implementing probabilistic behavior in games, simulations, or decision-making systems.

### 7. Non-Uniform Distributions

```rust
let value: f64 = rng.gen_range_dist(0.0..1.0, 2.0);
```

Non-uniform distributions allow you to bias random values toward lower or higher ends of a range:

- Power > 1.0: Favors lower values (e.g., `2.0` creates a quadratic distribution)
- Power < 1.0: Favors higher values (e.g., `0.5` creates a square root distribution)
- Power = 1.0: Uniform distribution (no bias)

This is useful for game mechanics (e.g., loot rarity), natural phenomena simulation, and weighted random selection.

### 8. Thread-Safe Usage

```rust
let shared_rng = Rng::new();
let rng_clone = shared_rng.clone();
```

The `Rng` type is thread-safe and can be cloned. Clones share the same underlying generator state, so they affect each other's sequences. This allows safe concurrent access from multiple threads.

## Key Concepts

### Thread Safety

The `Rng` type uses `Arc<Mutex<_>>` internally, making it safe to share across threads. When you clone an `Rng`, you're creating a new handle to the same underlying generator, not a new independent generator.

### Reproducibility vs. Entropy

- `Rng::new()` - Uses system entropy for unpredictable sequences
- `Rng::from_seed(seed)` - Uses a fixed seed for reproducible sequences

For production code needing security, always use `Rng::new()`. For testing and simulations, use `Rng::from_seed()`.

### Distribution Types

The crate supports multiple ways to generate random values:

1. **Raw values**: `next_u32()`, `next_u64()` - Full range of the type
2. **Standard distribution**: `random()` - Uniform distribution over the type's range
3. **Range-based**: `gen_range()` - Uniform distribution over a specific range
4. **Non-uniform**: `gen_range_dist()` - Biased distribution over a range
5. **Boolean**: `gen_bool()`, `gen_ratio()` - Probability-based true/false

## Testing the Example

Run the example multiple times to observe different random values for unseeded generators:

```bash
# Run multiple times to see different outputs
cargo run --manifest-path packages/random/examples/basic_usage/Cargo.toml
cargo run --manifest-path packages/random/examples/basic_usage/Cargo.toml
```

Notice that:

- Unseeded RNG values change between runs
- Seeded RNG values (with seed 12345) remain consistent across runs
- Non-uniform distributions show visible bias toward lower or higher values

## Troubleshooting

### "cannot sample empty range" panic

This occurs when you pass an empty range to `gen_range()`:

```rust
// WRONG - empty range
let value = rng.gen_range(10..10);

// CORRECT - non-empty range
let value = rng.gen_range(1..=10);
```

### Unexpected distribution behavior

If `gen_range_dist()` seems to behave like uniform distribution:

- Check that your power value is significantly different from 1.0
- Try more extreme powers (e.g., 3.0 or 0.3) to see clearer bias
- Generate more samples to observe statistical patterns

### Compilation errors with features

This example requires the default features of `switchy_random`. If you've disabled default features in your workspace, ensure you enable either `rand` or `simulator`:

```toml
switchy_random = { workspace = true, default-features = true }
```

## Related Examples

This is currently the only example for `switchy_random`. For more detailed API documentation, see:

- Package documentation: `packages/random/README.md`
- API docs: Run `cargo doc --package switchy_random --open`
