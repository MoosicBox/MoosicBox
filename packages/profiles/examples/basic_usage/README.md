# Basic Usage Example

A simple example demonstrating core profile management functionality in `moosicbox_profiles`.

## What This Example Demonstrates

- Adding profiles to the global registry
- Retrieving specific profiles by name
- Listing all registered profiles
- Using `add_fetch()` to add and retrieve in one operation
- Removing profiles from the registry
- Verifying profile existence

## Prerequisites

- Rust 1.75 or later
- Basic understanding of Rust ownership and borrowing

## Running the Example

```bash
cargo run --manifest-path packages/profiles/examples/basic_usage/Cargo.toml
```

## Expected Output

```
=== MoosicBox Profiles - Basic Usage Example ===

1. Adding profiles to the registry...
   Added: user1, user2, admin

2. Retrieving a specific profile...
   Found profile: user1

3. Attempting to retrieve non-existent profile...
   Profile 'nonexistent' not found (as expected)

4. Listing all registered profiles...
   Total profiles: 3
   - admin
   - user1
   - user2

5. Adding and fetching a profile in one operation...
   Added and retrieved: guest

6. Verifying all profiles after addition...
   Total profiles: 4
   - admin
   - guest
   - user1
   - user2

7. Removing profile 'user2'...
   Removed: user2

8. Verifying profiles after removal...
   Total profiles: 3
   - admin
   - guest
   - user1

9. Confirming removed profile is not retrievable...
   Confirmed: 'user2' is no longer in the registry

=== Example completed successfully ===
```

## Code Walkthrough

### Accessing the Global Registry

```rust
use moosicbox_profiles::PROFILES;
```

The `PROFILES` static provides access to the global profile registry, which is thread-safe and can be used from anywhere in your application.

### Adding Profiles

```rust
PROFILES.add("user1".to_string());
```

Profiles are added to the registry using the `add()` method. Profile names are stored as strings and must be owned.

### Retrieving Profiles

```rust
match PROFILES.get("user1") {
    Some(profile) => println!("Found: {}", profile),
    None => println!("Not found"),
}
```

The `get()` method returns `Option<String>`, allowing you to check if a profile exists before using it.

### Listing All Profiles

```rust
let all_profiles = PROFILES.names();
```

The `names()` method returns a `Vec<String>` containing all registered profile names in sorted order (stored in a `BTreeSet` internally).

### Add and Fetch Pattern

```rust
let profile = PROFILES.add_fetch("guest");
```

The `add_fetch()` method combines adding a profile and retrieving it in one operation, which is useful when you need the profile value immediately after registration.

### Removing Profiles

```rust
PROFILES.remove("user2");
```

Profiles can be removed from the registry using the `remove()` method. Subsequent `get()` calls will return `None` for removed profiles.

## Key Concepts

### Global Registry Pattern

The `PROFILES` static uses `LazyLock` to provide a globally accessible, lazily initialized registry. This pattern is ideal for application-wide state that needs to be accessed from multiple modules without passing references around.

### Thread Safety

All operations on the profile registry are thread-safe. The underlying `RwLock` allows multiple concurrent readers or a single writer, making it efficient for read-heavy workloads typical of profile lookups.

### BTreeSet Storage

Profiles are stored in a `BTreeSet<String>`, which provides:

- Automatic sorting of profile names
- Efficient lookups
- Guaranteed uniqueness (duplicate additions are ignored)

## Testing the Example

The example is self-contained and demonstrates all major operations. You can modify it to:

1. **Test with your own profile names**: Change the profile names in `add()` calls
2. **Simulate concurrent access**: Wrap operations in threads to test thread safety
3. **Measure performance**: Add timing code to measure registry operations

## Troubleshooting

### Panics on RwLock poisoning

If you see a panic about "RwLock poisoned", this indicates that a thread panicked while holding the lock. This should not occur in normal usage, but if you're testing error conditions or using the registry from multiple threads with panicking code, ensure proper error handling.

## Related Examples

- `api_integration` - Demonstrates using profiles with actix-web HTTP extractors
- `events` - Shows how to subscribe to profile update events
