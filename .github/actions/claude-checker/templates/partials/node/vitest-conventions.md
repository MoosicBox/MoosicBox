---
# Partial: Vitest-Specific Conventions
# Expected variables (with defaults)
package_name: ''
---

### Vitest-Specific Patterns

**Imports:**

```typescript
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
```

**Mocking Modules:**

```typescript
// Mock an entire module
vi.mock('./myModule', () => ({
    myFunction: vi.fn(() => 'mocked'),
    myOtherFunction: vi.fn(),
}));

// Mock with actual implementation for some exports
vi.mock('./myModule', async (importOriginal) => {
    const actual = await importOriginal();
    return {
        ...actual,
        myFunction: vi.fn(() => 'mocked'),
    };
});
```

**Spying:**

```typescript
// Spy on an object method
const spy = vi.spyOn(object, 'method');

// Spy and mock implementation
vi.spyOn(object, 'method').mockImplementation(() => 'mocked');

// Spy and mock return value
vi.spyOn(object, 'method').mockReturnValue('mocked');

// Spy on async method
vi.spyOn(object, 'asyncMethod').mockResolvedValue('mocked');
```

**Mock Functions:**

```typescript
// Create a mock function
const mockFn = vi.fn();

// With implementation
const mockFn = vi.fn((x) => x * 2);

// Mock return value
mockFn.mockReturnValue(42);
mockFn.mockReturnValueOnce(42);

// Mock resolved value (for async)
mockFn.mockResolvedValue({ data: 'test' });
mockFn.mockRejectedValue(new Error('failed'));

// Check calls
expect(mockFn).toHaveBeenCalled();
expect(mockFn).toHaveBeenCalledWith('arg1', 'arg2');
expect(mockFn).toHaveBeenCalledTimes(2);
```

**Resetting Mocks:**

```typescript
beforeEach(() => {
    vi.clearAllMocks(); // Clear call history
    // or
    vi.resetAllMocks(); // Clear history + reset implementations
    // or
    vi.restoreAllMocks(); // Restore original implementations
});
```

**Timers:**

```typescript
beforeEach(() => {
    vi.useFakeTimers();
});

afterEach(() => {
    vi.useRealTimers();
});

it('should handle timeout', async () => {
    const callback = vi.fn();
    setTimeout(callback, 1000);

    await vi.advanceTimersByTimeAsync(1000);

    expect(callback).toHaveBeenCalled();
});
```

**Inline Snapshots:**

```typescript
it('should match snapshot', () => {
    const result = formatOutput(data);
    expect(result).toMatchInlineSnapshot(`
        {
            "key": "value",
        }
    `);
});
```

**Soft Assertions (continue after failure):**

```typescript
it('should validate multiple fields', () => {
    const result = validate(input);

    expect.soft(result.name).toBe('expected');
    expect.soft(result.value).toBe(42);
    expect.soft(result.valid).toBe(true);
    // All assertions run even if earlier ones fail
});
```

**Running Tests:**

```bash
# Run all tests
${pm_run('test')}

# Run with coverage
${pm_run('test')} -- --coverage

# Run specific file
${pm_run('test')} -- path/to/file.test.ts

# Run tests matching pattern
${pm_run('test')} -- -t "should handle"

# Watch mode
${pm_run('test')} -- --watch
```
