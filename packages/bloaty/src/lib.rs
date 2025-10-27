//! Binary size analysis tool for Rust workspace packages.
//!
//! `bloaty` is a command-line tool that measures and compares the size impact of Cargo features
//! on both library and binary targets in a Rust workspace. It builds packages with different
//! feature combinations and generates detailed size reports in multiple formats.
//!
//! This crate provides only a binary executable. For usage documentation, run:
//!
//! ```bash
//! bloaty --help
//! ```
//!
//! # Features
//!
//! * Feature size analysis for rlib and binary targets
//! * Multiple output formats (text, JSON, JSONL)
//! * Package and feature filtering with regex patterns
//! * Integration with cargo-bloat, cargo-llvm-lines, and cargo-size
//!
//! # Example Usage
//!
//! Analyze all workspace packages:
//!
//! ```bash
//! bloaty
//! ```
//!
//! Analyze specific packages with feature filtering:
//!
//! ```bash
//! bloaty --package moosicbox_core --skip-features fail-on-warnings
//! ```
//!
//! Generate JSON report:
//!
//! ```bash
//! bloaty --output-format json --report-file analysis
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
