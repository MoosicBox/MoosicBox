---
# Partial: Rust Test Selection Criteria
# Expected variables: none required
---

## CRITICAL: Test Selection Criteria

You must ONLY add tests that meet ALL of the following criteria:

1. **Clear Scope**: You must VERY CLEARLY understand the exact scope and behavior of the code being tested
2. **No Duplication**: You must be ABSOLUTELY SURE there are no other tests that test the same thing
3. **Meaningful Value**: The test must provide USEFUL coverage, not just test trivial or obvious behavior
4. **Real Gaps**: The test must fill an actual gap in test coverage, not redundantly test well-covered code

**DO NOT** add tests for:

- **Simple getters/setters with no logic** - Functions that just return or set a field value without any transformation, validation, or business logic
- **Builder pattern methods** - Do not test `with_*()` or `set_*()` builder methods that simply add values to collections or set fields. These are trivial setters. Example: testing that `with_listener(callback)` adds the callback to a Vec is redundant
- **Trivial type conversions** - Standard trait implementations like `ToString`, `AsRef`, `Into`, `From`, `FromStr` that simply convert between types without complex logic
- **Trivial constructors** - Constructors that just assign values to fields without validation or setup logic (e.g., `new()`, `default()`)
- **Code already well-tested through integration tests** - Avoid redundant unit tests for behavior that is thoroughly covered by integration/end-to-end tests
- **Obvious behavior that doesn't need verification** - Self-evident functionality that would fail to compile if incorrect
- **Simple forwarding functions with no logic** - Functions that just call another function or method without transformation
- **Debug/Display trait implementations** - Do not test formatting output of Debug or Display traits (e.g., `format!("{:?}", value)` or `value.to_string()`). This includes Display impls that delegate to strum's `AsRef` or similar derive macros
- **Clone trait implementations** - Do not test that Clone works correctly, trust the derive macro or manual implementation
- **External dependency behavior** - Do not test that external libraries (e.g., flume channels, tokio, etc.) work correctly. Trust that dependencies are tested by their maintainers
- **Derived or auto-generated trait implementations** - Do not test traits that are derived (Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize, etc.). This INCLUDES strum derives like `EnumString`, `AsRefStr`, `Display`, `EnumIter` - do NOT test that FromStr/Display/AsRef work correctly for enums using strum derives
- **Standard library trait implementations** - Do not test standard conversions (AsRef, Into, From) unless they contain complex logic beyond simple type conversion
- **Default trait implementations** - Do not test that Default::default() creates expected default values (e.g., testing that fields are None, empty, or have default values)
- **Serde serialization/deserialization** - Do not test simple serde serialization/deserialization unless there is complex custom logic
- **Strum-derived enum traits** - Do not test `FromStr`, `Display`, `AsRef`, or iteration behavior for enums that derive from strum macros (e.g., `EnumString`, `AsRefStr`, `Display`, `EnumIter`). The strum library is well-tested; trust the derive macros
- **Error Display formatting** - Do not test error Display or Debug output unless there is custom formatting logic beyond what thiserror or derive macros provide
- **Arbitrary/Generator constraints** - Do not test that property-based test generators satisfy their own constraints (e.g., testing that a generator for valid CSS doesn't generate invalid CSS)
- **Parser tests for trivial grammars** - Do not test that parsers correctly parse their own defined grammar unless testing edge cases or error handling
- **Constant values** - Do not test that constants have specific values unless the values are computed or have complex initialization logic

**Why avoid these tests?**
These tests provide minimal value because:

1. They test the compiler or standard library, not your code
2. They're brittle - they break when refactoring without indicating real bugs
3. They clutter the test suite and reduce signal-to-noise ratio
4. They give false confidence in coverage metrics
5. If these basic operations fail, many other tests will also fail, making the failure obvious

**DO** add tests for:

- Complex business logic
- Edge cases and error conditions
- State transitions and validation logic
- Data transformations and calculations
- Concurrent operations and race conditions
- Error handling paths
