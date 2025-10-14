# Switchy Random

A basic random number generation library providing a unified interface for random number generation with optional seeding and distribution utilities.

## Features

- **Generic RNG Interface**: Unified trait for different random number generators
- **Thread-Safe Wrapper**: Safe concurrent access to random number generators
- **Basic Random Generation**: Generate u32, i32, u64 values and fill byte arrays
- **Distribution Support**: Sample from various probability distributions using rand crate
- **Optional Features**: Conditional compilation for rand and simulator modules
- **Custom Distributions**: Non-uniform distribution utilities

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
switchy_random = "0.1.4"
```

## Usage

### Basic Random Generation

```rust
use switchy_random::{Rng, GenericRng};

fn main() {
    // Create a new random number generator
    let rng = Rng::new();

    // Generate basic random numbers
    let random_u32 = rng.next_u32();
    let random_i32 = rng.next_i32();
    let random_u64 = rng.next_u64();

    println!("Random u32: {}", random_u32);
    println!("Random i32: {}", random_i32);
    println!("Random u64: {}", random_u64);

    // Fill a byte array with random data
    let mut bytes = [0u8; 16];
    rng.fill_bytes(&mut bytes);
    println!("Random bytes: {:?}", bytes);
}
```

### Seeded Random Generation

```rust
use switchy_random::Rng;

fn main() {
    // Create generator with specific seed for reproducible results
    let rng = Rng::from_seed(Some(12345));

    let value1 = rng.next_u32();
    let value2 = rng.next_u32();

    // Create another generator with same seed
    let rng2 = Rng::from_seed(Some(12345));

    // Should produce same sequence
    assert_eq!(value1, rng2.next_u32());
    assert_eq!(value2, rng2.next_u32());
}
```

### Distribution Sampling

```rust
use switchy_random::Rng;

fn main() {
    let rng = Rng::new();

    // Generate random values from different distributions
    let uniform_float: f64 = rng.random();
    let uniform_int: i32 = rng.random();

    // Generate values in specific ranges
    let dice_roll = rng.gen_range(1..=6);
    let percentage = rng.gen_range(0.0..100.0);

    println!("Uniform float: {}", uniform_float);
    println!("Uniform int: {}", uniform_int);
    println!("Dice roll: {}", dice_roll);
    println!("Percentage: {}", percentage);

    // Boolean generation
    let coin_flip = rng.gen_bool(0.5); // 50% chance
    let biased = rng.gen_ratio(3, 4);  // 75% chance

    println!("Coin flip: {}", coin_flip);
    println!("Biased (75%): {}", biased);
}
```

### Non-Uniform Distributions

```rust
use switchy_random::{Rng, non_uniform_distribute_f64, non_uniform_distribute_i32};

fn main() {
    let rng = Rng::new();

    // Apply non-uniform distribution to a value
    let base_value = 0.5;
    let power = 2.0;
    let distributed = non_uniform_distribute_f64(base_value, power, &rng);

    println!("Base value: {}", base_value);
    println!("Distributed value: {}", distributed);

    // Integer power distribution
    let int_distributed = non_uniform_distribute_i32(base_value, 3, &rng);
    println!("Integer distributed: {}", int_distributed);
}
```

### Custom Range Generation with Distribution

```rust
use switchy_random::Rng;

fn main() {
    let rng = Rng::new();

    // Generate with custom distribution applied
    // F64Convertible is already implemented for f32, f64, and integer types
    let value: f32 = rng.gen_range_dist(0.0..1.0, 2.0);
    let int_value: i32 = rng.gen_range_disti(1..100, 2);

    println!("Distributed float: {}", value);
    println!("Distributed int: {}", int_value);
}
```

## Architecture

### Core Traits

- `GenericRng`: Main trait defining random number generation interface
- `F64Convertible`: Trait for types that can convert to/from f64 for distributions

### Thread Safety

The `RngWrapper` provides thread-safe access to random number generators using `Arc<Mutex<R>>`, allowing safe concurrent usage across multiple threads.

### Optional Features

Both features are enabled by default:

- `rand`: Provides the standard `RandRng` implementation using `rand::rngs::SmallRng`
- `simulator`: Provides `SimulatorRng` with deterministic seeding via `SIMULATOR_SEED` environment variable

## Error Handling

The library provides basic error handling for random number generation failures:

- `fill`: Will panic if the underlying RNG fails to fill
- `try_fill`: Returns `Result<(), rand::Error>` for graceful error handling
- `fill_bytes`: Part of the `GenericRng` trait, delegates to underlying RNG
- `try_fill_bytes`: Returns `Result<(), rand::Error>` from the underlying RNG

## Performance

The wrapper adds minimal overhead while providing thread safety. For high-performance scenarios where thread safety isn't required, consider using the underlying RNG directly.
