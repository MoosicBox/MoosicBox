# HyperChad Transformer

Core UI transformation system with container models, layout calculations, and HTML generation.

## Overview

The HyperChad Transformer package provides:

- **Container System**: Comprehensive UI container model with styling and layout
- **Layout Engine**: Advanced layout calculation with flexbox and grid support
- **HTML Generation**: Complete HTML rendering with CSS generation
- **Calculation System**: CSS calc() expressions with viewport units
- **Element Types**: Full HTML element support with semantic elements
- **Responsive Design**: Conditional styling and responsive breakpoints
- **Tree Traversal**: Efficient container tree navigation and manipulation

## Features

### Container Model

- **Comprehensive Styling**: Complete CSS property support
- **Layout Properties**: Flexbox, grid, positioning, and spacing
- **Typography**: Font families, sizes, colors, and text decoration
- **Visual Effects**: Borders, backgrounds, opacity, and transforms
- **Interactive Elements**: Actions, routes, and event handling
- **State Management**: Component state and data attributes

### Layout System

- **Flexbox Layout**: Complete flexbox implementation
- **Grid Layout**: CSS Grid support with cell sizing
- **Positioning**: Static, relative, absolute, fixed, and sticky
- **Responsive Units**: vw, vh, dvw, dvh, percentages
- **Calculations**: CSS calc() expressions with math operations
- **Viewport Handling**: Dynamic viewport size calculations

### Element Types

- **Semantic HTML**: div, main, header, footer, section, aside
- **Forms**: input, button, textarea, form elements
- **Media**: images with responsive loading and sizing
- **Navigation**: anchors with target and href support
- **Typography**: headings (h1-h6), spans
- **Lists**: ordered and unordered lists with items
- **Tables**: complete table structure (table, thead, tbody, tr, td, th)
- **Interactive**: details and summary elements for collapsible content
- **Canvas**: Optional canvas element support

### Tree Operations

- **BFS Traversal**: Breadth-first search with level tracking
- **Element Finding**: Find by ID, string ID, or CSS class
- **Parent/Child**: Navigate parent-child relationships
- **Element Replacement**: Replace elements by ID or selector
- **Filtering**: Filter by visibility, position, or element type

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_transformer = { path = "../hyperchad/transformer" }

# Enable additional features
hyperchad_transformer = {
    path = "../hyperchad/transformer",
    features = ["html", "layout", "layout-offset"]
}
```

## Usage

### Basic Container Creation

```rust
use hyperchad_transformer::{Container, Element, Number};
use hyperchad_transformer::models::LayoutDirection;
use hyperchad_color::Color;

let mut container = Container {
    element: Element::Div,
    width: Some(Number::from(300)),
    height: Some(Number::from(200)),
    background: Some(Color::from_hex("#f0f0f0")),
    direction: LayoutDirection::Column,
    padding_left: Some(Number::from(20)),
    padding_right: Some(Number::from(20)),
    ..Default::default()
};
```

### HTML Generation

```rust
use hyperchad_transformer::Container;

let container = Container::default();

// Convert to HTML string
let html = container.to_string();

// Pretty printed HTML
let pretty_html = container.display_to_string_default_pretty(true, true)
    .expect("Failed to generate HTML");

// With debug attributes
let debug_html = container.display_to_string_default(true, true)
    .expect("Failed to generate HTML");
```

### Layout Calculations

```rust
use hyperchad_transformer::{Number, Calculation};

// CSS calc() expressions
let width = Number::Calc(Calculation::Subtract(
    Box::new(Calculation::Number(Box::new(Number::RealPercent(100.0)))),
    Box::new(Calculation::Number(Box::new(Number::Real(40.0))))
));

// Viewport units
let height = Number::RealVh(50.0); // 50vh
let font_size = Number::RealDvw(4.0); // 4dvw
```

### Element Types

```rust
use hyperchad_transformer::{Element, Input, HeaderSize};
use hyperchad_transformer::models::{ImageFit, ImageLoading, LinkTarget};

// Form input
let text_input = Element::Input {
    input: Input::Text {
        value: Some("default value".to_string()),
        placeholder: Some("Enter text...".to_string()),
    },
    name: Some("username".to_string()),
    autofocus: None,
};

// Image with responsive loading
let image = Element::Image {
    source: Some("/images/photo.jpg".to_string()),
    alt: Some("Photo description".to_string()),
    fit: Some(ImageFit::Cover),
    loading: Some(ImageLoading::Lazy),
    sizes: Some(Number::from(300)),
    source_set: Some("photo-300.jpg 300w, photo-600.jpg 600w".to_string()),
};

// Heading
let heading = Element::Heading {
    size: HeaderSize::H1,
};

// Link
let link = Element::Anchor {
    href: Some("https://example.com".to_string()),
    target: Some(LinkTarget::Blank),
};
```

### Tree Traversal

```rust
use hyperchad_transformer::Container;

let mut root = Container::default();
// ... populate with children

// Breadth-first traversal
let paths = root.bfs();
paths.traverse(&root, |container| {
    println!("Visiting container: {:?}", container.element);
});

// Find elements
if let Some(element) = root.find_element_by_str_id("my-element") {
    println!("Found element: {:?}", element);
}

// Find by class
if let Some(element) = root.find_element_by_class("button") {
    println!("Found button: {:?}", element);
}
```

### Element Replacement

```rust
use hyperchad_transformer::Container;

let mut root = Container::default();
let new_elements = vec![Container::default()];

// Replace children by ID
if let Some(old_children) = root.replace_str_id_children_with_elements(
    new_elements,
    "container-id"
) {
    println!("Replaced {} children", old_children.len());
}

// Replace element by ID
if let Some(old_element) = root.replace_str_id_with_elements(
    vec![Container::default()],
    "element-id"
) {
    println!("Replaced element: {:?}", old_element.element);
}
```

### Responsive Design

```rust
use hyperchad_transformer::{Container, ConfigOverride, OverrideCondition, OverrideItem, Number};
use hyperchad_transformer::models::LayoutDirection;

let mut container = Container::default();

// Add responsive override
container.overrides.push(ConfigOverride {
    condition: OverrideCondition::ResponsiveTarget {
        name: "mobile".to_string(),
    },
    overrides: vec![
        OverrideItem::Direction(LayoutDirection::Column),
        OverrideItem::Width(Number::RealPercent(100.0)),
    ],
    default: Some(OverrideItem::Direction(LayoutDirection::Row)),
});
```

### Layout Calculations (with `layout` feature)

```rust
#[cfg(feature = "layout")]
use hyperchad_transformer::layout::Calc;

#[cfg(feature = "layout")]
{
    struct MyCalculator;

    impl Calc for MyCalculator {
        fn calc(&self, container: &mut Container) -> bool {
            // Perform layout calculations
            // Return true if layout was modified, false otherwise
            true
        }
    }

    let calculator = MyCalculator;
    let mut container = Container::default();
    container.partial_calc(&calculator, container.id);
}
```

### Table Operations

```rust
use hyperchad_transformer::{Container, Element};

let mut table = Container {
    element: Element::Table,
    ..Default::default()
};

// Iterate table structure
let table_iter = table.table_iter();
if let Some(headings) = table_iter.headings {
    for heading_row in headings {
        for cell in heading_row {
            println!("Header cell: {:?}", cell);
        }
    }
}

for row in table_iter.rows {
    for cell in row {
        println!("Data cell: {:?}", cell);
    }
}
```

### Input Elements

```rust
use hyperchad_transformer::{Element, Input};

// Text input
let text_input = Element::Input {
    input: Input::Text {
        value: None,
        placeholder: Some("Enter your name".to_string()),
    },
    name: Some("name".to_string()),
    autofocus: None,
};

// Checkbox
let checkbox = Element::Input {
    input: Input::Checkbox {
        checked: Some(true),
    },
    name: Some("agree".to_string()),
    autofocus: None,
};

// Password input
let password = Element::Input {
    input: Input::Password {
        value: None,
        placeholder: Some("Password".to_string()),
    },
    name: Some("password".to_string()),
    autofocus: None,
};
```

## Number System

### Basic Numbers

```rust
use hyperchad_transformer::Number;

let pixels = Number::Real(300.0);
let percentage = Number::RealPercent(50.0);
let viewport_width = Number::RealVw(100.0);
let viewport_height = Number::RealVh(50.0);
```

### Calculations

```rust
use hyperchad_transformer::{Number, Calculation};

let calc = Number::Calc(Calculation::Add(
    Box::new(Calculation::Number(Box::new(Number::RealPercent(50.0)))),
    Box::new(Calculation::Number(Box::new(Number::Real(20.0))))
));
// Represents: calc(50% + 20px)
```

## Container Properties

### Layout Properties

- **direction**: LayoutDirection (Row/Column)
- **overflow_x/overflow_y**: LayoutOverflow handling
- **justify_content**: Main axis alignment
- **align_items**: Cross axis alignment
- **flex**: Flexbox grow, shrink, basis

### Spacing Properties

- **width/height**: Element dimensions
- **min_width/max_width**: Size constraints
- **margin\_\***: External spacing
- **padding\_\***: Internal spacing
- **column_gap/row_gap**: Flexbox/grid gaps

### Visual Properties

- **background**: Background color
- **color**: Text color
- **opacity**: Element transparency
- **border\_\***: Border styling
- **border\_\*\_radius**: Corner rounding

### Typography Properties

- **font_family**: Font stack
- **font_size**: Text size
- **text_align**: Text alignment
- **text_decoration**: Text styling

## Feature Flags

- **`html`**: Enable HTML generation and parsing
- **`layout`**: Enable layout calculation system
- **`layout-offset`**: Enable offset calculations (requires `layout`)
- **`canvas`**: Enable canvas element support
- **`logic`**: Enable logic/conditional features
- **`format`**: Enable XML formatting for pretty-printed output
- **`syntax-highlighting`**: Enable syntax highlighting in output
- **`simd`**: Enable SIMD optimizations for HTML parsing

## Dependencies

- **HyperChad Actions**: Interactive action system
- **HyperChad Color**: Color handling and conversion
- **HyperChad Transformer Models**: Layout and styling types
- **Serde**: Serialization and deserialization
- **Strum**: Enum utilities

## Integration

This package is designed for:

- **UI Frameworks**: Core UI component system
- **Layout Engines**: Advanced layout calculation
- **HTML Generation**: Server-side rendering
- **Desktop Applications**: Native UI layout
- **Responsive Design**: Adaptive UI components
