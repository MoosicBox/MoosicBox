# HyperChad Details/Summary Example

This example demonstrates collapsible content using HTML `<details>` and `<summary>` elements in HyperChad.

## Features Demonstrated

✅ Basic details/summary usage
✅ Default open state with `open`
✅ FAQ accordion pattern
✅ Nested details elements
✅ Styled details with HyperChad attributes
✅ Details without summary (browser default triangle)
✅ Practical use cases (settings panels, debug info)

## Running the Example

### Development (with embedded assets):

```bash
cd packages/hyperchad/examples/details_summary
PORT=3132 cargo run -- serve
```

Then open your browser to: **http://localhost:3132**

> **Note**: The default port is 8080. Set the `PORT` environment variable to use a different port.

### Production (expects external JS hosting):

```bash
PORT=3132 cargo run --no-default-features --features actix,vanilla-js -- serve
```

## Features

- `dev` - Enables embedded assets for local development (includes the bundled JavaScript)
- `assets` - Enables static asset serving
- `vanilla-js` - Enables vanilla JavaScript renderer
- `actix` - Enables Actix web server backend

## Use Cases Shown

### 1. Basic Collapsible

Simple expand/collapse functionality showing hidden content.

### 2. Default Open

Pre-expanded sections using `open` attribute.

### 3. FAQ Accordion

Multiple independent collapsible sections perfect for frequently asked questions.

### 4. Nested Sections

Details elements within other details elements, each independently collapsible.

### 5. Styled Details

Custom appearance using HyperChad's styling attributes (padding, background, colors, etc.).

### 6. Debug Panel

Developer information toggle with monospace font and custom styling.

### 7. No Summary

Details without a summary element uses the browser's default disclosure triangle.

## HTML Details/Summary

The `<details>` element creates a disclosure widget where content can be shown or hidden.
The `<summary>` element (optional) provides the clickable heading.

### Key Points

- ✅ No JavaScript required - native HTML functionality
- ✅ `<summary>` must be first child of `<details>` if present
- ✅ Only one `<summary>` allowed per `<details>`
- ✅ Can be styled with standard CSS/HyperChad attributes
- ✅ Fully accessible by default
- ✅ Works in all modern browsers

## Code Highlights

### Basic Usage

```rust
details {
    summary { "Click me" }
    div { "Hidden content" }
}
```

### Default Open

```rust
details open {
    summary { "Already expanded" }
    div { "Visible content" }
}
```

### Styled with HyperChad Attributes

```rust
details
    padding=16
    background=#f9fafb
    border-radius=8
{
    summary
        font-weight=bold
        cursor=pointer
    {
        "⚙️ Settings"
    }
    div { "Options here..." }
}
```

### Nested Details

```rust
details {
    summary { "Parent" }
    div {
        "Parent content"

        details {
            summary { "Nested Child" }
            div { "Nested content" }
        }
    }
}
```

### FAQ Pattern

```rust
section {
    h2 { "FAQ" }

    details {
        summary { "Question 1?" }
        div { "Answer 1" }
    }

    details {
        summary { "Question 2?" }
        div { "Answer 2" }
    }
}
```

## Clean Attribute Syntax

This example demonstrates HyperChad's clean, unquoted syntax:

- Unquoted colors: `background=#3b82f6`
- Unquoted numbers: `padding=24`, `gap=12`
- Unquoted keywords: `cursor=pointer`, `font-weight=bold`
- Gap on containers: `gap=12` instead of individual margins
- Flexbox: `direction=row`, `justify-content=center`

## Architecture

This example uses:

- **HyperChad router** for page routing
- **Actix web server** backend (via hyperchad renderer)
- **Vanilla JS renderer** for client-side interactivity
- **Native HTML `<details>`** - no custom JavaScript needed
- **HyperChad template syntax** for declarative UI

## Validation

HyperChad enforces compile-time validation for details/summary:

- ❌ `<summary>` outside `<details>` - **compile error**
- ❌ `<summary>` not as first child - **compile error**
- ❌ Multiple `<summary>` in one `<details>` - **compile error**
- ✅ Details without summary - **allowed**
- ✅ Nested details - **allowed**

These validations happen at compile time, ensuring your HTML structure is always correct!

## Browser Compatibility

The `<details>` and `<summary>` elements are supported in all modern browsers:

- Chrome/Edge: ✅
- Firefox: ✅
- Safari: ✅
- Opera: ✅

No polyfills or JavaScript required!
