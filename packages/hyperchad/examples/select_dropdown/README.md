# HyperChad Select/Dropdown Example

This example demonstrates select/option dropdown elements in
[HyperChad](https://github.com/MoosicBox/MoosicBox/tree/master/packages/hyperchad).

## Features Demonstrated

- Basic select dropdown usage
- Default selected value with `selected` attribute
- Disabled placeholder option pattern
- Dynamic options using `@for` iteration
- Change event handling with `fx-change`
- Styled dropdowns with HyperChad attributes
- Form integration with `name` attribute
- Visual feedback on selection change

## Running the Example

```bash
cd packages/hyperchad/examples/select_dropdown
PORT=3133 cargo run -- serve
```

Then open your browser to: <http://localhost:3133>

## Key Points

- `<select>` creates dropdown selection menus
- `<option>` elements must be direct children of `<select>`
- `selected` attribute on select sets the currently selected value
- `disabled` on option prevents selection (useful for placeholders)
- `fx-change` triggers actions when selection changes
- `value` on option defines the form submission value
- `name` on select identifies the field in form submissions
- Native HTML functionality - works in all browsers

## Example Usage

Basic select:

```rust,ignore
select name="fruit" {
    option value="apple" { "Apple" }
    option value="banana" { "Banana" }
    option value="orange" { "Orange" }
}
```

With default selection:

```rust,ignore
select name="size" selected="medium" {
    option value="small" { "Small" }
    option value="medium" { "Medium" }
    option value="large" { "Large" }
}
```

Placeholder pattern:

```rust,ignore
select name="country" selected="" {
    option value="" disabled { "-- Select a country --" }
    option value="us" { "United States" }
    option value="uk" { "United Kingdom" }
}
```

With change handler:

```rust,ignore
select
    name="animal"
    fx-change=fx { http_get("/api/animal-display", Some("#animal-display")) }
{
    option value="dog" { "Dog" }
    option value="cat" { "Cat" }
}
```

Dynamic options:

```rust,ignore
let colors = vec![("red", "Red"), ("blue", "Blue")];

select name="color" {
    @for (value, label) in &colors {
        option value=(value) { (label) }
    }
}
```
