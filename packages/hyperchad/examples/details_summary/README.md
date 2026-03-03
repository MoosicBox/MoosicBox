# HyperChad Details/Summary Example

Example demonstrating collapsible content using HTML `<details>` and `<summary>` elements
in [HyperChad](https://github.com/MoosicBox/MoosicBox/tree/master/packages/hyperchad).

## Features Demonstrated

- Basic details/summary usage
- Default open state with `open` attribute
- FAQ accordion pattern with multiple collapsible sections
- Nested details elements
- Styled details with HyperChad attributes
- Details without summary (browser default triangle)
- Practical use cases: settings panels and debug information

## Running the Example

```bash
cd packages/hyperchad/examples/details_summary
PORT=3132 cargo run -- serve
```

Then open your browser to: http://localhost:3132

The server defaults to port 8080 if `PORT` is not set.

## Example Usage

Basic collapsible section:

```rust
details {
    summary { "Click me" }
    div { "Hidden content" }
}
```

Default open state:

```rust
details open {
    summary { "Already expanded" }
    div { "Visible content" }
}
```

Nested details:

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

## Key Points

- No JavaScript required - native HTML functionality
- `<summary>` must be first child of `<details>` if present
- Only one `<summary>` allowed per `<details>`
- Can be styled with standard CSS/HyperChad attributes
- Fully accessible by default
- Works in all modern browsers

## License

See the [LICENSE](../../../../LICENSE) file for details.
