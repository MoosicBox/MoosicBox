#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `moosicbox_date_utils`.
//!
//! This example demonstrates parsing various date and time string formats
//! using the `parse_date_time` function.

use moosicbox_date_utils::chrono::parse_date_time;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("MoosicBox Date Utils - Basic Usage Example\n");
    println!("===========================================\n");

    // Parse a year only (defaults to January 1st)
    println!("1. Parsing year only:");
    let year_only = parse_date_time("2024")?;
    println!("   Input:  \"2024\"");
    println!("   Output: {year_only}\n");

    // Parse a date only (defaults to midnight)
    println!("2. Parsing date only:");
    let date_only = parse_date_time("2024-10-31")?;
    println!("   Input:  \"2024-10-31\"");
    println!("   Output: {date_only}\n");

    // Parse ISO datetime with Z suffix (UTC timezone)
    println!("3. Parsing ISO datetime with Z suffix:");
    let utc_datetime = parse_date_time("2024-10-31T14:30:45Z")?;
    println!("   Input:  \"2024-10-31T14:30:45Z\"");
    println!("   Output: {utc_datetime}\n");

    // Parse ISO datetime with timezone offset
    println!("4. Parsing ISO datetime with timezone:");
    let tz_datetime = parse_date_time("2024-10-31T14:30:45.123+00:00")?;
    println!("   Input:  \"2024-10-31T14:30:45.123+00:00\"");
    println!("   Output: {tz_datetime}\n");

    // Parse ISO datetime with fractional seconds
    println!("5. Parsing ISO datetime with fractional seconds:");
    let fractional_datetime = parse_date_time("2024-10-31T14:30:45.123456")?;
    println!("   Input:  \"2024-10-31T14:30:45.123456\"");
    println!("   Output: {fractional_datetime}\n");

    // Demonstrate error handling with invalid format
    println!("6. Error handling with invalid format:");
    let invalid_input = "invalid-date-format";
    match parse_date_time(invalid_input) {
        Ok(dt) => println!("   Unexpectedly parsed: {dt}"),
        Err(e) => println!("   Input:  \"{invalid_input}\"\n   Error:  {e}"),
    }

    println!("\n===========================================");
    println!("Example completed successfully!");

    Ok(())
}
