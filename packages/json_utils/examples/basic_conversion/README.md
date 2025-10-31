# Basic JSON Value Conversion Example

A comprehensive example demonstrating JSON value conversion and type-safe extraction using the `moosicbox_json_utils` crate.

## Summary

This example showcases the core functionality of `moosicbox_json_utils` for converting JSON values to Rust types, extracting values from JSON objects, navigating nested structures, and handling optional values with proper error handling.

## What This Example Demonstrates

- **Basic type conversions** - Converting JSON primitives (numbers, strings, booleans) to Rust types using `ToValueType`
- **Object value extraction** - Extracting typed values from JSON objects by key using `ToValue`
- **Nested structure navigation** - Accessing deeply nested JSON values with paths using `ToNestedValue`
- **Array handling** - Working with JSON arrays and converting them to Rust vectors
- **Optional value handling** - Properly handling null values and missing fields with `Option<T>`
- **Error handling** - Comprehensive error handling patterns for type mismatches and missing values

## Prerequisites

- Basic understanding of Rust and JSON
- Familiarity with error handling using `Result` and the `?` operator
- Understanding of trait-based APIs

## Running the Example

Run the example from the repository root:

```bash
cargo run --manifest-path packages/json_utils/examples/basic_conversion/Cargo.toml
```

Or from the example directory:

```bash
cd packages/json_utils/examples/basic_conversion
cargo run
```

## Expected Output

```
=== MoosicBox JSON Utils: Basic Conversion Example ===

1. Basic Type Conversions:
  JSON number to i32: 42 -> 42
  JSON boolean: true -> true
  JSON string: "Hello, MoosicBox!" -> 'Hello, MoosicBox!'
  JSON float: 3.14159 -> 3.14159

2. Extracting Values from Objects:
  Track: 'Bohemian Rhapsody' by Queen
  Duration: 355s, Year: 1975, Favorite: true

3. Navigating Nested Structures:
  Album: 'The Dark Side of the Moon'
  Artist: Pink Floyd
  Released: 1973 on Harvest Records
  10 tracks, 2532 seconds total

4. Working with Arrays:
  Array of numbers: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
  Sum: 55
  Genres: rock, jazz, classical, electronic
  Playlist with 3 tracks:
    - 'Song A' (180s)
    - 'Song B' (210s)
    - 'Song C' (195s)
  Total duration: 585s

5. Handling Optional Values:
  Name: John Doe
  Email: john@example.com
  Phone: None
  Address: None
  Age: Some(30)
  No phone number on file

6. Error Handling:
  Attempting invalid conversion (string as number):
    ConvertType error (expected): Path 'text' failed to convert value to type: 'ConvertType("u32")'
  Attempting to access non-existent field:
    Parse error (expected): Missing value: 'missing' ({"count":42,"text":"hello"})
  Valid conversion:
    Successfully extracted count: 42

=== All examples completed successfully! ===
```

## Code Walkthrough

### 1. Basic Type Conversions

The example starts by demonstrating simple conversions using the `ToValueType` trait:

```rust
use moosicbox_json_utils::serde_json::ToValueType;

let json_number = json!(42);
let number: i32 = (&json_number).to_value_type()?;
```

The `ToValueType` trait provides a consistent interface for converting JSON values to any supported Rust type. It works with primitives (integers, floats, booleans, strings) and compound types (vectors, options).

### 2. Extracting Values from JSON Objects

When working with JSON objects, the `ToValue` trait provides convenient access by key:

```rust
use moosicbox_json_utils::serde_json::ToValue;

let track = json!({
    "title": "Bohemian Rhapsody",
    "duration": 355
});

let title: String = track.to_value("title")?;
let duration: u32 = track.to_value("duration")?;
```

This pattern is concise and type-safe, with automatic error handling through the `?` operator.

### 3. Navigating Nested Structures

For deeply nested JSON, the `ToNestedValue` trait accepts a path as an array of keys:

```rust
use moosicbox_json_utils::serde_json::ToNestedValue;

let album = json!({
    "metadata": {
        "release": {
            "year": 1973
        }
    }
});

let year: u16 = album.to_nested_value(&["metadata", "release", "year"])?;
```

This eliminates the need for manual null-checking and intermediate unwrapping.

### 4. Working with Arrays

JSON arrays can be converted to Rust vectors with full type inference:

```rust
let json_array = json!([1, 2, 3, 4, 5]);
let numbers: Vec<i32> = (&json_array).to_value_type()?;
```

The example also demonstrates extracting an array of objects and processing each element:

```rust
let tracks: Vec<&Value> = playlist.to_value("tracks")?;
for track in &tracks {
    let title: String = track.to_value("title")?;
    let duration: u32 = track.to_value("duration")?;
    // Process track...
}
```

### 5. Handling Optional Values

The library provides seamless handling of optional values using `Option<T>`:

```rust
let data = json!({
    "name": "John",
    "phone": null
});

// Required value
let name: String = data.to_value("name")?; // Ok("John")

// Optional value (null)
let phone: Option<String> = data.to_value("phone")?; // Ok(None)

// Optional value (missing field)
let address: Option<String> = data.to_value("address")?; // Ok(None)
```

When using `Option<T>`, both null values and missing fields return `Ok(None)` rather than an error, making optional field handling ergonomic.

### 6. Error Handling

The library provides specific error types through the `ParseError` enum:

```rust
use moosicbox_json_utils::ParseError;

match data.to_value::<u32>("text") {
    Ok(val) => println!("Success: {}", val),
    Err(ParseError::ConvertType(msg)) => {
        // Type mismatch (e.g., trying to convert string to number)
        eprintln!("Conversion failed: {}", msg);
    }
    Err(ParseError::Parse(msg)) => {
        // Missing value or parse failure
        eprintln!("Parse failed: {}", msg);
    }
    Err(ParseError::MissingValue(field)) => {
        // Required field is missing
        eprintln!("Missing field: {}", field);
    }
}
```

## Key Concepts

### Type-Safe Conversions

All conversions are type-safe and checked at compile time. The target type is inferred from context or explicitly specified using turbofish syntax (`::<T>`).

### Trait-Based API

The library uses traits (`ToValueType`, `ToValue`, `ToNestedValue`) to provide a consistent interface across different JSON value types. This makes the API predictable and composable.

### Error Propagation

All conversion methods return `Result<T, ParseError>`, allowing seamless error propagation using the `?` operator. This integrates naturally with Rust's error handling patterns.

### Zero-Copy Where Possible

Many conversions, especially for strings and object references, avoid unnecessary copying by returning references where appropriate.

### Optional Value Semantics

The `Option<T>` handling follows Rust conventions: `None` for both null JSON values and missing fields, making optional field handling consistent and ergonomic.

## Testing the Example

You can modify the example code to test different scenarios:

1. **Try different types**: Change the target types in conversions to see type safety in action
2. **Add more nesting**: Extend the nested JSON examples with deeper structures
3. **Test error cases**: Intentionally create type mismatches to see error messages
4. **Work with real data**: Replace example JSON with actual data from your application

## Troubleshooting

### Type Conversion Errors

If you see `ConvertType` errors, ensure:

- The JSON value type matches the Rust type (e.g., JSON number for numeric types)
- You're using the correct numeric type (signed vs unsigned, size)
- String values aren't being converted to non-string types without parsing

### Missing Value Errors

If you see `Parse` or `MissingValue` errors:

- Check that field names match exactly (case-sensitive)
- Verify the JSON structure matches your path
- Consider using `Option<T>` for fields that may be absent

### Compilation Errors

If the example doesn't compile:

- Ensure you're using a recent Rust version (1.70+)
- Check that all dependencies are properly specified in `Cargo.toml`
- Verify you're running from the correct directory

## Related Examples

- Database value conversion examples (coming soon)
- Tantivy search integration examples (coming soon)
- SQLite value conversion examples (coming soon)

## Further Reading

- [moosicbox_json_utils README](../../README.md) - Full package documentation
- [serde_json documentation](https://docs.rs/serde_json) - JSON serialization/deserialization
- [Rust error handling guide](https://doc.rust-lang.org/book/ch09-00-error-handling.html)
