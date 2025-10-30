# Events Example

A comprehensive example demonstrating the event system in `moosicbox_profiles`, showing how to subscribe to and trigger profile update events.

## What This Example Demonstrates

- Registering event listeners for profile changes
- Handling profile addition events
- Handling profile removal events
- Managing multiple concurrent event listeners
- Tracking cumulative statistics across events
- Async event handling with tokio

## Prerequisites

- Rust 1.75 or later
- Understanding of async/await in Rust
- Basic familiarity with tokio runtime

## Running the Example

```bash
cargo run --manifest-path packages/profiles/examples/events/Cargo.toml
```

## Expected Output

```
=== MoosicBox Profiles - Events Example ===

1. Registering first event listener...
   ✓ Listener 1 registered

2. Registering second event listener...
   ✓ Listener 2 registered

3. Registering statistics listener...
   ✓ Statistics listener registered

4. Triggering event: Adding profiles...

[Listener 1 - Event #1]
  Profiles added: ["alice", "bob", "charlie"]

[Listener 2]
  Total changes: 3 added, 0 removed
  ✓ Listener 2 processing complete

[Statistics Listener]
  Cumulative totals:
    Total added: 3
    Total removed: 0
    Net change: 3

5. Triggering event: Removing profile 'bob'...

[Listener 1 - Event #2]
  Profiles removed: ["bob"]

[Listener 2]
  Total changes: 0 added, 1 removed
  ✓ Listener 2 processing complete

[Statistics Listener]
  Cumulative totals:
    Total added: 3
    Total removed: 1
    Net change: 2

6. Triggering event: Mixed changes...

[Listener 1 - Event #3]
  Profiles added: ["david", "eve"]
  Profiles removed: ["charlie"]

[Listener 2]
  Total changes: 2 added, 1 removed
  ✓ Listener 2 processing complete

[Statistics Listener]
  Cumulative totals:
    Total added: 5
    Total removed: 2
    Net change: 3

7. Final profile registry state:
   Total profiles: 4
   - alice
   - david
   - eve

8. Event processing summary:
   Total events triggered: 3
   Total profiles added: 5
   Total profiles removed: 2

=== Example completed successfully ===
```

## Code Walkthrough

### Registering Event Listeners

```rust
use moosicbox_profiles::events::on_profiles_updated_event;

on_profiles_updated_event(|added, removed| {
    let added = added.to_vec();
    let removed = removed.to_vec();
    async move {
        println!("Profiles added: {:?}", added);
        println!("Profiles removed: {:?}", removed);
        Ok(())
    }
}).await;
```

The `on_profiles_updated_event` function registers a callback that receives two parameters:

- `added: &[String]` - Slice of profile names that were added
- `removed: &[String]` - Slice of profile names that were removed

The callback must return a `Future` that resolves to `Result<(), Box<dyn std::error::Error + Send>>`.

### Triggering Events

```rust
use moosicbox_profiles::events::trigger_profiles_updated_event;

trigger_profiles_updated_event(
    vec!["alice".to_string(), "bob".to_string()],  // added
    vec!["charlie".to_string()]                     // removed
).await?;
```

The `trigger_profiles_updated_event` function notifies all registered listeners about profile changes. It takes two vectors:

1. Profiles that were added
2. Profiles that were removed

### Capturing Variables in Listeners

To use external variables in your listener callbacks, clone them before moving into the async block:

```rust
let counter = Arc::new(AtomicUsize::new(0));
let counter_clone = Arc::clone(&counter);

on_profiles_updated_event(move |added, removed| {
    let count = counter_clone.fetch_add(1, Ordering::SeqCst);
    let added = added.to_vec();
    let removed = removed.to_vec();
    async move {
        println!("Event #{}", count);
        Ok(())
    }
}).await;
```

Note: We clone the slices (`added.to_vec()`, `removed.to_vec()`) to move owned data into the async block.

## Key Concepts

### Event-Driven Architecture

The events system allows you to decouple profile management from business logic. Instead of directly calling functions when profiles change, you can:

1. Trigger events when profiles are updated
2. Multiple systems can independently listen for these events
3. Each listener processes events according to its own needs

This is useful for:

- Logging profile changes
- Syncing profiles to external systems
- Invalidating caches when profiles change
- Triggering workflows based on profile updates

### Async Event Processing

All event listeners are async, allowing them to:

- Make network requests
- Query databases
- Perform I/O operations
- Execute other async operations

Listeners run concurrently, but the `trigger_profiles_updated_event` function waits for all listeners to complete before returning.

### Error Handling

If any listener returns an error, `trigger_profiles_updated_event` collects all errors and returns them as `Vec<Box<dyn std::error::Error + Send>>`. This allows you to:

- See which listeners failed
- Continue processing even if some listeners fail
- Handle errors at the event trigger site

### Thread Safety

The event system is thread-safe:

- Listeners are stored in `Arc<RwLock<Vec<...>>>`
- Multiple threads can trigger events concurrently
- Listeners are protected from concurrent modification during event dispatch

## Testing the Example

The example is self-contained and demonstrates:

1. **Multiple listeners** - Shows how multiple systems can independently react to the same events
2. **Stateful listeners** - Demonstrates using `Arc` and atomic types to maintain state across events
3. **Async operations** - Shows how listeners can perform async work (simulated with `sleep`)
4. **Mixed changes** - Demonstrates events with both additions and removals

### Modifying the Example

Try these modifications:

1. **Add error handling**:

    ```rust
    on_profiles_updated_event(|added, removed| async move {
        if added.len() > 5 {
            return Err("Too many profiles added at once".into());
        }
        Ok(())
    }).await;
    ```

2. **Add a logger listener**:

    ```rust
    on_profiles_updated_event(|added, removed| async move {
        let timestamp = std::time::SystemTime::now();
        println!("[{:?}] Profile change event", timestamp);
        Ok(())
    }).await;
    ```

3. **Simulate database sync**:
    ```rust
    on_profiles_updated_event(|added, removed| async move {
        // Simulate database write
        tokio::time::sleep(Duration::from_millis(500)).await;
        println!("Synced {} profiles to database", added.len());
        Ok(())
    }).await;
    ```

## Troubleshooting

### Events not firing

Ensure you:

1. Call `trigger_profiles_updated_event` after registering listeners
2. Use `.await` on both registration and triggering
3. Run your code in a tokio runtime (`#[tokio::main]`)

### Listener panics

If a listener panics, it will poison the event system. Ensure your listeners:

- Use proper error handling (`Result` types)
- Don't panic on normal error conditions
- Catch and log unexpected errors

### Listeners not seeing captured variables

Remember to:

- Clone `Arc` references before moving into the async block
- Convert slices to owned `Vec` with `.to_vec()` if needed beyond the callback scope
- Use `move` keyword when capturing variables

## Related Examples

- `basic_usage` - Demonstrates core profile registry operations
- `api_integration` - Shows actix-web integration with profiles

## Real-World Use Cases

This event system is useful for:

1. **Cache invalidation**: Clear caches when profiles are removed
2. **Audit logging**: Record all profile changes with timestamps
3. **External sync**: Update external databases or APIs when profiles change
4. **Metrics**: Track profile creation/deletion rates
5. **Notifications**: Alert administrators when profiles are created/removed
