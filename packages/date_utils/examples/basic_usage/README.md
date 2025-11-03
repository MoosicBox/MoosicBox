# Basic Usage Example

A comprehensive example demonstrating the date and time parsing capabilities of `moosicbox_date_utils`.

## Summary

This example shows how to use the `parse_date_time` function to parse various date and time string formats into `NaiveDateTime` objects, including proper error handling.

## What This Example Demonstrates

- Parsing year-only strings (e.g., "2024")
- Parsing date-only strings (e.g., "2024-10-31")
- Parsing ISO 8601 datetime with Z suffix (UTC)
- Parsing ISO 8601 datetime with timezone offset
- Parsing ISO 8601 datetime with fractional seconds
- Proper error handling for invalid date formats

## Prerequisites

- Basic understanding of Rust
- Familiarity with date/time concepts (ISO 8601 format, timezones)
- Rust toolchain installed (cargo)

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/date_utils/examples/basic_usage/Cargo.toml
```

Or from within the example directory:

```bash
cd packages/date_utils/examples/basic_usage
cargo run
```

## Expected Output

```
MoosicBox Date Utils - Basic Usage Example

===========================================

1. Parsing year only:
   Input:  "2024"
   Output: 2024-01-01 00:00:00

2. Parsing date only:
   Input:  "2024-10-31"
   Output: 2024-10-31 00:00:00

3. Parsing ISO datetime with Z suffix:
   Input:  "2024-10-31T14:30:45Z"
   Output: 2024-10-31 14:30:45

4. Parsing ISO datetime with timezone:
   Input:  "2024-10-31T14:30:45.123+00:00"
   Output: 2024-10-31 14:30:45.123

5. Parsing ISO datetime with fractional seconds:
   Input:  "2024-10-31T14:30:45.123456"
   Output: 2024-10-31 14:30:45.123456

6. Error handling with invalid format:
   Input:  "invalid-date-format"
   Error:  input contains invalid characters

===========================================
Example completed successfully!
```

## Code Walkthrough

### Importing the Parser

```rust
use moosicbox_date_utils::chrono::parse_date_time;
```

The `parse_date_time` function is the main utility for parsing date/time strings.

### Parsing Year Only

```rust
let year_only = parse_date_time("2024")?;
```

When parsing a year-only string (4 digits or less), the function automatically defaults to January 1st at midnight.

### Parsing Date Only

```rust
let date_only = parse_date_time("2024-10-31")?;
```

Date-only strings in `YYYY-MM-DD` format are parsed to midnight (00:00:00) on that date.

### Parsing Full ISO Datetimes

```rust
// With Z suffix (UTC)
let utc_datetime = parse_date_time("2024-10-31T14:30:45Z")?;

// With timezone offset
let tz_datetime = parse_date_time("2024-10-31T14:30:45.123+00:00")?;

// With fractional seconds
let fractional_datetime = parse_date_time("2024-10-31T14:30:45.123456")?;
```

The function supports multiple ISO 8601 datetime formats, including timezone information and fractional seconds.

### Error Handling

```rust
match parse_date_time(invalid_input) {
    Ok(dt) => println!("Unexpectedly parsed: {dt}"),
    Err(e) => println!("Error: {e}"),
}
```

The function returns a `Result<NaiveDateTime, chrono::ParseError>`, allowing you to handle parsing failures gracefully.

## Key Concepts

### Format Auto-Detection

The `parse_date_time` function automatically detects the format of the input string based on its length and characteristics:

- **4 digits or less**: Treated as a year
- **10 characters**: Treated as a date (YYYY-MM-DD)
- **Ends with 'Z'**: ISO datetime with UTC timezone
- **Ends with '+00:00'**: ISO datetime with explicit timezone offset
- **Otherwise**: ISO datetime with optional fractional seconds

### NaiveDateTime

The function returns a `chrono::NaiveDateTime`, which represents a datetime without timezone information. This is useful when you need to work with dates and times in a timezone-agnostic way.

### Error Logging

The function internally logs parsing errors using the `log` crate, which can be helpful for debugging issues in production environments.

## Testing the Example

1. Run the example and verify all parsing operations succeed
2. Modify the input strings to test different date formats
3. Try invalid inputs to see how errors are handled
4. Experiment with edge cases like leap years or day boundaries

## Troubleshooting

### Common Issues

**Issue**: Example fails to compile
**Solution**: Ensure you're using the workspace root as your working directory and running with the full manifest path.

**Issue**: Unexpected parsing results
**Solution**: Verify your input string matches one of the supported formats. Check the function documentation for supported format patterns.

**Issue**: Error messages are too verbose
**Solution**: The function uses the `log` crate for error logging. Configure your log level to reduce verbosity in production.

## Related Examples

This is currently the only example for `moosicbox_date_utils`. The package provides a focused utility for date/time parsing, and this example comprehensively demonstrates its capabilities.
