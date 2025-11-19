# Examples Guidelines for HyperChad

## Package-Specific Context

HyperChad is a **template-based UI framework** for building cross-platform applications. It's an **umbrella package** that re-exports functionality from ~20 child packages. Examples should demonstrate how these packages work together to create real applications.

### What is HyperChad?

- **Multi-renderer architecture**: Write UI once, render to web (HTML/Vanilla JS), desktop (Egui/FLTK), or server-side
- **Template-based**: Uses the `container!` macro for declarative UI composition
- **Component ecosystem**: Router, actions, state, markdown, color management, etc.
- **Type-safe**: Rust's type system prevents UI bugs at compile time

## Example Priorities

### High Priority (Great Candidates for New Examples)

1. **Router + State Example** - Show navigation with persistent state across routes
    - Demonstrates: `hyperchad_router`, `hyperchad_state`, `hyperchad_template`
    - Use case: Multi-page app with shared state (e.g., shopping cart, user preferences)
    - Backend-agnostic: Should work with multiple renderers

2. **State Persistence Example** - SQLite-backed state management
    - Demonstrates: `hyperchad_state` with `persistence-sqlite` feature
    - Use case: Form data persistence, user settings storage
    - Backend-agnostic: State logic is independent of renderer

3. **Actions + Events Example** - Interactive form with validation
    - Demonstrates: `hyperchad_actions`, `hyperchad_router`, form handling
    - Use case: Contact form, login page, data entry
    - Backend-agnostic: Event handling works across renderers

4. **Template Composition Example** - Reusable components and composition patterns
    - Demonstrates: `hyperchad_template`, `container!` macro patterns
    - Use case: Building a component library, layout system
    - Backend-agnostic: Templates are renderer-independent

### Medium Priority

5. **Color Theming Example** - Dynamic theme switching
    - Demonstrates: `hyperchad_color`, state management for themes
    - Backend-agnostic: Color system works across all renderers

6. **Conditional Rendering Example** - Dynamic UI based on state
    - Demonstrates: Control flow in templates, state reactivity
    - Use case: Show/hide elements, conditional layouts

### Already Well-Covered

- ✅ **Markdown rendering** (`examples/markdown/`) - Comprehensive, excellent documentation
- ✅ **Details/Summary component** (`examples/details_summary/`) - Interactive HTML elements
- ✅ **HTTP events** (`examples/http_events/`) - Event-driven updates

## Important Considerations

### Backend-Agnostic First

**CRITICAL**: Examples should be **backend-agnostic** by default. HyperChad's core value proposition is writing UI code once and deploying to multiple backends (web, desktop, server).

Examples should:

- Work with multiple renderers (HTML, Egui, FLTK) via feature flags
- Focus on the `Container` abstraction that's renderer-independent
- Demonstrate portability by showing the same code works everywhere
- Avoid backend-specific APIs or patterns

The `markdown` example is a good model - while it uses Actix and Vanilla JS for the running example, the core markdown-to-Container conversion is backend-agnostic.

### Application Initialization

**ALWAYS use `AppBuilder` and `build_default()`** when creating HyperChad applications:

```rust
// ✅ Correct pattern - build_default() selects renderer based on features
use hyperchad::app::AppBuilder;

let app = AppBuilder::new()
    .with_title("My App".to_string())
    .with_router(router)
    .build_default()?;  // Uses feature flags to pick renderer

app.run()?;
```

```rust
// ❌ Avoid backend-specific builders
let app = AppBuilder::new()
    .with_router(router)
    .build_default_egui()?;  // DON'T - ties code to Egui backend

// ❌ Avoid manual renderer construction
let renderer = EguiRenderer::new(...);
let app = AppBuilder::new()
    .with_router(router)
    .build(renderer)?;  // DON'T - requires backend-specific knowledge
```

The `build_default()` method automatically selects the appropriate renderer based on enabled features (egui, fltk, actix, lambda, etc.). This keeps examples backend-agnostic and demonstrates HyperChad's cross-platform capabilities.

**Feature-based selection**: The same code with `build_default()` will compile to:

- Egui desktop app when `egui` feature is enabled
- FLTK desktop app when `fltk` feature is enabled
- Actix web server when `actix` + `vanilla-js` features are enabled
- Lambda function when `lambda` feature is enabled
- And more based on feature combinations

This is the core value of HyperChad - write once, deploy anywhere.

### Multi-Package Integration

**KEY**: HyperChad examples should show **integration** of child packages, not just single features. The `markdown` example is the gold standard:

- Uses `hyperchad_markdown` for content
- Uses `hyperchad_router` for navigation
- Uses `hyperchad_renderer_html_actix` for serving
- Uses `hyperchad_renderer_vanilla_js` for client-side features
- Uses `hyperchad_template` for UI composition

New examples should follow this pattern of combining 3-5 child packages.

### Template System Focus

All examples must heavily feature the `container!` macro - it's the heart of HyperChad:

```rust
container! {
    div class="card" {
        h1 { "Title" }
        p { "Content" }
        button fx-click=fx { handle_click() } {
            "Click Me"
        }
    }
}
```

### Features and Dependencies

- Examples can use specific backends for demonstration (e.g., Actix for web serving)
- But the core logic should be backend-agnostic
- Keep dependencies minimal - only include what's demonstrated
- Use `workspace = true` for all HyperChad dependencies
- Show how feature flags enable different backends

### Child Packages to Emphasize

These child packages would benefit most from being featured in examples:

1. **hyperchad_router** - Routing is fundamental but under-demonstrated
2. **hyperchad_state** - State management patterns need clear examples
3. **hyperchad_actions** - Event handling beyond simple clicks
4. **hyperchad_template** - Advanced macro patterns and composition
5. **hyperchad_color** - Theming and color management

### Architecture Notes

- HyperChad separates **data** (Container structs) from **rendering** (backends)
- Examples should show this separation clearly
- Demonstrate backend-agnostic code whenever possible
- The Container tree is the same regardless of renderer - emphasize this

## Examples to Avoid

- **Single-function demos** - HyperChad is about integration, not isolated features
- **Pure HTML/CSS examples** - Not demonstrating HyperChad's value proposition
- **Trivial "hello world"** - The existing examples already cover basics
- **Examples that only use one child package** - Show integration instead
- **Backend-specific examples** (e.g., Egui-only, FLTK-only, Actix-only) - HyperChad's value is backend portability; examples should work across renderers
- **Renderer-specific features** - Avoid features that only work with one backend unless absolutely necessary

## Testing Guidance

Examples should:

- Compile with `cargo check` and `cargo clippy -- -D warnings`
- Use standard HyperChad clippy attributes
- Include clear README with running instructions
- Show expected output/screenshots when helpful
- Include troubleshooting for common issues
- Document which renderers the example supports (ideally: all)

## Style Guidelines

- Use `container!` macro consistently
- **Always use `AppBuilder` and `build_default()`** for app initialization
- Prefer declarative patterns over imperative
- Show idiomatic Rust (proper error handling, type inference, etc.)
- Keep examples focused - one main concept per example
- Add inline comments explaining HyperChad-specific patterns
- Emphasize backend-agnostic code structure

## Reference Example

**`examples/markdown/`** is the gold standard - study it for:

- Comprehensive README structure
- Multi-package integration
- Feature flag usage
- **Proper `AppBuilder` usage with `build_default()`**
- Real-world application patterns
- Clear documentation of all features
- Backend-agnostic markdown conversion (Container generation works for any renderer)
