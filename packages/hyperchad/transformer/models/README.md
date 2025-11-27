# HyperChad Transformer Models

Core data models and types for HyperChad UI transformations and layout.

## Overview

The HyperChad Transformer Models package provides:

- **Layout Models**: Flexible layout direction and overflow handling
- **Alignment Types**: Content and item alignment options
- **Position Models**: Element positioning and cursor types
- **Route Models**: HTTP routing and dynamic content swapping
- **Visual Models**: Visibility, image, and text styling options
- **Typography Models**: Font weight and text decoration options
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
- **TextDecorationLine**: Inherit, None, Underline, Overline, LineThrough
- **TextDecorationStyle**: Inherit, Solid, Double, Dotted, Dashed, Wavy
- **FontWeight**: Thin, ExtraLight, Light, Normal, Medium, SemiBold, Bold, ExtraBold, Black, Lighter, Bolder, and numeric weights (100-900)
- **WhiteSpace**: Normal, Preserve, PreserveWrap
- **UserSelect**: Auto, None, Text, All
- **OverflowWrap**: Normal, BreakWord, Anywhere
- **TextOverflow**: Clip, Ellipsis

### Visual Properties

- **Visibility**: Visible/Hidden states
- **Position**: Static, Relative, Absolute, Sticky, Fixed
- **Cursor**: Comprehensive cursor type definitions
- **ImageLoading**: Eager/Lazy loading strategies
- **ImageFit**: Default, Contain, Cover, Fill, None

### Routing & Swapping

- **Route**: GET, POST, PUT, DELETE, PATCH HTTP methods
- **Selector**: Id, Class, ChildClass, SelfTarget element targeting
- **SwapStrategy**: This, Children, BeforeBegin, AfterBegin, BeforeEnd, AfterEnd, Delete, None swap strategies
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
    TextAlign, TextDecorationLine, TextDecorationStyle, FontWeight
};

// Text configuration
let align = TextAlign::Center;
let decoration = TextDecorationLine::Underline;
let style = TextDecorationStyle::Dashed;
let weight = FontWeight::Bold;

println!("Text: {} {} {} {}", align, decoration, style, weight);
// Output: "Text: center underline dashed bold"
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

### Dynamic Routing

```rust
use hyperchad_transformer_models::{Route, Selector, SwapStrategy};

// Define routes
let get_route = Route::Get {
    route: "/api/data".to_string(),
    trigger: Some("click".to_string()),
    target: Selector::SelfTarget,
    strategy: SwapStrategy::This,
};

let post_route = Route::Post {
    route: "/api/submit".to_string(),
    trigger: Some("submit".to_string()),
    target: Selector::Id("result".to_string()),
    strategy: SwapStrategy::Children,
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

### Font Weights

```rust
use hyperchad_transformer_models::FontWeight;

// Semantic font weights
let normal = FontWeight::Normal;
let bold = FontWeight::Bold;

// Numeric font weights
let weight_400 = FontWeight::Weight400;
let weight_700 = FontWeight::Weight700;

println!("Semantic: {} {}", normal, bold);
// Output: "Semantic: normal bold"

println!("Numeric: {} {}", weight_400, weight_700);
// Output: "Numeric: 400 700"
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
- **FontWeight**: Font weight variants (semantic and numeric)
- **WhiteSpace**: Whitespace handling
- **UserSelect**: Text selection behavior
- **OverflowWrap**: Word wrapping behavior
- **TextOverflow**: Text overflow handling

### Visual Models

- **Visibility**: Element visibility states
- **Position**: Element positioning types
- **Cursor**: Mouse cursor appearances
- **ImageLoading**: Image loading strategies
- **ImageFit**: Image fitting modes

### Interaction Models

- **Route**: HTTP routing with dynamic content swapping
- **Selector**: Element selector targeting
- **SwapStrategy**: Content swap strategies
- **LinkTarget**: Link navigation targets

## String Conversion

All models implement `Display` for string output compatible with various backends:

```rust
use hyperchad_transformer_models::*;

// All models convert to backend-compatible strings
assert_eq!(LayoutDirection::Row.to_string(), "row");
assert_eq!(LayoutDirection::Column.to_string(), "col");
assert_eq!(JustifyContent::SpaceBetween.to_string(), "space-between");
assert_eq!(TextAlign::Center.to_string(), "center");
assert_eq!(Cursor::Pointer.to_string(), "pointer");
assert_eq!(FontWeight::Bold.to_string(), "bold");
assert_eq!(FontWeight::Weight700.to_string(), "700");
assert_eq!(WhiteSpace::Preserve.to_string(), "preserve");
assert_eq!(UserSelect::None.to_string(), "none");
assert_eq!(OverflowWrap::BreakWord.to_string(), "break-word");
assert_eq!(TextOverflow::Ellipsis.to_string(), "ellipsis");
```

## Feature Flags

- **`serde`**: Enable serialization/deserialization
- **`layout`**: Enable layout positioning models
- **`arb`**: Enable arbitrary data generation for testing

## Dependencies

Core dependencies:

- **log**: Logging support
- **moosicbox_assert**: Assertion utilities
- **thiserror**: Error type definitions

Optional dependencies:

- **serde**: Serialization/deserialization support (with `serde` feature)
- **moosicbox_arb**: Arbitrary data generation (with `arb` feature)
- **proptest**: Property-based testing support (with `arb` feature)

## Integration

This package is designed for:

- **UI Frameworks**: Core UI component modeling
- **Style Generation**: Class and style generation for various backends
- **Dynamic Content**: Server-side rendered applications
- **Layout Systems**: Flexbox and grid layout systems
- **Component Libraries**: Reusable UI component definitions
