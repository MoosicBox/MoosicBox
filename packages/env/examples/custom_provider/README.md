# Custom Provider Example

Demonstrates how to implement custom environment variable providers by implementing the `EnvProvider` trait.

## Summary

This example shows advanced patterns for creating custom environment variable sources, including static providers, merged configurations, prefixed variables, and case-insensitive lookups.

## What This Example Demonstrates

- Implementing the `EnvProvider` trait for custom variable sources
- Creating a static in-memory provider
- Building a merged provider with priority-based fallback behavior
- Implementing a prefixed provider for namespaced variables
- Creating a case-insensitive variable lookup provider
- Combining multiple providers using generic composition
- Using all `EnvProvider` trait methods with custom implementations

## Prerequisites

- Understanding of Rust traits and trait implementations
- Familiarity with basic switchy_env API (see `basic_usage` example)
- Knowledge of configuration management patterns
- Understanding of `BTreeMap` for ordered key-value storage

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/env/examples/custom_provider/Cargo.toml
```

## Expected Output

```
=== switchy_env Custom Provider Example ===

1. Static environment provider:
   APP_NAME = MyApp
   APP_VERSION = 1.0.0
   PORT = 8080
   DEBUG (parsed) = true
   UNKNOWN (with default) = default_value

2. Case-insensitive environment provider:
   database_url = sqlite::memory:
   DATABASE_URL = sqlite::memory:
   DaTaBaSe_UrL = sqlite::memory:

3. Prefixed environment provider:
   HOST = localhost
   PORT = 3000
   Available vars: {"HOST", "PORT"}

4. Merged environment provider:
   PORT = 9000 (from high priority)
   APP_NAME = MyApp (from low priority)
   CUSTOM = high_priority_value (only in high priority)

5. Complex configuration with composition:
   ENV = production (from production)
   PORT = 443 (from production)
   HOST = 0.0.0.0 (from defaults)
   WORKERS = 4 (from defaults)

6. Using `EnvProvider` trait methods:
   PORT exists: 8080
   MISSING not set (as expected)

=== Example Complete ===

Key takeaway: Implement `EnvProvider` to create custom environment
variable sources for advanced configuration management.
```

## Code Walkthrough

### 1. Implementing a Basic Provider

```rust
use switchy_env::{EnvProvider, EnvError, Result};
use std::collections::BTreeMap;

struct StaticEnv {
    vars: BTreeMap<String, String>,
}

impl EnvProvider for StaticEnv {
    fn var(&self, name: &str) -> Result<String> {
        self.vars
            .get(name)
            .cloned()
            .ok_or_else(|| EnvError::NotFound(name.to_string()))
    }

    fn vars(&self) -> BTreeMap<String, String> {
        self.vars.clone()
    }
}
```

The `EnvProvider` trait requires two methods:

- `var(&self, name: &str) -> Result<String>`: Get a single variable
- `vars(&self) -> BTreeMap<String, String>`: Get all variables

All other methods (`var_or`, `var_parse`, etc.) have default implementations based on these two.

### 2. Merged Provider with Priority

```rust
struct MergedEnv<T1: EnvProvider, T2: EnvProvider> {
    high_priority: T1,
    low_priority: T2,
}

impl<T1: EnvProvider, T2: EnvProvider> EnvProvider for MergedEnv<T1, T2> {
    fn var(&self, name: &str) -> Result<String> {
        self.high_priority
            .var(name)
            .or_else(|_| self.low_priority.var(name))
    }

    fn vars(&self) -> BTreeMap<String, String> {
        let mut all_vars = self.low_priority.vars();
        all_vars.extend(self.high_priority.vars());
        all_vars
    }
}
```

The merged provider checks high priority first, then falls back to low priority. This is useful for:

- Development vs. production configs
- User overrides with system defaults
- Environment-specific settings with global fallbacks

### 3. Prefixed Provider

```rust
struct PrefixedEnv<T: EnvProvider> {
    prefix: String,
    inner: T,
}

impl<T: EnvProvider> EnvProvider for PrefixedEnv<T> {
    fn var(&self, name: &str) -> Result<String> {
        let prefixed = format!("{}_{}", self.prefix, name);
        self.inner.var(&prefixed)
    }
}
```

The prefixed provider adds a namespace to all variable names. For example:

- `var("PORT")` looks up `MYAPP_PORT` in the underlying provider
- Useful for multi-tenant configurations
- Prevents variable name collisions

### 4. Case-Insensitive Provider

```rust
struct CaseInsensitiveEnv {
    vars: BTreeMap<String, String>,
}

impl EnvProvider for CaseInsensitiveEnv {
    fn var(&self, name: &str) -> Result<String> {
        let upper_name = name.to_uppercase();
        self.vars.get(&upper_name).cloned()
            .ok_or_else(|| EnvError::NotFound(name.to_string()))
    }
}
```

This provider normalizes variable names to uppercase, allowing case-insensitive lookups. Useful for Windows compatibility or user-friendly configuration.

### 5. Composing Providers

```rust
let prod_env = StaticEnv::from_map(prod_config);
let default_env = StaticEnv::from_map(defaults);

let merged = MergedEnv::new(prod_env, default_env);
let prefixed = PrefixedEnv::new("MYAPP", merged);
```

Providers can be composed to create sophisticated configuration hierarchies:

1. Start with defaults
2. Merge production config on top (higher priority)
3. Add prefix for namespacing
4. Result: Production values override defaults, all namespaced under MYAPP\_

## Key Concepts

### EnvProvider Trait

The `EnvProvider` trait provides a unified interface for environment variable access:

```rust
pub trait EnvProvider: Send + Sync {
    // Required methods
    fn var(&self, name: &str) -> Result<String>;
    fn vars(&self) -> BTreeMap<String, String>;

    // Provided methods (implemented automatically)
    fn var_or(&self, name: &str, default: &str) -> String { ... }
    fn var_parse<T>(&self, name: &str) -> Result<T> { ... }
    fn var_parse_or<T>(&self, name: &str, default: T) -> T { ... }
    fn var_parse_opt<T>(&self, name: &str) -> Result<Option<T>> { ... }
    fn var_exists(&self, name: &str) -> bool { ... }
}
```

### Thread Safety

All providers must be `Send + Sync`:

- `Send`: Can be transferred between threads
- `Sync`: Can be referenced from multiple threads

This allows providers to be used in concurrent contexts safely.

### Use Cases for Custom Providers

**Configuration Files**:

```rust
// Load environment from JSON, TOML, YAML, etc.
struct FileEnv {
    vars: BTreeMap<String, String>,
}

impl FileEnv {
    fn from_json(path: &Path) -> Result<Self> {
        // Load and parse JSON file
    }
}
```

**Remote Configuration**:

```rust
// Fetch configuration from a service
struct RemoteEnv {
    cache: Arc<RwLock<BTreeMap<String, String>>>,
}

impl RemoteEnv {
    fn new(config_url: &str) -> Self {
        // Fetch from remote service
    }
}
```

**Encrypted Variables**:

```rust
// Decrypt variables on access
struct EncryptedEnv {
    inner: Box<dyn EnvProvider>,
    encryption_key: Vec<u8>,
}

impl EnvProvider for EncryptedEnv {
    fn var(&self, name: &str) -> Result<String> {
        let encrypted = self.inner.var(name)?;
        Ok(decrypt(&encrypted, &self.encryption_key))
    }
}
```

**Validation Layer**:

```rust
// Validate variables match expected schemas
struct ValidatingEnv {
    inner: Box<dyn EnvProvider>,
    schema: ValidationSchema,
}
```

## Testing the Example

Modify the example to explore different patterns:

1. **Add new provider types**: Implement providers for TOML, JSON, or YAML files
2. **Add validation**: Create a validating provider that checks variable formats
3. **Add caching**: Implement a caching layer for expensive lookups
4. **Test ordering**: Change the layer order to see precedence effects

Example modification - add a default value provider:

```rust
struct DefaultEnv {
    defaults: BTreeMap<String, String>,
    inner: Box<dyn EnvProvider>,
}

impl EnvProvider for DefaultEnv {
    fn var(&self, name: &str) -> Result<String> {
        self.inner.var(name)
            .or_else(|_| {
                self.defaults.get(name)
                    .cloned()
                    .ok_or_else(|| EnvError::NotFound(name.to_string()))
            })
    }
}
```

## Troubleshooting

### Type Object Safety Issues

If you get "trait cannot be made into an object" errors:

- Ensure your trait methods don't use `Self` in return positions
- Generic methods may not work with `Box<dyn EnvProvider>`
- Use concrete types or associated types instead

### Clone vs. Reference

When storing providers:

```rust
// This works:
struct MyEnv {
    inner: Box<dyn EnvProvider>,
}

// This doesn't (providers may not be Clone):
struct MyEnv {
    inner: dyn EnvProvider,  // Unsized type
}
```

### Thread Safety

If you get "not Send/Sync" errors:

- Ensure all fields in your provider are `Send + Sync`
- Use `Arc` instead of `Rc` for shared state
- Use `RwLock` or `Mutex` instead of `RefCell`

## Related Examples

- **basic_usage**: Learn the fundamental EnvProvider API usage
- **simulator_testing**: See the built-in simulator provider implementation
