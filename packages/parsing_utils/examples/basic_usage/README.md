# Integer Parsing Example

Demonstrates how to parse integer sequences and ranges using moosicbox_parsing_utils.

## Summary

This example shows how to use the parsing utilities to parse comma-separated integer sequences and hyphen-separated ranges, including proper error handling for invalid inputs.

## What This Example Demonstrates

- Parsing simple comma-separated integer sequences
- Parsing single integers
- Parsing simple hyphen-separated ranges (e.g., "1-5")
- Parsing comma-separated integers without ranges
- Parsing ranges with comma-separated start and end values
- Parsing multiple ranges in a single input
- Handling parsing errors (invalid integers, malformed ranges, ranges too large)
- Practical use case: processing ID ranges from user input

## Prerequisites

- Basic understanding of Rust
- Familiarity with Result types and error handling in Rust
- Understanding of iterators and vectors

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/parsing_utils/examples/basic_usage/Cargo.toml
```

Or from the example directory:

```bash
cd packages/parsing_utils/examples/basic_usage
cargo run
```

## Expected Output

The example will display 11 different parsing scenarios with their inputs and outputs:

```
=== MoosicBox Parsing Utils - Basic Usage Example ===

1. Parsing Simple Comma-Separated Sequences
   Input: "1,2,3,4,5"
   Output: [1, 2, 3, 4, 5]

2. Parsing Single Integer
   Input: "42"
   Output: [42]

3. Parsing Simple Range
   Input: "1-5"
   Output: [1, 2, 3, 4, 5]

4. Parsing Comma-Separated Integers (No Ranges)
   Input: "1,3,5,7,9"
   Output: [1, 3, 5, 7, 9]

5. Parsing Range with Comma-Separated Start Values
   Input: "1,2,3-7"
   Output: [1, 2, 3, 4, 5, 6, 7]

6. Parsing Range with Comma-Separated Start and End Values
   Input: "1,2-5,6"
   Output: [1, 2, 3, 4, 5, 6]

7. Parsing Multiple Ranges
   Input: "1,2-5,10"
   Output: [1, 2, 3, 4, 5, 10]

8. Error Handling - Invalid Integer
   Input: "1,abc,3"
   Error: Could not parse 'abc' as an integer

9. Error Handling - Unmatched Range
   Input: "1-2-3"
   Error: Malformed range specification: '1-2-3'

10. Error Handling - Range Too Large
    Input: "1-200000"
    Error: Range '1-199999' exceeds maximum size of 100,000 items

11. Practical Use Case - Parsing ID Ranges
    Processing IDs: "100,101,105-110,150"
    Parsed 9 IDs: [100, 101, 105, 106, 107, 108, 109, 110, 150]
    First ID: 100, Last ID: 150

=== All examples completed successfully! ===
```

## Code Walkthrough

### Setting Up the Example

The example imports the necessary functions and error types:

```rust
use moosicbox_parsing_utils::integer_range::{
    parse_integer_ranges, parse_integer_sequences, ParseIntegersError,
};
```

### Parsing Simple Sequences

The `parse_integer_sequences` function parses comma-separated integers:

```rust
let result = parse_integer_sequences("1,2,3,4,5")?;
// Result: [1, 2, 3, 4, 5]
```

This function splits the input on commas and attempts to parse each segment as a `u64` integer.

### Parsing Ranges

The `parse_integer_ranges` function handles more complex inputs including ranges:

```rust
let result = parse_integer_ranges("1-5")?;
// Result: [1, 2, 3, 4, 5]
```

The function splits on hyphens to identify range boundaries, then expands the range into a sequence of integers.

### Complex Range Parsing

The parser can handle comma-separated values within range specifications:

```rust
let result = parse_integer_ranges("1,2,3-7,8")?;
// Result: [1, 2, 3, 4, 5, 6, 7, 8]
```

The parsing logic:

1. Splits by hyphens: `["1,2,3", "7,8"]`
2. Parses start sequence: `"1,2,3"` → `[1, 2, 3]`
3. Parses end sequence: `"7,8"` → `[7, 8]`
4. Takes the last from start (`3`) and first from end (`7`)
5. Expands the range from `4` to `6`: `[4, 5, 6]`
6. Combines all: `[1, 2, 3, 4, 5, 6, 7, 8]`

### Error Handling

The example demonstrates three types of errors:

**Invalid Integer Format:**

```rust
match parse_integer_sequences("1,abc,3") {
    Err(ParseIntegersError::ParseId(id)) => {
        println!("Could not parse '{}' as an integer", id);
    }
    // ...
}
```

**Malformed Range:**

```rust
match parse_integer_ranges("1-2-3") {
    Err(ParseIntegersError::UnmatchedRange(range)) => {
        println!("Malformed range specification: '{}'", range);
    }
    // ...
}
```

**Range Too Large:**

```rust
match parse_integer_ranges("1-200000") {
    Err(ParseIntegersError::RangeTooLarge(range)) => {
        println!("Range '{}' exceeds maximum size of 100,000 items", range);
    }
    // ...
}
```

### Practical Use Case

The example concludes with a practical scenario: parsing ID ranges from user input:

```rust
let user_input = "100,101,105-110,150";
let ids = parse_integer_ranges(user_input)?;
println!("Parsed {} IDs: {:?}", ids.len(), ids);
```

This demonstrates how you might use the parsing utilities in a real application to handle user-provided ID ranges for batch operations, data exports, or similar tasks.

## Key Concepts

### Sequence vs Range Parsing

- **`parse_integer_sequences`**: Only handles comma-separated integers. Simpler and faster when you know your input has no ranges.
- **`parse_integer_ranges`**: Handles both comma-separated integers AND hyphen-separated ranges. More flexible but slightly more complex.

### Range Expansion

When a range is specified (e.g., "1-5"), the parser expands it into a complete sequence of integers. This is useful for user interfaces where typing "1-100" is more convenient than listing all 100 numbers.

### Memory Protection

The parser includes a safety limit: ranges cannot exceed 100,000 items. This prevents memory exhaustion from malicious or accidental inputs like "1-999999999".

### Error Recovery

All parsing functions return `Result` types, allowing you to handle errors gracefully:

- Validate user input before processing
- Provide helpful error messages
- Continue processing other inputs if one fails

## Testing the Example

Try modifying the example with your own test cases:

1. **Add a new parsing scenario**: Add another example in `main()` to test a specific input pattern
2. **Test edge cases**: Try inputs like `"0"`, `"1-1"`, or `"99999-100000"`
3. **Experiment with formats**: Test different combinations of commas and hyphens
4. **Error testing**: Try various invalid inputs to see how errors are reported

Example modifications:

```rust
// Test very large single numbers
let result = parse_integer_sequences("18446744073709551615")?;
println!("Max u64: {:?}", result);

// Test consecutive ranges
let result = parse_integer_ranges("1-10-20")?;
// This will error - demonstrates unmatched range validation
```

## Troubleshooting

### "Could not parse integers" Error

This occurs when a segment cannot be parsed as a valid `u64` integer. Check for:

- Non-numeric characters (letters, symbols)
- Negative numbers (not supported - use `u64`)
- Numbers larger than `18446744073709551615` (u64 maximum)
- Extra whitespace (trim your input first)

### "Unmatched range" Error

This occurs when the range syntax is invalid. The parser expects:

- Single value: `"5"`
- Comma-separated values: `"1,2,3"`
- Single range: `"1-10"`
- Even number of range parts when more than 2 hyphens are used

Invalid examples:

- `"1-2-3"` (odd number of range parts)
- `"1-2-3-4-5"` (odd number of range parts)

### "Range too large" Error

This occurs when a range would expand to more than 100,000 items. Solutions:

- Break the range into smaller chunks
- Process the range in batches
- Reconsider whether you need to expand the entire range into memory

## Related Examples

This is currently the only example for `moosicbox_parsing_utils`. For related parsing functionality, see:

- `moosicbox_date_utils/examples/basic_parsing` - Date parsing utilities
- `moosicbox_config/examples/basic_usage` - Configuration value parsing
