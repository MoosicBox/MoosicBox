# Clippier Test Utilities

Test utilities for the `clippier` package that provide helper functions and utilities for testing Rust code analysis and linting functionality.

## Overview

This package contains shared test utilities used by `clippier` for testing various aspects of Rust code analysis, including:

- Workspace analysis utilities
- Test fixture management
- Code parsing helpers
- Lint rule testing infrastructure

## Features

- **Workspace Testing**: Utilities for testing multi-package Rust workspaces
- **Fixture Management**: Helper functions for managing test code fixtures
- **Analysis Helpers**: Common utilities for code analysis testing
- **Mock Infrastructure**: Mock implementations for testing complex scenarios

## Usage

This package is primarily intended for internal use by the `clippier` package's test suite.

```rust
use clippier_test_utilities::*;

#[test]
fn test_workspace_analysis() {
    let workspace = create_test_workspace();
    // Test workspace analysis functionality
}
```

## Test Fixtures

The package includes utilities for working with test fixtures located in the `test-resources/` directory, providing realistic workspace structures for comprehensive testing.

## Internal Use

This package is designed for internal use by the `clippier` project and may not be suitable for external consumption. The API may change without notice as it evolves to support `clippier`'s testing needs.