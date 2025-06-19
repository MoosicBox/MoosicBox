# MoosicBox Simvar

Simulation variable system and testing harness for deterministic async testing.

## Overview

The MoosicBox Simvar package provides:

- **Simulation Harness**: Re-exports simulation testing framework
- **Utilities**: Optional simulation utilities (with `utils` feature)
- **Testing Framework**: Deterministic async testing capabilities
- **Variable Management**: Simulation variable handling and control

## Features

### Simulation Harness
- **Deterministic Testing**: Predictable async test execution
- **Variable Control**: Manage simulation variables and state
- **Test Framework**: Comprehensive testing utilities
- **Async Support**: Full async/await testing support

### Optional Utilities
- **Simulation Utils**: Additional utilities (with `utils` feature)
- **Helper Functions**: Common simulation operations
- **Variable Manipulation**: Advanced variable control

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_simvar = { path = "../simvar" }

# Enable utilities
moosicbox_simvar = {
    path = "../simvar",
    features = ["utils"]
}
```

## Usage

### Basic Simulation

```rust
use moosicbox_simvar::*;

// Use simulation harness functionality
// (Re-exported from simvar_harness)
```

### With Utilities

```rust
#[cfg(feature = "utils")]
use moosicbox_simvar::utils;

#[cfg(feature = "utils")]
{
    // Use simulation utilities
    // (Available when utils feature is enabled)
}
```

## Feature Flags

- **`utils`**: Enable simulation utilities module

## Dependencies

- **Simvar Harness**: Core simulation testing framework
- **Simvar Utils**: Optional utilities (feature-gated)

## Integration

This package is designed for:
- **Testing**: Deterministic async testing
- **Simulation**: Variable-based simulation systems
- **Development**: Testing framework for async applications
- **Quality Assurance**: Reliable and predictable test execution

## Note

This package serves as a facade for the simulation variable system, re-exporting functionality from the core harness and providing optional utilities. It provides a unified interface for simulation-based testing in MoosicBox applications.
