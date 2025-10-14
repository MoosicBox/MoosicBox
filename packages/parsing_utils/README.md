# Parsing Utils

Utilities for parsing and processing integer sequences and ranges.

## Overview

The Parsing Utils package provides:

- **Integer Sequence Parsing**: Parse comma-separated integer lists
- **Integer Range Parsing**: Parse hyphen-separated integer ranges
- **Range Validation**: Prevent excessive range sizes
- **Error Handling**: Comprehensive error types for parsing failures

## Features

### Integer Sequence Parsing
- **Comma-separated Lists**: Parse "1,2,3,4,5" into Vec<u64>
- **Single Values**: Handle single integers as sequences
- **Validation**: Ensure all values are valid u64 integers

### Integer Range Parsing
- **Hyphen-separated Ranges**: Parse "1-10" into expanded sequence
- **Range Limits**: Prevent ranges larger than 100,000 items
- **Comma-separated Integers**: Handle comma-separated integers within range boundaries

### Error Handling
- **Parse Errors**: Invalid integer format detection
- **Unmatched Ranges**: Detect malformed range syntax
- **Size Limits**: Prevent memory exhaustion from large ranges

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
parsing_utils = { path = "../parsing_utils" }
```

## Usage

### Parse Integer Sequences

```rust
use parsing_utils::integer_range::parse_integer_sequences;

// Parse comma-separated integers
let result = parse_integer_sequences("1,2,3,4,5")?;
assert_eq!(result, vec![1, 2, 3, 4, 5]);

// Parse single integer
let result = parse_integer_sequences("42")?;
assert_eq!(result, vec![42]);

// Handle parsing errors
match parse_integer_sequences("1,abc,3") {
    Ok(numbers) => println!("Parsed: {:?}", numbers),
    Err(e) => println!("Parse error: {}", e),
}
```

### Parse Integer Ranges

```rust
use parsing_utils::integer_range::parse_integer_ranges;

// Parse simple range
let result = parse_integer_ranges("1-5")?;
assert_eq!(result, vec![1, 2, 3, 4, 5]);

// Parse comma-separated sequences (no ranges)
let result = parse_integer_ranges("1,3,5")?;
assert_eq!(result, vec![1, 3, 5]);

// Parse range with comma-separated start values
let result = parse_integer_ranges("1,2,3-7")?;
assert_eq!(result, vec![1, 2, 3, 4, 5, 6, 7]);

// Parse range with comma-separated start and end values
let result = parse_integer_ranges("1,2-5,6")?;
assert_eq!(result, vec![1, 2, 3, 4, 5, 6]);
```

### Error Handling

```rust
use parsing_utils::integer_range::{parse_integer_ranges, ParseIntegersError};

// Handle different error types
match parse_integer_ranges("1-100000000") {
    Ok(numbers) => println!("Parsed {} numbers", numbers.len()),
    Err(ParseIntegersError::RangeTooLarge(range)) => {
        println!("Range too large: {}", range);
    }
    Err(ParseIntegersError::ParseId(id)) => {
        println!("Invalid integer: {}", id);
    }
    Err(ParseIntegersError::UnmatchedRange(range)) => {
        println!("Malformed range: {}", range);
    }
}
```

## Range Parsing Logic

### Simple Sequences
- Input: `"1,2,3,4"`
- Output: `[1, 2, 3, 4]`

### Simple Ranges
- Input: `"1-5"`
- Output: `[1, 2, 3, 4, 5]`

### Ranges with Commas
- Input: `"1,2,3-7,8"`
- Processing:
  1. Split by hyphens: `["1,2,3", "7,8"]`
  2. Parse start sequence: `"1,2,3"` → `[1, 2, 3]`
  3. Parse end sequence: `"7,8"` → `[7, 8]`
  4. Take last from start (`3`) and first from end (`7`)
  5. Expand range from `4` to `6`: `[4, 5, 6]`
  6. Combine: `[1, 2, 3, 4, 5, 6, 7, 8]`

**Note**: The function splits on hyphens first, treating them as range delimiters. Commas are parsed within the start and end portions of a range.

### Range Validation
- Maximum range size: 100,000 items
- Prevents memory exhaustion attacks
- Returns `RangeTooLarge` error for excessive ranges

## Error Types

### ParseIntegersError

```rust
pub enum ParseIntegersError {
    // Invalid integer format
    ParseId(String),

    // Malformed range syntax
    UnmatchedRange(String),

    // Range exceeds 100,000 items
    RangeTooLarge(String),
}
```

### Error Examples

```rust
use parsing_utils::integer_range::parse_integer_ranges;

// ParseId error
let result = parse_integer_ranges("1,abc,3");
// Error: ParseId("abc")

// UnmatchedRange error (odd number of range parts > 1)
let result = parse_integer_ranges("1-2-3");
// Error: UnmatchedRange("1-2-3")

// RangeTooLarge error
let result = parse_integer_ranges("1-200000");
// Error: RangeTooLarge("1-200000")
```

## Performance Considerations

- **Memory Usage**: Large ranges are expanded into Vec<u64>
- **Range Limits**: 100,000 item maximum prevents excessive memory usage
- **Parsing Speed**: Simple split-and-parse approach for efficiency
- **Error Handling**: Early validation prevents unnecessary processing

## Use Cases

- **ID Range Processing**: Parse user input for ID ranges
- **Batch Operations**: Process sequences of items
- **Configuration Parsing**: Parse numeric configuration values
- **Data Import**: Handle CSV-style numeric data
- **API Parameters**: Parse query parameters with ranges

## Dependencies

- **thiserror**: Error handling and display traits
