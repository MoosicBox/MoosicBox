#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `moosicbox_parsing_utils`.
//!
//! This example demonstrates how to parse integer sequences and ranges using the parsing utilities.

use moosicbox_parsing_utils::integer_range::{
    ParseIntegersError, parse_integer_ranges, parse_integer_sequences,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MoosicBox Parsing Utils - Basic Usage Example ===\n");

    // Example 1: Parse simple comma-separated sequences
    println!("1. Parsing Simple Comma-Separated Sequences");
    println!("   Input: \"1,2,3,4,5\"");
    let result = parse_integer_sequences("1,2,3,4,5")?;
    println!("   Output: {result:?}\n");

    // Example 2: Parse single integer
    println!("2. Parsing Single Integer");
    println!("   Input: \"42\"");
    let result = parse_integer_sequences("42")?;
    println!("   Output: {result:?}\n");

    // Example 3: Parse simple range
    println!("3. Parsing Simple Range");
    println!("   Input: \"1-5\"");
    let result = parse_integer_ranges("1-5")?;
    println!("   Output: {result:?}\n");

    // Example 4: Parse comma-separated integers (no ranges)
    println!("4. Parsing Comma-Separated Integers (No Ranges)");
    println!("   Input: \"1,3,5,7,9\"");
    let result = parse_integer_ranges("1,3,5,7,9")?;
    println!("   Output: {result:?}\n");

    // Example 5: Parse range with comma-separated start values
    println!("5. Parsing Range with Comma-Separated Start Values");
    println!("   Input: \"1,2,3-7\"");
    let result = parse_integer_ranges("1,2,3-7")?;
    println!("   Output: {result:?}\n");

    // Example 6: Parse range with comma-separated start and end values
    println!("6. Parsing Range with Comma-Separated Start and End Values");
    println!("   Input: \"1,2-5,6\"");
    let result = parse_integer_ranges("1,2-5,6")?;
    println!("   Output: {result:?}\n");

    // Example 7: Parse multiple ranges
    println!("7. Parsing Multiple Ranges");
    println!("   Input: \"1,2-5,10\"");
    let result = parse_integer_ranges("1,2-5,10")?;
    println!("   Output: {result:?}\n");

    // Example 8: Demonstrate error handling for invalid integers
    println!("8. Error Handling - Invalid Integer");
    println!("   Input: \"1,abc,3\"");
    match parse_integer_sequences("1,abc,3") {
        Ok(numbers) => println!("   Result: {numbers:?}"),
        Err(ParseIntegersError::ParseId(id)) => {
            println!("   Error: Could not parse '{id}' as an integer");
        }
        Err(e) => println!("   Error: {e}"),
    }
    println!();

    // Example 9: Demonstrate error handling for unmatched ranges
    println!("9. Error Handling - Unmatched Range");
    println!("   Input: \"1-2-3\"");
    match parse_integer_ranges("1-2-3") {
        Ok(numbers) => println!("   Result: {numbers:?}"),
        Err(ParseIntegersError::UnmatchedRange(range)) => {
            println!("   Error: Malformed range specification: '{range}'");
        }
        Err(e) => println!("   Error: {e}"),
    }
    println!();

    // Example 10: Demonstrate error handling for ranges that are too large
    println!("10. Error Handling - Range Too Large");
    println!("    Input: \"1-200000\"");
    match parse_integer_ranges("1-200000") {
        Ok(numbers) => {
            let len = numbers.len();
            println!("    Result: {len} numbers");
        }
        Err(ParseIntegersError::RangeTooLarge(range)) => {
            println!("    Error: Range '{range}' exceeds maximum size of 100,000 items");
        }
        Err(e) => println!("    Error: {e}"),
    }
    println!();

    // Example 11: Practical use case - parsing ID ranges from user input
    println!("11. Practical Use Case - Parsing ID Ranges");
    let user_input = "100,101,105-110,150";
    println!("    Processing IDs: \"{user_input}\"");
    let ids = parse_integer_ranges(user_input)?;
    let len = ids.len();
    println!("    Parsed {len} IDs: {ids:?}");
    let first = ids[0];
    let last = ids[ids.len() - 1];
    println!("    First ID: {first}, Last ID: {last}\n");

    println!("=== All examples completed successfully! ===");

    Ok(())
}
