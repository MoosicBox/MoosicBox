---
# Partial: Property-Based Testing
# Expected variables: none required
---

### Property-Based Testing

Property-based testing automatically generates test inputs to verify that code satisfies certain properties across a wide range of cases. Use `proptest` for property-based tests.

**When to use property-based testing:**

- **Serialization roundtrips** - Verify that serialize → deserialize returns the original value
- **Parser correctness** - Verify that parse → format → parse returns equivalent results
- **Invariants** - Properties that should always hold (e.g., `len(a) + len(b) == len(a + b)`)
- **Idempotent operations** - Operations that produce the same result when applied multiple times
- **Commutative/associative operations** - Mathematical properties of functions
- **Edge case discovery** - Finding inputs that break assumptions

**Basic usage with `proptest!`:**

```rust
#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn serialization_roundtrip(value: MyType) {
            let serialized = serde_json::to_string(&value).unwrap();
            let deserialized: MyType = serde_json::from_str(&serialized).unwrap();
            prop_assert_eq!(deserialized, value);
        }
    }
}
```

**Deriving `Arbitrary` for types:**

Use `test_strategy::Arbitrary` to automatically derive proptest strategies:

```rust
#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub enum MyEnum {
    VariantA,
    VariantB,
    VariantC(u32),
}

#[cfg_attr(feature = "arb", derive(test_strategy::Arbitrary))]
pub struct MyStruct {
    pub field_a: String,
    pub field_b: Option<u32>,
}
```

**Custom strategies:**

For types that need constrained generation:

```rust
use proptest::prelude::*;

fn my_custom_strategy() -> impl Strategy<Value = MyType> {
    prop_oneof![
        Just(MyType::VariantA),
        Just(MyType::VariantB),
        (1u32..100).prop_map(MyType::VariantC),
    ]
}

proptest! {
    #[test]
    fn test_with_custom_strategy(value in my_custom_strategy()) {
        // test code
    }
}
```

**Using `moosicbox_arb` for common constrained types:**

```rust
use moosicbox_arb::css::CssIdentifierString;
use moosicbox_arb::xml::XmlString;

proptest! {
    #[test]
    fn css_class_names_are_valid(name: CssIdentifierString) {
        prop_assert!(is_valid_css_identifier(&name.0));
    }
}
```

**Dev dependencies:**

```toml
[dev-dependencies]
proptest      = { workspace = true }
test-strategy = { workspace = true }
```

For types with `Arbitrary` implementations behind a feature flag, add to `[features]`:

```toml
[features]
arb = ["dep:proptest", "dep:test-strategy"]
```

**Note**: Property-based tests may need `#[serial]` if they interact with global state, just like regular tests.
