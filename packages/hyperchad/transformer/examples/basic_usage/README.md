# Basic Usage Example

Demonstrates core features of the HyperChad Transformer package including container creation, HTML generation, and tree operations.

## Summary

This example shows how to create styled containers, build component hierarchies, generate HTML output, traverse container trees, and work with different element types using the HyperChad Transformer API.

## What This Example Demonstrates

- Creating containers with styling properties (dimensions, colors, padding)
- Building hierarchical component structures (header, main content, footer)
- Generating HTML output from container trees
- Traversing the container tree using breadth-first search
- Using various element types (div, header, main, footer, button, image, anchor)
- Working with the number system (pixels, percentages, viewport units)
- Applying layout directions and gaps (flexbox-style)

## Prerequisites

- Basic understanding of UI components and styling concepts
- Familiarity with HTML elements and CSS properties
- Rust programming knowledge

## Running the Example

Execute the following command from the repository root:

```bash
cargo run --manifest-path packages/hyperchad/transformer/examples/basic_usage/Cargo.toml
```

Or from within the example directory:

```bash
cd packages/hyperchad/transformer/examples/basic_usage
cargo run
```

## Expected Output

The example will output:

1. **Step-by-step progress messages** showing each demonstration phase
2. **Generated HTML output** - A complete HTML representation of the page structure with inline CSS styling
3. **Tree traversal results** - List of elements found during breadth-first search traversal
4. **Element type examples** - Confirmation of button, image, and anchor element creation
5. **Number system examples** - Different unit types (px, %, vw, vh)
6. **Summary checklist** - Confirmation of all demonstrated features

Example console output:

```
=== HyperChad Transformer Basic Usage Example ===

1. Creating a simple container with styling...
   Container created with 300x200px dimensions and #f0f0f0 background

2. Building a component hierarchy (header, content, footer)...
   Built page with header, main content, and footer sections

3. Generating HTML output...
   Generated HTML (1234 bytes):

--- HTML Output ---
<div style="width: 800px; flex-direction: column; row-gap: 20px;">
  <header style="background: #2c3e50; color: #ecf0f1; padding: 20px 30px;">
    <h1 style="font-size: 32px;">Welcome to HyperChad</h1>
  </header>
  ...
</div>
--- End HTML Output ---

4. Demonstrating tree traversal...
   - Found Div element
   - Found Header element
   - Found Heading element
   ...
   Total containers traversed: 8

5. Creating different element types...
   - Created Button element with styling
   - Created Image element (400x300px)
   - Created Anchor (link) element

6. Number system examples...
   - Pixels: 100px
   - Percentage: 50%
   - Viewport width: 80vw
   - Viewport height: 60vh

=== Example Complete ===
```

## Code Walkthrough

### Step 1: Creating a Simple Container

```rust
let simple_container = Container {
    element: Element::Div,
    width: Some(Number::from(300)),
    height: Some(Number::from(200)),
    background: Some(Color::from_hex("#f0f0f0")),
    padding_left: Some(Number::from(20)),
    // ... more properties
    ..Default::default()
};
```

Creates a basic div container with dimensions, background color, and padding. The `Container` struct holds all styling and layout properties.

### Step 2: Building Component Hierarchy

```rust
let mut page = Container {
    element: Element::Div,
    direction: LayoutDirection::Column,
    width: Some(Number::from(800)),
    row_gap: Some(Number::from(20)),
    ..Default::default()
};

// Create child containers
let header = Container { element: Element::Header, /* ... */ };
let main_section = Container { element: Element::Main, /* ... */ };
let footer = Container { element: Element::Footer, /* ... */ };

// Build hierarchy
page.children.push(header);
page.children.push(main_section);
page.children.push(footer);
```

Demonstrates creating a page layout with semantic HTML elements arranged in a column with gaps between sections. The `children` vector builds the tree structure.

### Step 3: HTML Generation

```rust
let html = page
    .display_to_string_default_pretty(false, true)
    .expect("Failed to generate HTML");
```

Converts the container tree into pretty-printed HTML with inline CSS styling. The first parameter controls debug output, the second enables pretty printing.

### Step 4: Tree Traversal

```rust
let paths = page.bfs();
paths.traverse(&page, |container| {
    println!("Found {:?} element", container.element);
});
```

Uses breadth-first search to traverse the entire container tree, visiting each element in level order. Useful for searching, analyzing, or modifying the tree structure.

### Step 5: Different Element Types

The example creates various element types to demonstrate the API:

- **Button**: Interactive element with styling and text
- **Image**: Media element with source, alt text, and dimensions
- **Anchor**: Link element with href and target attributes

Each element type has specific properties while sharing common container properties.

### Step 6: Number System

```rust
let pixels = Number::from(100);          // 100px
let percentage = Number::RealPercent(50.0);  // 50%
let viewport_width = Number::RealVw(80.0);   // 80vw
let viewport_height = Number::RealVh(60.0);  // 60vh
```

Demonstrates the flexible number system supporting multiple CSS units including absolute pixels, percentages, and viewport-relative units.

## Key Concepts

### Container Model

The `Container` struct is the core building block, representing any UI element with:

- **Element type**: What HTML element to render (div, button, header, etc.)
- **Styling properties**: Colors, dimensions, spacing, typography
- **Layout properties**: Direction (row/column), alignment, gaps
- **Hierarchy**: Parent-child relationships via the `children` vector
- **Content**: Text content, images, or nested containers

### HTML Generation

The `html` feature enables converting containers to HTML strings with inline CSS. The transformation:

1. Traverses the container tree
2. Generates appropriate HTML tags based on element types
3. Converts styling properties to inline CSS
4. Handles special elements (images, forms, links) with their attributes
5. Produces valid, styled HTML output

### Tree Operations

HyperChad provides efficient tree traversal and manipulation:

- **BFS traversal**: Visit all containers level-by-level
- **Element finding**: Locate containers by ID or class
- **Element replacement**: Swap containers by ID or selector
- **Parent navigation**: Access parent containers

These operations enable dynamic UI updates and complex transformations.

### Responsive Number System

The number system supports:

- **Fixed units**: Pixels for absolute sizing
- **Relative units**: Percentages based on parent container
- **Viewport units**: vw, vh for screen-relative sizing
- **Dynamic viewport units**: dvw, dvh for mobile viewport handling
- **Calculations**: CSS calc() expressions with arithmetic operations

## Testing the Example

After running the example:

1. **Verify HTML output**: Check that the generated HTML is valid and properly structured
2. **Inspect element hierarchy**: Confirm all elements (header, main, footer) are present
3. **Check styling**: Verify colors, dimensions, and padding are correctly applied in the output
4. **Count containers**: The tree traversal should find 8 total containers (page, header, title, main, 2 text spans, footer, and implicit elements)
5. **Review element types**: Confirm button, image, and anchor elements were created with appropriate properties

You can copy the HTML output to an HTML file and open it in a browser to see the visual result.

## Troubleshooting

### Issue: "html feature not enabled" message

**Solution**: Ensure you're running with the html feature enabled (it's included by default in the example's Cargo.toml). If you see this message, the example will still run but use the Display trait instead of generating full HTML.

### Issue: Container tree traversal shows unexpected count

**Solution**: The count includes all containers in the hierarchy. If you modify the example and add/remove children, the count will change accordingly. This is expected behavior.

### Issue: Colors not appearing in terminal output

**Solution**: Color hex codes appear in the HTML output, not as colored terminal text. To see the visual result, save the HTML output to a file and open it in a web browser.

## Related Examples

This is currently the only example for `hyperchad_transformer`. Future examples may demonstrate:

- Advanced layout calculations with the `layout` feature
- Responsive design with override conditions
- Complex form elements and input handling
- CSS calc() expressions and dynamic sizing
- Table structures with the table iterator API
