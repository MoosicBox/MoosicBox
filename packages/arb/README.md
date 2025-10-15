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

- **CssIdentifierString**: Generate valid CSS identifier strings
- **Testing Support**: CSS identifier generation for property-based tests

### Serde Support

- **JsonValue**: Arbitrary JSON value generation (currently strings)
- **JsonF64**: Finite f64 generation for JSON compatibility
- **JsonF32**: Finite f32 generation for JSON compatibility
- **Testing Support**: Property-based testing for JSON serialization

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
use moosicbox_arb::xml::{XmlString, XmlAttrNameString, is_valid_xml_char};
use quickcheck::{quickcheck, TestResult};

quickcheck! {
    fn valid_xml_strings(xml_str: XmlString) -> TestResult {
        // All characters in XmlString are valid XML characters
        TestResult::from_bool(xml_str.0.chars().all(is_valid_xml_char))
    }

    fn valid_xml_attributes(attr: XmlAttrNameString) -> TestResult {
        // XML attribute names are alphanumeric with dashes/underscores
        TestResult::from_bool(!attr.0.is_empty())
    }
}
```

### CSS Identifier Generation

```rust
use moosicbox_arb::css::CssIdentifierString;
use quickcheck::{quickcheck, TestResult};

quickcheck! {
    fn valid_css_identifiers(css_id: CssIdentifierString) -> TestResult {
        // CSS identifiers are non-empty alphanumeric strings
        TestResult::from_bool(!css_id.0.is_empty())
    }
}
```

### JSON Value Generation

```rust
use moosicbox_arb::serde::{JsonValue, JsonF64, JsonF32};
use quickcheck::quickcheck;

quickcheck! {
    fn json_f64_is_finite(num: JsonF64) -> bool {
        num.0.is_finite()
    }

    fn json_f32_is_finite(num: JsonF32) -> bool {
        num.0.is_finite()
    }
}
```

## Feature Flags

- **`xml`**: Enable XML string generation utilities (XmlString, XmlAttrNameString)
- **`css`**: Enable CSS identifier generation utilities (CssIdentifierString)
- **`serde`**: Enable JSON value generation utilities (JsonValue, JsonF64, JsonF32). Requires `xml` feature.
- **`default`**: Enables all features: `["css", "serde", "xml"]`

## Dependencies

- **quickcheck**: Property-based testing framework (required)
- **log**: Logging framework (required)
- **serde_json**: JSON serialization support (optional, enabled with `serde` feature)
- **moosicbox_assert**: Internal assertion utilities (required)
