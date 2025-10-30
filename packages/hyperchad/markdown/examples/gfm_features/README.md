# GitHub Flavored Markdown Features Example

This example demonstrates the GitHub Flavored Markdown (GFM) extensions supported by `hyperchad_markdown`, including tables, strikethrough, task lists, and emoji support.

## Summary

A comprehensive demonstration of GFM-specific features that extend standard CommonMark markdown, showing how `hyperchad_markdown` handles tables, task lists, strikethrough text, emoji shortcodes, autolinks, and smart punctuation.

## What This Example Demonstrates

- Creating and formatting tables with alignment
- Using strikethrough text with `~~text~~` syntax
- Building interactive task lists with checkboxes
- Converting emoji shortcodes to Unicode emoji (`:rocket:` → 🚀)
- Handling autolinks for URLs and email addresses
- Smart punctuation conversion (ellipses, em-dashes, quotes)
- Combining multiple GFM features in complex documents

## Prerequisites

- Understanding of basic Markdown syntax
- Familiarity with GitHub Flavored Markdown extensions
- Knowledge of HyperChad Container model (helpful but not required)

## Running the Example

Basic usage (without emoji):

```bash
cargo run --manifest-path packages/hyperchad/markdown/examples/gfm_features/Cargo.toml
```

With emoji support enabled:

```bash
cargo run --manifest-path packages/hyperchad/markdown/examples/gfm_features/Cargo.toml --features emoji
```

## Expected Output

The example outputs each GFM feature demonstration:

```
=== HyperChad Markdown - GitHub Flavored Markdown Features ===

Example 1: Tables
-----------------
Markdown:
| Feature | Supported |
|---------|-----------|
| Tables  | ✓         |
| GFM     | ✓         |

Container children count: 1

Example 2: Strikethrough
------------------------
Markdown: This is ~~incorrect~~ correct text.
Container children count: 1

[... additional examples ...]

=== All GFM examples completed successfully! ===
```

## Code Walkthrough

### Tables

GFM tables are created using pipe delimiters and alignment indicators:

```rust
let markdown = r"| Feature | Supported |
|---------|-----------|
| Tables  | ✓         |
| GFM     | ✓         |";
let container = markdown_to_container(markdown);
```

Tables are converted to `Element::Table`, `Element::THead`, `Element::TR`, and `Element::TD` containers with appropriate styling.

### Strikethrough

Strikethrough text uses double tildes:

```rust
let markdown = "This is ~~incorrect~~ correct text.";
let container = markdown_to_container(markdown);
```

Creates a Container with `TextDecoration::LineThrough` style.

### Task Lists

Task lists use checkbox syntax in list items:

```rust
let markdown = r"- [x] Completed task
- [ ] Pending task";
let container = markdown_to_container(markdown);
```

Each checkbox is converted to an `Element::Input::Checkbox` with the checked state preserved.

### Emoji Support

When the `emoji` feature is enabled, emoji shortcodes are converted:

```rust
#[cfg(feature = "emoji")]
{
    let markdown = ":rocket: Launch successful!";
    let container = markdown_to_container(markdown);
    // ":rocket:" becomes "🚀"
}
```

### Complex Tables

Tables support alignment indicators:

```rust
let markdown = r"| Left | Center | Right |
|:-----|:------:|------:|
| L    | C      | R     |";
```

- `:---` = left aligned
- `:---:` = center aligned
- `---:` = right aligned

### Combining Features

GFM features can be combined in a single document:

```rust
let markdown = r"# Project Status

- [x] ~~Phase 1~~ **Done**
- [ ] Phase 2

| Phase | Status |
|-------|--------|
| 1     | ✓      |";
```

## Key Concepts

### GitHub Flavored Markdown Extensions

GFM extends CommonMark with practical features commonly used in GitHub documentation:

- **Tables**: Structured data presentation
- **Strikethrough**: Show edits and corrections
- **Task Lists**: Track completion status
- **Autolinks**: Automatic URL and email linking

### Feature Compilation

The emoji feature is optional and controlled at compile time:

- `default` features: Includes emoji and XSS protection
- `--features emoji`: Explicitly enable emoji support
- Without `emoji` feature: Shortcodes remain as text

### XSS Protection

All examples use default XSS protection, which filters:

- Dangerous HTML tags (`<script>`, `<iframe>`, etc.)
- JavaScript URLs (`javascript:`, `data:`, `vbscript:`)

### Smart Punctuation

When enabled, smart punctuation converts:

- `...` → `…` (ellipsis)
- `--` → `–` (en-dash)
- `---` → `—` (em-dash)
- `"quotes"` → `"smart quotes"`

## Testing the Example

1. Run without emoji and verify task lists render
2. Run with `--features emoji` and check emoji conversion
3. Verify tables have the correct number of rows/columns
4. Check that strikethrough text is properly marked
5. Ensure all examples complete without errors

## Troubleshooting

**Issue**: Emoji shortcodes not converting

- **Solution**: Compile with `--features emoji` to enable emoji support

**Issue**: Tables not rendering correctly

- **Solution**: Ensure proper table syntax with header separator row (`|---|---|`)

**Issue**: Task list checkboxes not appearing

- **Solution**: Verify syntax uses `- [ ]` or `- [x]` with spaces inside brackets

## Related Examples

- `basic_usage` - Introduction to core markdown conversion
- `custom_options` - Learn how to selectively enable/disable GFM features
