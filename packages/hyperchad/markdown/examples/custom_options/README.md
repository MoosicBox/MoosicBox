# Custom Options Example

This example demonstrates how to use `MarkdownOptions` to customize markdown parsing behavior, selectively enabling or disabling specific features to match different use cases.

## Summary

A comprehensive guide to configuring `hyperchad_markdown` through the `MarkdownOptions` struct, showing how to control GitHub Flavored Markdown features, emoji conversion, XSS protection, and smart punctuation for different scenarios like blogs, user comments, and documentation.

## What This Example Demonstrates

- Using default options with all features enabled
- Selectively disabling individual features (tables, strikethrough, task lists)
- Creating minimal configurations with only basic markdown
- Enabling and disabling XSS protection
- Configuring smart punctuation conversion
- Controlling emoji shortcode conversion
- Creating purpose-specific configurations for different use cases

## Prerequisites

- Understanding of basic markdown syntax
- Familiarity with GitHub Flavored Markdown features
- Knowledge of security considerations for user-generated content

## Running the Example

Basic usage (with default features):

```bash
cargo run --manifest-path packages/hyperchad/markdown/examples/custom_options/Cargo.toml
```

With all features enabled:

```bash
cargo run --manifest-path packages/hyperchad/markdown/examples/custom_options/Cargo.toml --features emoji,xss-protection
```

## Expected Output

The example demonstrates each configuration option:

```
=== HyperChad Markdown - Custom Options Example ===

Example 1: Default Options
--------------------------
Markdown: | Table | Support |
|-------|--------|
| Yes   | ✓      |
With default options, tables are enabled
Container children count: 1

Example 2: Tables Disabled
--------------------------
[... demonstrations of each option ...]

=== All custom options examples completed successfully! ===
```

## Code Walkthrough

### Default Options

The simplest approach uses default options via `markdown_to_container`:

```rust
let container = markdown_to_container(markdown);
```

This is equivalent to:

```rust
let options = MarkdownOptions::default();
let container = markdown_to_container_with_options(markdown, options);
```

### Disabling Specific Features

To disable a specific feature, create custom options:

```rust
let options = MarkdownOptions {
    enable_tables: false,
    ..Default::default()
};
let container = markdown_to_container_with_options(markdown, options);
```

The `..Default::default()` syntax keeps all other options at their defaults.

### Minimal Configuration

For basic markdown only, disable all GFM features:

```rust
let options = MarkdownOptions {
    enable_tables: false,
    enable_strikethrough: false,
    enable_tasklists: false,
    enable_footnotes: false,
    enable_smart_punctuation: false,
    emoji_enabled: false,
    xss_protection: true,  // Keep security enabled
};
```

### XSS Protection

XSS protection is critical for user-generated content:

```rust
// Safe for untrusted content
let safe_options = MarkdownOptions {
    xss_protection: true,
    ..Default::default()
};

// Only for trusted content
let trusted_options = MarkdownOptions {
    xss_protection: false,
    ..Default::default()
};
```

When enabled, XSS protection:

- Escapes dangerous HTML tags (`<script>`, `<iframe>`, etc.)
- Filters JavaScript URLs (`javascript:`, `data:`, `vbscript:`)

### Smart Punctuation

Smart punctuation improves typography:

```rust
let options = MarkdownOptions {
    enable_smart_punctuation: true,
    ..Default::default()
};
```

Converts:

- `...` → `…` (ellipsis)
- `--` → `–` (en-dash)
- `---` → `—` (em-dash)
- `"text"` → `"text"` (smart quotes)

### Emoji Configuration

Emoji conversion requires the `emoji` feature at compile time:

```rust
#[cfg(feature = "emoji")]
let options = MarkdownOptions {
    emoji_enabled: true,
    ..Default::default()
};
```

### Use Case Configurations

Different scenarios need different configurations:

**Blog Posts** (full features + security):

```rust
let blog_options = MarkdownOptions {
    enable_tables: true,
    enable_strikethrough: true,
    enable_tasklists: true,
    enable_footnotes: true,
    enable_smart_punctuation: true,
    emoji_enabled: true,
    xss_protection: true,
};
```

**User Comments** (restricted + maximum security):

```rust
let comment_options = MarkdownOptions {
    enable_tables: false,        // Prevent layout abuse
    enable_strikethrough: true,  // Allow basic formatting
    enable_tasklists: false,     // No interactive elements
    enable_footnotes: false,     // Keep simple
    enable_smart_punctuation: true,
    emoji_enabled: true,         // Allow expression
    xss_protection: true,        // Critical!
};
```

**Documentation** (comprehensive, trusted):

```rust
let doc_options = MarkdownOptions {
    enable_tables: true,
    enable_strikethrough: true,
    enable_tasklists: true,
    enable_footnotes: true,
    enable_smart_punctuation: true,
    emoji_enabled: false,        // Professional tone
    xss_protection: false,       // Trusted source
};
```

## Key Concepts

### Feature Independence

Each option controls a specific feature independently. Disabling one feature doesn't affect others.

### Compile-Time vs Runtime Features

- **Compile-time**: `emoji` and `xss-protection` Cargo features
- **Runtime**: `emoji_enabled` and `xss_protection` options

Both must be enabled for features to work:

```toml
# In Cargo.toml
hyperchad_markdown = { version = "*", features = ["emoji"] }
```

```rust
// In code
let options = MarkdownOptions {
    emoji_enabled: true,  // Runtime enable
    ..Default::default()
};
```

### Security Considerations

**Always enable XSS protection for user-generated content**:

- Comments
- Forum posts
- User profiles
- Any untrusted markdown source

Only disable for trusted content:

- Internal documentation
- Admin-created content
- Pre-validated markdown

### Performance Implications

Disabling features can improve parsing performance for simple documents:

- Fewer parsing rules to check
- Simpler parser state machine
- Faster processing of plain text

For performance-critical applications, disable unused features.

## Testing the Example

1. Run with default features and observe full functionality
2. Run individual examples to see feature effects
3. Compare output with features enabled vs disabled
4. Test XSS protection with dangerous HTML
5. Verify emoji conversion with and without the feature

## Troubleshooting

**Issue**: Emoji not converting even with `emoji_enabled: true`

- **Solution**: Compile with `--features emoji` to enable at compile time

**Issue**: XSS protection not working

- **Solution**: Ensure both compile-time feature and runtime option are enabled

**Issue**: Smart punctuation not converting

- **Solution**: Check that `enable_smart_punctuation: true` in options

**Issue**: Features still working when disabled

- **Solution**: Verify you're using `markdown_to_container_with_options`, not `markdown_to_container`

## Related Examples

- `basic_usage` - Introduction to core markdown conversion
- `gfm_features` - See all GFM features in action with defaults
