# MoosicBox ARB

Arbitrary data generation utilities for testing and property-based testing.

## Overview

The MoosicBox ARB package provides:

- **XML Generation**: Arbitrary XML string and attribute generation
- **CSS Generation**: CSS-safe string generation for testing
- **Serde Integration**: Serialization testing utilities
- **QuickCheck Integration**: Property-based testing support

## Features

### XML Utilities
- **XmlString**: Valid XML character string generation
- **XmlAttrNameString**: XML attribute name generation
- **Character Validation**: XML character validity checking

### CSS Utilities
- **CSS-safe Strings**: Generate valid CSS identifier strings
- **Testing Support**: CSS parsing and generation testing

### Serde Support
- **Serialization Testing**: Arbitrary data for serde testing
- **Property Testing**: Roundtrip serialization validation

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_arb = { path = "../arb" }

# Enable specific features
moosicbox_arb = {
    path = "../arb",
    features = ["xml", "css", "serde"]
}
```

## Usage

### XML String Generation

```rust
use moosicbox_arb::xml::{XmlString, XmlAttrNameString};
use quickcheck::{quickcheck, TestResult};

quickcheck! {
    fn valid_xml_strings(xml_str: XmlString) -> TestResult {
        // Test with valid XML strings
        TestResult::from_bool(is_valid_xml(&xml_str.0))
    }

    fn valid_xml_attributes(attr: XmlAttrNameString) -> TestResult {
        // Test with valid XML attribute names
        TestResult::from_bool(is_valid_attr_name(&attr.0))
    }
}
```

## Feature Flags

- **`xml`**: Enable XML string generation utilities
- **`css`**: Enable CSS string generation utilities
- **`serde`**: Enable serde integration utilities

## Dependencies

- **QuickCheck**: Property-based testing framework
- **Serde**: Optional serialization framework
