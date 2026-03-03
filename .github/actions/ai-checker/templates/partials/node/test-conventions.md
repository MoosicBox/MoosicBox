---
# Partial: Node/TypeScript Test Conventions
# Expected variables (with defaults)
package_name: ''
---

## Node/TypeScript Testing Conventions

### Test File Organization

- Place tests adjacent to source files (`foo.ts` → `foo.test.ts`) OR in a `__tests__` directory
- Use descriptive test names that explain what is being tested
- Group related tests with `describe` blocks

### Test Structure

**Basic test structure:**

```typescript
describe('MyModule', () => {
    describe('myFunction', () => {
        it('should handle normal input', () => {
            // Arrange
            const input = 'test';

            // Act
            const result = myFunction(input);

            // Assert
            expect(result).toBe('expected');
        });

        it('should throw on invalid input', () => {
            expect(() => myFunction(null)).toThrow();
        });
    });
});
```

### Async Testing

```typescript
it('should fetch data', async () => {
    const result = await fetchData();
    expect(result).toBeDefined();
});
```

### Setup and Teardown

```typescript
describe('DatabaseTests', () => {
    beforeEach(() => {
        // Reset state before each test
    });

    afterEach(() => {
        // Cleanup after each test
    });

    beforeAll(() => {
        // Run once before all tests in this describe block
    });

    afterAll(() => {
        // Run once after all tests in this describe block
    });
});
```

### Test Isolation

- Each test should be independent and not rely on state from other tests
- Use `beforeEach` to reset state, not shared mutable variables
- Clean up any side effects (timers, mocks, event listeners) in `afterEach`

### Naming Conventions

- Test files: `*.test.ts`, `*.spec.ts`, or `__tests__/*.ts`
- Test names should describe the expected behavior: `'should return null when input is empty'`
- Avoid vague names like `'works correctly'` or `'handles edge case'`

${test_framework == 'vitest' ? include('node/vitest-conventions', { package_name: package_name }) : ''}
