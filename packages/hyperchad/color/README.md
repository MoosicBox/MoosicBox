# HyperChad Color

Color parsing, manipulation, and conversion utilities for HyperChad applications.

## Overview

The HyperChad Color package provides:

- **Color Parsing**: Hex string to color conversion
- **Color Constants**: Predefined color constants (BLACK, WHITE)
- **Format Support**: RGB and RGBA color support
- **Display Formatting**: Color to hex string conversion
- **Framework Integration**: Optional egui integration
- **Error Handling**: Comprehensive hex parsing error handling

## Features

### Color Parsing

- **Hex Strings**: Parse 3, 4, 6, and 8 character hex strings
- **Flexible Input**: Support for #-prefixed and plain hex strings
- **Whitespace Handling**: Automatic whitespace trimming
- **Case Insensitive**: Support for both uppercase and lowercase hex

### Color Formats

- **RGB**: Standard red, green, blue color values
- **RGBA**: RGB with alpha transparency channel
- **Short Form**: 3-character hex (#RGB) expansion
- **Long Form**: 6-character hex (#RRGGBB) support

### Error Handling

- **ParseHexError**: Comprehensive error types for parsing failures
- **Invalid Characters**: Detailed error reporting for invalid hex characters
- **Length Validation**: Proper validation of hex string lengths
- **Non-ASCII Detection**: Detection and reporting of non-ASCII characters

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_color = { path = "../hyperchad/color" }

# Enable additional features
hyperchad_color = {
    path = "../hyperchad/color",
    features = ["serde", "egui", "arb"]
}
```

## Usage

### Basic Color Creation

```rust
use hyperchad_color::Color;

// Create colors from hex strings (runtime parsing)
let red = Color::from_hex("#FF0000");
let green = Color::from_hex("#00FF00");
let blue = Color::from_hex("#0000FF");

// Short form hex (automatically expanded)
let white = Color::from_hex("#FFF");  // Expands to #FFFFFF
let black = Color::from_hex("#000");  // Expands to #000000

// With alpha channel
let transparent_red = Color::from_hex("#FF000080");
let transparent_blue = Color::from_hex("#00F8");  // Short form with alpha
```

### Compile-Time Color Parsing

```rust
use hyperchad_color::color_from_hex;

// Parse hex colors at compile time (errors at compile time if invalid)
let color = color_from_hex!("#FF5733");
```

### Error Handling

```rust
use hyperchad_color::{Color, ParseHexError};

// Safe parsing with error handling
match Color::try_from_hex("#INVALID") {
    Ok(color) => println!("Parsed color: {}", color),
    Err(ParseHexError::InvalidCharacter(index, char)) => {
        println!("Invalid character '{}' at position {}", char, index);
    }
    Err(ParseHexError::InvalidLength) => {
        println!("Invalid hex string length");
    }
    Err(e) => println!("Parse error: {}", e),
}
```

### Color Constants

```rust
use hyperchad_color::Color;

// Use predefined constants
let black = Color::BLACK;
let white = Color::WHITE;

// Constants are equivalent to hex parsing
assert_eq!(Color::BLACK, Color::from_hex("#000000"));
assert_eq!(Color::WHITE, Color::from_hex("#FFFFFF"));
```

### Color Display

```rust
use hyperchad_color::Color;

// Colors display as hex strings
let color = Color::from_hex("#FF5733");
println!("{}", color); // Output: "#FF5733"

// RGBA colors include alpha
let transparent = Color::from_hex("#FF573380");
println!("{}", transparent); // Output: "#FF573380"
```

### String Conversion

```rust
use hyperchad_color::Color;

// From &str
let color1: Color = "#FF0000".into();

// From String
let hex_string = "#00FF00".to_string();
let color2: Color = hex_string.into();

// From &String
let color3: Color = (&hex_string).into();
```

### Color Properties

```rust
use hyperchad_color::Color;

let color = Color::from_hex("#FF573380");

// Access color components
println!("Red: {}", color.r);     // 255
println!("Green: {}", color.g);   // 87
println!("Blue: {}", color.b);    // 51
println!("Alpha: {:?}", color.a); // Some(128)

// Check for alpha channel
if color.a.is_some() {
    println!("Color has transparency");
}
```

### egui Integration (with `egui` feature)

```rust
use hyperchad_color::Color;
use egui::Color32;

let color = Color::from_hex("#FF5733");

// Convert to egui Color32
let egui_color: Color32 = color.into();
let egui_color_ref: Color32 = (&color).into();

// Use in egui applications
ui.colored_label(egui_color, "Colored text");
```

## Color Structure

```rust
pub struct Color {
    pub r: u8,      // Red component (0-255)
    pub g: u8,      // Green component (0-255)
    pub b: u8,      // Blue component (0-255)
    pub a: Option<u8>, // Optional alpha component (0-255)
}
```

## Supported Hex Formats

- **3-character**: `#RGB` (e.g., `#F00` → `#FF0000`)
- **4-character**: `#RGBA` (e.g., `#F008` → `#FF000088`)
- **6-character**: `#RRGGBB` (e.g., `#FF0000`)
- **8-character**: `#RRGGBBAA` (e.g., `#FF000080`)

All formats support:

- Optional `#` prefix
- Uppercase and lowercase hex digits
- Trailing whitespace (automatically trimmed)

## Error Types

```rust
pub enum ParseHexError {
    InvalidCharacter(usize, char),    // Invalid hex character
    InvalidNonAsciiCharacter(usize),  // Non-ASCII character
    StringTooLong,                    // Hex string too long
    InvalidLength,                    // Invalid hex string length
}
```

## Feature Flags

- **`serde`**: Enable serialization/deserialization
- **`egui`**: Enable egui Color32 conversion
- **`arb`**: Enable arbitrary data generation for testing

## Dependencies

### Core Dependencies

- **color-hex**: Core hex parsing functionality
- **thiserror**: Error handling and display
- **moosicbox_assert**: Assertion utilities
- **log**: Logging functionality

### Optional Dependencies

- **serde**: Serialization/deserialization support (feature: `serde`)
- **egui**: egui Color32 conversion support (feature: `egui`)
- **proptest**: Arbitrary data generation for testing (feature: `arb`)

## Integration

This package is designed for:

- **UI Frameworks**: Color management in UI applications
- **CSS Generation**: Converting colors to CSS-compatible formats
- **Theme Systems**: Dynamic color scheme management
- **Graphics Applications**: Color manipulation and conversion
- **Design Tools**: Color parsing and validation utilities
