# HyperChad Transformer Models

Core data models and types for HyperChad UI transformations and layout.

## Overview

The HyperChad Transformer Models package provides:

- **Layout Models**: Flexible layout direction and overflow handling
- **Alignment Types**: Content and item alignment options
- **Position Models**: Element positioning and cursor types
- **Route Models**: HTTP routing and HTMX integration
- **Visual Models**: Visibility, image, and text styling options
- **Serialization**: Optional serde support for all models

## Models

### Layout System
- **LayoutDirection**: Row/Column layout directions
- **LayoutOverflow**: Auto, Scroll, Expand, Squash, Wrap, Hidden
- **JustifyContent**: Start, Center, End, SpaceBetween, SpaceEvenly
- **AlignItems**: Start, Center, End alignment
- **LayoutPosition**: Grid positioning with row/column (with `layout` feature)

### Text & Typography
- **TextAlign**: Start, Center, End, Justify alignment
- **TextDecorationLine**: None, Underline, Overline, LineThrough
- **TextDecorationStyle**: Solid, Double, Dotted, Dashed, Wavy

### Visual Properties
- **Visibility**: Visible/Hidden states
- **Position**: Static, Relative, Absolute, Sticky, Fixed
- **Cursor**: Comprehensive cursor type definitions
- **ImageLoading**: Eager/Lazy loading strategies
- **ImageFit**: Default, Contain, Cover, Fill, None

### Routing & HTMX
- **Route**: GET, POST, PUT, DELETE, PATCH HTTP methods
- **SwapTarget**: This, Children, Id targeting for HTMX swaps
- **LinkTarget**: SelfTarget, Blank, Parent, Top, Custom

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_transformer_models = { path = "../hyperchad/transformer/models" }

# Enable additional features
hyperchad_transformer_models = {
    path = "../hyperchad/transformer/models",
    features = ["serde", "layout", "arb"]
}
```

## Usage

### Layout Configuration

```rust
use hyperchad_transformer_models::{
    LayoutDirection, LayoutOverflow, JustifyContent, AlignItems
};

// Configure layout
let direction = LayoutDirection::Row;
let overflow = LayoutOverflow::Wrap { grid: true };
let justify = JustifyContent::SpaceBetween;
let align = AlignItems::Center;

println!("Layout: {} {} {} {}", direction, overflow, justify, align);
// Output: "Layout: row wrap-grid space-between center"
```

### Text Styling

```rust
use hyperchad_transformer_models::{
    TextAlign, TextDecorationLine, TextDecorationStyle
};

// Text configuration
let align = TextAlign::Center;
let decoration = TextDecorationLine::Underline;
let style = TextDecorationStyle::Dashed;

println!("Text: {} {} {}", align, decoration, style);
// Output: "Text: center underline dashed"
```

### Visual Properties

```rust
use hyperchad_transformer_models::{Visibility, Position, Cursor};

// Visual configuration
let visibility = Visibility::Visible;
let position = Position::Relative;
let cursor = Cursor::Pointer;

println!("Visual: {} {} {}", visibility, position, cursor);
// Output: "Visual: visible relative pointer"
```

### Image Configuration

```rust
use hyperchad_transformer_models::{ImageLoading, ImageFit};

// Image settings
let loading = ImageLoading::Lazy;
let fit = ImageFit::Cover;

println!("Image: {} {}", loading, fit);
// Output: "Image: lazy cover"
```

### HTMX Routing

```rust
use hyperchad_transformer_models::{Route, SwapTarget};

// Define routes
let get_route = Route::Get {
    route: "/api/data".to_string(),
    trigger: Some("click".to_string()),
    swap: SwapTarget::This,
};

let post_route = Route::Post {
    route: "/api/submit".to_string(),
    trigger: Some("submit".to_string()),
    swap: SwapTarget::Id("result".to_string()),
};
```

### Link Targets

```rust
use hyperchad_transformer_models::LinkTarget;

// Link target configurations
let self_target = LinkTarget::SelfTarget;
let blank_target = LinkTarget::Blank;
let custom_target = LinkTarget::Custom("custom-frame".to_string());

println!("Targets: {} {} {}", self_target, blank_target, custom_target);
// Output: "Targets: _self _blank custom-frame"
```

### Layout Positioning (with `layout` feature)

```rust
#[cfg(feature = "layout")]
use hyperchad_transformer_models::LayoutPosition;

#[cfg(feature = "layout")]
{
    // Grid positioning
    let position = LayoutPosition::Wrap { row: 2, col: 3 };

    println!("Row: {:?}", position.row()); // Some(2)
    println!("Column: {:?}", position.column()); // Some(3)
}
```

## Model Categories

### Layout Models
- **LayoutDirection**: Flexbox direction (row/column)
- **LayoutOverflow**: Content overflow behavior
- **JustifyContent**: Main axis alignment
- **AlignItems**: Cross axis alignment
- **LayoutPosition**: Grid positioning (feature-gated)

### Typography Models
- **TextAlign**: Text alignment options
- **TextDecorationLine**: Text decoration types
- **TextDecorationStyle**: Decoration styling

### Visual Models
- **Visibility**: Element visibility states
- **Position**: CSS positioning types
- **Cursor**: Mouse cursor appearances
- **ImageLoading**: Image loading strategies
- **ImageFit**: Image fitting modes

### Interaction Models
- **Route**: HTTP routing with HTMX support
- **SwapTarget**: HTMX content swap targets
- **LinkTarget**: Link navigation targets

## String Conversion

All models implement `Display` for CSS-compatible string output:

```rust
use hyperchad_transformer_models::*;

// All models convert to CSS-compatible strings
assert_eq!(LayoutDirection::Row.to_string(), "row");
assert_eq!(JustifyContent::SpaceBetween.to_string(), "space-between");
assert_eq!(TextAlign::Center.to_string(), "center");
assert_eq!(Cursor::Pointer.to_string(), "pointer");
```

## Feature Flags

- **`serde`**: Enable serialization/deserialization
- **`layout`**: Enable layout positioning models
- **`arb`**: Enable arbitrary data generation for testing

## Dependencies

- **Serde**: Optional serialization support

## Integration

This package is designed for:
- **UI Frameworks**: Core UI component modeling
- **CSS Generation**: CSS class and style generation
- **HTMX Integration**: Server-side rendered applications
- **Layout Systems**: Flexbox and grid layout systems
- **Component Libraries**: Reusable UI component definitions
