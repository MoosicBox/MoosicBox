//! Utilities for parsing and processing text input.
//!
//! This crate provides utilities for parsing integers from strings, including support for
//! comma-separated sequences and hyphen-separated ranges.
//!
//! # Examples
//!
//! Parse comma-separated integers:
//!
//! ```rust
//! use moosicbox_parsing_utils::integer_range::parse_integer_sequences;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let result = parse_integer_sequences("1,2,3,10")?;
//! assert_eq!(result, vec![1, 2, 3, 10]);
//! # Ok(())
//! # }
//! ```
//!
//! Parse ranges:
//!
//! ```rust
//! use moosicbox_parsing_utils::integer_range::parse_integer_ranges;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let result = parse_integer_ranges("1,2-5,10")?;
//! assert_eq!(result, vec![1, 2, 3, 4, 5, 10]);
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub mod integer_range;
