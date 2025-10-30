# Basic Date Parsing Example

A comprehensive example demonstrating how to parse various date and time formats using the `moosicbox_date_utils` package.

## Summary

This example showcases the `parse_date_time` function's ability to automatically parse multiple date and time string formats, from simple year-only inputs to full ISO 8601 timestamps with fractional seconds and timezone information.

## What This Example Demonstrates

- Parsing year-only format (e.g., "2024")
- Parsing ISO date format (e.g., "2024-10-24")
- Parsing ISO datetime with UTC indicator (e.g., "2024-10-24T15:30:45Z")
- Parsing ISO datetime with timezone offset (e.g., "2024-10-24T15:30:45.123456+00:00")
- Parsing ISO datetime with fractional seconds (e.g., "2024-10-24T15:30:45.999999")
- Proper error handling for invalid date strings
- Understanding `NaiveDateTime` return type

## Prerequisites

- Rust toolchain installed
- Basic understanding of date/time concepts
- Familiarity with Rust's `Result` type for error handling

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/date_utils/examples/basic_parsing/Cargo.toml
```

Or from the example directory:

```bash
cd packages/date_utils/examples/basic_parsing
cargo run
```

## Expected Output

```
MoosicBox Date Utils - Basic Parsing Example
=============================================

1. Parsing year only:
   Input:  "2024"
   Result: 2024-01-01 00:00:00
   Note:   Defaults to January 1st at midnight

2. Parsing ISO date:
   Input:  "2024-10-24"
   Result: 2024-10-24 00:00:00
   Note:   Defaults to midnight (00:00:00)

3. Parsing ISO datetime with Z suffix:
   Input:  "2024-10-24T15:30:45Z"
   Result: 2024-10-24 15:30:45
   Note:   Z indicates UTC timezone

4. Parsing ISO datetime with timezone offset:
   Input:  "2024-10-24T15:30:45.123456+00:00"
   Result: 2024-10-24 15:30:45.123456
   Note:   Includes fractional seconds and timezone offset

5. Parsing ISO datetime with fractional seconds:
   Input:  "2024-10-24T15:30:45.999999"
   Result: 2024-10-24 15:30:45.999999
   Note:   Supports microsecond precision

6. Error handling example:
   Input:  "not-a-date"
   Error:  Failed to parse (as expected)
   Detail: input contains invalid characters

✓ Example completed successfully!

Key takeaways:
  • parse_date_time() supports multiple formats automatically
  • Returns NaiveDateTime (no timezone information retained)
  • Use Result for proper error handling
  • Fractional seconds up to microsecond precision supported
```

## Code Walkthrough

### Importing the Function

```rust
use moosicbox_date_utils::chrono::parse_date_time;
```

The `parse_date_time` function is the primary API for parsing date/time strings.

### Parsing Different Formats

The function automatically detects the format:

```rust
// Year only - defaults to January 1st at midnight
let year_only = parse_date_time("2024")?;

// ISO date - defaults to midnight
let date_only = parse_date_time("2024-10-24")?;

// Full ISO datetime with UTC indicator
let datetime_z = parse_date_time("2024-10-24T15:30:45Z")?;

// With timezone offset and fractional seconds
let datetime_tz = parse_date_time("2024-10-24T15:30:45.123456+00:00")?;

// With fractional seconds (microsecond precision)
let datetime_frac = parse_date_time("2024-10-24T15:30:45.999999")?;
```

### Error Handling

The function returns a `Result`, allowing proper error handling:

```rust
match parse_date_time(invalid_input) {
    Ok(dt) => println!("Parsed: {}", dt),
    Err(e) => println!("Failed to parse: {}", e),
}
```

## Key Concepts

### NaiveDateTime

The `parse_date_time` function returns `chrono::NaiveDateTime`, which represents a date and time without timezone information. Even when parsing strings with timezone indicators (like "Z" or "+00:00"), the timezone information is used for parsing but not retained in the result.

### Format Detection

The function uses a series of heuristics to detect the format:

1. **Length check for year**: If 4 characters or less and numeric, treats as year
2. **Length check for date**: If exactly 10 characters, attempts ISO date format
3. **Suffix checks**: Looks for "Z" or "+00:00" to determine parsing strategy
4. **Fallback**: Attempts to parse as ISO datetime with fractional seconds

### Fractional Seconds

The parser supports fractional seconds up to microsecond precision (6 decimal places). The `%.f` format specifier in chrono handles variable-length fractional seconds.

### Default Values

When parsing partial date/time information:

- **Year only**: Defaults to January 1st at 00:00:00
- **Date only**: Defaults to 00:00:00 (midnight)

## Testing the Example

The example is self-contained and doesn't require user interaction. Simply run it to see the parsing results.

To experiment with different formats:

1. Modify the date strings in `main.rs`
2. Add new parsing examples
3. Try edge cases like leap years, month boundaries, etc.

## Troubleshooting

### "input contains invalid characters"

This error occurs when the string doesn't match any of the supported formats. Ensure your input follows one of these patterns:

- Year: `"2024"`
- Date: `"2024-10-24"`
- DateTime with Z: `"2024-10-24T15:30:45Z"`
- DateTime with timezone: `"2024-10-24T15:30:45.123456+00:00"`
- DateTime with fractions: `"2024-10-24T15:30:45.999999"`

### Invalid Dates

The parser will reject invalid dates like "2024-02-30" or "2024-13-01". Use valid calendar dates only.

## Related Examples

This is the primary example for `moosicbox_date_utils`. For more advanced date/time operations, refer to the [chrono documentation](https://docs.rs/chrono/).
