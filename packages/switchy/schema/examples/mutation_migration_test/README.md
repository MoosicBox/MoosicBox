# Mutation Migration Test Example

This example demonstrates how to use `switchy_schema` with the `verify_migrations_with_mutations` test utility to test migrations against various database states and operations.

## What This Example Shows

- Creating migrations that handle complex data scenarios
- Using `verify_migrations_with_mutations` for comprehensive testing
- Implementing mutation providers for dynamic test data
- Testing migrations against multiple database states

## Key Features

- **Mutation Testing**: Tests migrations against various database states and operations
- **Dynamic Data**: Uses mutation providers to generate different test scenarios
- **Comprehensive Coverage**: Ensures migrations work under various conditions
- **Edge Case Testing**: Validates behavior with different data patterns

## Running the Example

```bash
cargo run --bin mutation_migration_test
```

This will:
1. Create an in-memory SQLite database
2. Run migrations with various mutations applied
3. Verify that migrations handle all scenarios correctly
4. Display the results

## Migration Structure

The example includes:
- `CreateUsersTable`: Initial table creation
- `UpdateUserStatuses`: Migration that updates user status values
- `TestMutationProvider`: Generates various test scenarios including:
  - Empty database states
  - Databases with different user configurations
  - Edge cases with null/default values

## Use Cases

This pattern is ideal for:
- Testing migrations against production-like data scenarios
- Ensuring migrations work with various data distributions
- Validating edge cases and error conditions
- Comprehensive migration testing before deployment

## Mutation Provider

The mutation provider creates different database states to test against:
- Empty databases
- Databases with various user counts
- Different status value distributions
- Edge cases with special characters and null values