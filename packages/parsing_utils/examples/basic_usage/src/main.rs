#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `moosicbox_parsing_utils`
//!
//! This example demonstrates parsing integer sequences and ranges from strings.

use moosicbox_parsing_utils::integer_range::{
    ParseIntegersError, parse_integer_ranges, parse_integer_sequences,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MoosicBox Parsing Utils - Basic Usage Example ===\n");

    // Example 1: Parse simple comma-separated integers
    println!("1. Parsing comma-separated integers:");
    println!("   Input: \"1,2,3,10,15\"");
    let result = parse_integer_sequences("1,2,3,10,15")?;
    println!("   Output: {result:?}\n");

    // Example 2: Parse a single integer
    println!("2. Parsing a single integer:");
    println!("   Input: \"42\"");
    let result = parse_integer_sequences("42")?;
    println!("   Output: {result:?}\n");

    // Example 3: Parse a simple range
    println!("3. Parsing a simple range:");
    println!("   Input: \"1-5\"");
    let result = parse_integer_ranges("1-5")?;
    println!("   Output: {result:?}\n");

    // Example 4: Parse ranges with comma-separated values
    println!("4. Parsing range with comma-separated start:");
    println!("   Input: \"1,2,3-7\"");
    let result = parse_integer_ranges("1,2,3-7")?;
    println!("   Output: {result:?}");
    println!("   Explanation: Expands from 3 to 7, includes 1,2,3 at start and 7 at end\n");

    // Example 5: Parse ranges with comma-separated start and end
    println!("5. Parsing range with comma-separated start and end:");
    println!("   Input: \"1,2-5,6\"");
    let result = parse_integer_ranges("1,2-5,6")?;
    println!("   Output: {result:?}");
    println!("   Explanation: Expands from 2 to 5, includes 1 at start and 5,6 at end\n");

    // Example 6: Parse comma-separated values (no range)
    println!("6. Parsing comma-separated values without ranges:");
    println!("   Input: \"10,20,30,40\"");
    let result = parse_integer_ranges("10,20,30,40")?;
    println!("   Output: {result:?}\n");

    // Example 7: Handle parse errors
    println!("7. Handling parse errors:");
    println!("   Input: \"1,abc,3\"");
    match parse_integer_sequences("1,abc,3") {
        Ok(numbers) => println!("   Unexpected success: {numbers:?}"),
        Err(ParseIntegersError::ParseId(id)) => {
            println!("   Error: Could not parse '{id}' as integer");
        }
        Err(e) => println!("   Error: {e}"),
    }
    println!();

    // Example 8: Handle unmatched range errors
    println!("8. Handling unmatched range errors:");
    println!("   Input: \"1-2-3\" (odd number of range parts)");
    match parse_integer_ranges("1-2-3") {
        Ok(numbers) => println!("   Unexpected success: {numbers:?}"),
        Err(ParseIntegersError::UnmatchedRange(range)) => {
            println!("   Error: Malformed range syntax in '{range}'");
        }
        Err(e) => println!("   Error: {e}"),
    }
    println!();

    // Example 9: Handle range too large errors
    println!("9. Handling range too large errors:");
    println!("   Input: \"1-200000\" (exceeds 100,000 item limit)");
    match parse_integer_ranges("1-200000") {
        Ok(numbers) => {
            let len = numbers.len();
            println!("   Unexpected success: {len} items");
        }
        Err(ParseIntegersError::RangeTooLarge(range)) => {
            println!("   Error: Range '{range}' exceeds maximum size of 100,000 items");
        }
        Err(e) => println!("   Error: {e}"),
    }
    println!();

    // Example 10: Practical use case - Processing ID ranges
    println!("10. Practical use case - Processing a list of track IDs:");
    println!("    Input: \"1,3,5-10,15\"");
    let track_ids = parse_integer_ranges("1,3,5-10,15")?;
    let len = track_ids.len();
    println!("    Processing {len} tracks: {track_ids:?}");
    println!("    This could represent tracks to add to a playlist or queue\n");

    println!("=== Example Complete ===");

    Ok(())
}
