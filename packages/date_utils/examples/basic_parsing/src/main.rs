#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic date parsing example demonstrating `moosicbox_date_utils` functionality.
//!
//! This example shows how to parse various date and time formats using the
//! `parse_date_time` function.

use moosicbox_date_utils::chrono::parse_date_time;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("MoosicBox Date Utils - Basic Parsing Example");
    println!("=============================================\n");

    // Parse year only (4 digits or less)
    println!("1. Parsing year only:");
    let year_only = parse_date_time("2024")?;
    println!("   Input:  \"2024\"");
    println!("   Result: {year_only}");
    println!("   Note:   Defaults to January 1st at midnight\n");

    // Parse ISO date (YYYY-MM-DD)
    println!("2. Parsing ISO date:");
    let date_only = parse_date_time("2024-10-24")?;
    println!("   Input:  \"2024-10-24\"");
    println!("   Result: {date_only}");
    println!("   Note:   Defaults to midnight (00:00:00)\n");

    // Parse ISO datetime with Z suffix (UTC)
    println!("3. Parsing ISO datetime with Z suffix:");
    let datetime_utc = parse_date_time("2024-10-24T15:30:45Z")?;
    println!("   Input:  \"2024-10-24T15:30:45Z\"");
    println!("   Result: {datetime_utc}");
    println!("   Note:   Z indicates UTC timezone\n");

    // Parse ISO datetime with timezone offset
    println!("4. Parsing ISO datetime with timezone offset:");
    let datetime_with_offset = parse_date_time("2024-10-24T15:30:45.123456+00:00")?;
    println!("   Input:  \"2024-10-24T15:30:45.123456+00:00\"");
    println!("   Result: {datetime_with_offset}");
    println!("   Note:   Includes fractional seconds and timezone offset\n");

    // Parse ISO datetime with fractional seconds
    println!("5. Parsing ISO datetime with fractional seconds:");
    let datetime_with_micros = parse_date_time("2024-10-24T15:30:45.999999")?;
    println!("   Input:  \"2024-10-24T15:30:45.999999\"");
    println!("   Result: {datetime_with_micros}");
    println!("   Note:   Supports microsecond precision\n");

    // Demonstrate error handling
    println!("6. Error handling example:");
    let invalid_input = "not-a-date";
    match parse_date_time(invalid_input) {
        Ok(dt) => println!("   Unexpectedly parsed: {dt}"),
        Err(e) => {
            println!("   Input:  \"{invalid_input}\"");
            println!("   Error:  Failed to parse (as expected)");
            println!("   Detail: {e}");
        }
    }

    println!("\n✓ Example completed successfully!");
    println!("\nKey takeaways:");
    println!("  • parse_date_time() supports multiple formats automatically");
    println!("  • Returns NaiveDateTime (no timezone information retained)");
    println!("  • Use Result for proper error handling");
    println!("  • Fractional seconds up to microsecond precision supported");

    Ok(())
}
