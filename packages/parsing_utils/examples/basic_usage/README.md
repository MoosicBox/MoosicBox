# Basic Usage Example

This example demonstrates the core functionality of the `moosicbox_parsing_utils` package, including parsing comma-separated integer sequences and hyphen-separated ranges.

## Summary

A comprehensive example showing how to parse integer sequences, handle ranges with commas, and properly handle various error conditions that can occur during parsing.

## What This Example Demonstrates

- Parsing comma-separated integer sequences
- Parsing simple hyphen-separated ranges
- Parsing ranges with comma-separated start and end values
- Handling parse errors (invalid integers)
- Handling unmatched range errors (malformed syntax)
- Handling range too large errors (exceeding 100,000 item limit)
- Practical use case: Processing track ID ranges for playlists

## Prerequisites

- Basic understanding of Rust
- Familiarity with `Result` type and error handling
- Understanding of vector data structures

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/parsing_utils/examples/basic_usage/Cargo.toml
```

Or from the example directory:

```bash
cd packages/parsing_utils/examples/basic_usage
cargo run
```

## Expected Output

```
=== MoosicBox Parsing Utils - Basic Usage Example ===

1. Parsing comma-separated integers:
   Input: "1,2,3,10,15"
   Output: [1, 2, 3, 10, 15]

2. Parsing a single integer:
   Input: "42"
   Output: [42]

3. Parsing a simple range:
   Input: "1-5"
   Output: [1, 2, 3, 4, 5]

4. Parsing range with comma-separated start:
   Input: "1,2,3-7"
   Output: [1, 2, 3, 4, 5, 6, 7]
   Explanation: Expands from 3 to 7, includes 1,2,3 at start and 7 at end

5. Parsing range with comma-separated start and end:
   Input: "1,2-5,6"
   Output: [1, 2, 3, 4, 5, 6]
   Explanation: Expands from 2 to 5, includes 1 at start and 5,6 at end

6. Parsing comma-separated values without ranges:
   Input: "10,20,30,40"
   Output: [10, 20, 30, 40]

7. Handling parse errors:
   Input: "1,abc,3"
   Error: Could not parse 'abc' as integer

8. Handling unmatched range errors:
   Input: "1-2-3" (odd number of range parts)
   Error: Malformed range syntax in '1-2-3'

9. Handling range too large errors:
   Input: "1-200000" (exceeds 100,000 item limit)
   Error: Range '2-200000' exceeds maximum size of 100,000 items

10. Practical use case - Processing a list of track IDs:
    Input: "1,3,5-10,15"
    Processing 9 tracks: [1, 3, 5, 6, 7, 8, 9, 10, 15]
    This could represent tracks to add to a playlist or queue

=== Example Complete ===
```

## Code Walkthrough

### Parsing Simple Sequences

The most basic operation is parsing comma-separated integers:

```rust
let result = parse_integer_sequences("1,2,3,10,15")?;
// Result: [1, 2, 3, 10, 15]
```

This function splits the string by commas and parses each segment as a `u64` integer.

### Parsing Simple Ranges

For ranges, use `parse_integer_ranges`:

```rust
let result = parse_integer_ranges("1-5")?;
// Result: [1, 2, 3, 4, 5]
```

The range is expanded to include all integers from the start to the end, inclusive.

### Parsing Complex Ranges

The parser supports comma-separated values at the boundaries of ranges:

```rust
// Range with comma-separated start
let result = parse_integer_ranges("1,2,3-7")?;
// Result: [1, 2, 3, 4, 5, 6, 7]

// Range with comma-separated start and end
let result = parse_integer_ranges("1,2-5,6")?;
// Result: [1, 2, 3, 4, 5, 6]
```

The parser:

1. Splits by hyphens to identify range boundaries
2. Parses comma-separated values at each boundary
3. Takes the last value from the start sequence and first value from the end sequence
4. Expands the range between them
5. Combines all values in order

### Error Handling

The example demonstrates handling all three error types:

```rust
// ParseId error - invalid integer
match parse_integer_sequences("1,abc,3") {
    Err(ParseIntegersError::ParseId(id)) => {
        println!("Could not parse '{}' as integer", id);
    }
    _ => {}
}

// UnmatchedRange error - malformed syntax
match parse_integer_ranges("1-2-3") {
    Err(ParseIntegersError::UnmatchedRange(range)) => {
        println!("Malformed range syntax in '{}'", range);
    }
    _ => {}
}

// RangeTooLarge error - exceeds 100,000 items
match parse_integer_ranges("1-200000") {
    Err(ParseIntegersError::RangeTooLarge(range)) => {
        println!("Range '{}' exceeds maximum size", range);
    }
    _ => {}
}
```

### Practical Example

The example concludes with a realistic use case:

```rust
let track_ids = parse_integer_ranges("1,3,5-10,15")?;
println!("Processing {} tracks: {:?}", track_ids.len(), track_ids);
```

This demonstrates how you might parse user input for selecting tracks to add to a playlist or queue.

## Key Concepts

### Range Expansion Algorithm

When the parser encounters a hyphen:

1. It splits the string by `-` characters
2. For each pair of segments (start and end):
    - Parses comma-separated integers in the start segment
    - Parses comma-separated integers in the end segment
    - Uses the **last** integer from start and **first** integer from end as range boundaries
    - Generates all integers between these boundaries
    - Combines start values, expanded range, and end values

### Error Prevention

The parser includes several safety mechanisms:

- **Parse validation**: Ensures all segments can be parsed as valid `u64` integers
- **Range validation**: Prevents ranges larger than 100,000 items to avoid memory exhaustion
- **Syntax validation**: Detects malformed range syntax (odd number of range parts greater than 1)

### Memory Considerations

Ranges are expanded into `Vec<u64>`, so:

- Each integer consumes 8 bytes
- A range of 100,000 integers uses approximately 800 KB
- The 100,000 item limit prevents excessive memory usage
- For very large ranges, consider using an iterator approach instead

## Testing the Example

The example is self-contained and demonstrates its own output. To verify:

1. Run the example and observe the output matches the expected output above
2. Note how each parsing operation produces the expected vector of integers
3. Observe how each error case is caught and handled appropriately
4. The practical example (item 10) shows how this could be used in a real application

## Troubleshooting

### Unexpected Parse Errors

**Problem**: Getting parse errors on seemingly valid input

**Solution**: Ensure there are no spaces or non-numeric characters in the input string. The parser expects pure numeric values separated by commas and hyphens.

### Unmatched Range Errors

**Problem**: Getting unmatched range errors on complex input

**Solution**: The parser splits on hyphens first. If you have more than two hyphen-separated segments and an odd number of them, it's considered invalid. Valid: `"1-5"`, `"1,2-5,6"`. Invalid: `"1-2-3"` (three segments).

### Range Too Large Errors

**Problem**: Hit the 100,000 item limit

**Solution**: Either break the range into smaller chunks or reconsider if you really need all those integers in memory at once. For large ranges, consider using an iterator approach or processing in batches.

## Related Examples

This is the only example for `moosicbox_parsing_utils` as it comprehensively covers all the package's functionality.
