# Basic Pagination Example

This example demonstrates the core features of the `moosicbox_paging` library, including page creation, async pagination, and data transformation.

## Summary

A comprehensive example showing how to use `moosicbox_paging` for handling paginated data with both synchronous page creation and asynchronous lazy loading patterns. The example simulates a data source with 100 items and demonstrates various ways to fetch and process paginated results.

## What This Example Demonstrates

- Creating basic `Page` instances with total counts and "has more" indicators
- Building async `PagingResponse` objects with lazy loading capabilities
- Fetching additional pages sequentially vs in parallel batches
- Retrieving all remaining items efficiently
- Transforming paginated data with `map` operations
- Including or excluding the current page in bulk operations
- Understanding the difference between pages and items in pagination

## Prerequisites

- Basic understanding of Rust async/await syntax
- Familiarity with pagination concepts (offset, limit, total)
- Knowledge of iterators and data transformation

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/paging/examples/basic_pagination/Cargo.toml
```

Or from within the example directory:

```bash
cd packages/paging/examples/basic_pagination
cargo run
```

## Expected Output

The example runs seven demonstrations in sequence, each showing different pagination features:

```
MoosicBox Paging - Basic Pagination Example
============================================

=== Demo 1: Basic Page Creation ===
Page with total:
  Items: ["Apple", "Banana", "Cherry"]
  Offset: 0
  Limit: 3
  Total: Some(10)
  Has more: true
  Remaining: Some(7)

Page with has_more:
  Items: [1, 2, 3, 4, 5]
  Offset: 20
  Has more: true
  Total: None

=== Demo 2: Fetching a Single Page ===
  [Fetching] offset=0, limit=10
First page loaded:
  Items: ["Item #0", "Item #1", "Item #2"]
  Total items in page: 10
  Total across all pages: Some(100)
  Has more pages: true

=== Demo 3: Sequential Pagination ===
Loading first page...
  [Fetching] offset=0, limit=10
First page: 10 items (offset=0)

Fetching remaining pages sequentially...
  [Fetching] offset=10, limit=10
  [Fetching] offset=20, limit=10
  ...
Loaded 9 additional pages
  Page 2: 10 items (offset=10)
  Page 3: 10 items (offset=20)
  ...

=== Demo 4: Fetching All Remaining Items ===
  [Fetching] offset=0, limit=10
First page: 10 items

Fetching all remaining items...
  [Fetching] offset=10, limit=10
  ...
Loaded 90 additional items
First remaining item: Item #10
Last remaining item: Item #99

=== Demo 5: Batch (Parallel) Pagination ===
Loading first page...
  [Fetching] offset=0, limit=10
First page: 10 items (offset=0)

Fetching remaining pages in parallel batches...
  [Fetching] offset=10, limit=10
  [Fetching] offset=20, limit=10
  ...
Loaded 9 additional pages in 100ms
(Compare this to sequential loading in Demo 3)

=== Demo 6: Data Transformation ===
  [Fetching] offset=0, limit=5
Original items: ["Item #0", "Item #1", "Item #2"]
Transformed items: ["ITEM #0", "ITEM #1", "ITEM #2"]

Fetching next page (will also be transformed)...
  [Fetching] offset=5, limit=5
Next page (transformed): ["ITEM #5", "ITEM #6", "ITEM #7"]

=== Demo 7: Including Current Page ===
  [Fetching] offset=0, limit=10
Fetching ALL items (including current page)...
  [Fetching] offset=10, limit=10
  ...
Total items loaded: 100
First item: Item #0
Last item: Item #99

=== All demos completed successfully! ===
```

## Code Walkthrough

### 1. Creating Basic Pages

The example starts by demonstrating two types of pages:

```rust
// Page with known total - use when you know the exact total count
let page = Page::WithTotal {
    items: vec!["Apple", "Banana", "Cherry"],
    offset: 0,
    limit: 3,
    total: 10,
};

// Page with "has more" indicator - use when you only know if more exists
let page = Page::WithHasMore {
    items: vec![1, 2, 3, 4, 5],
    offset: 20,
    limit: 5,
    has_more: true,
};
```

Both variants provide methods like `offset()`, `limit()`, `items()`, and `has_more()`, but `WithTotal` additionally provides `total()` and `remaining()`.

### 2. Async Pagination with Fetch Function

The `fetch_page` function demonstrates how to create a `PagingResponse` that knows how to fetch additional pages:

```rust
async fn fetch_page(offset: u32, limit: u32) -> Result<PagingResponse<String, String>, String> {
    // Simulate data fetching (database query, API call, etc.)
    let items: Vec<String> = (offset..offset + limit)
        .map(|i| format!("Item #{i}"))
        .collect();

    let page = Page::WithTotal {
        items,
        offset,
        limit,
        total: 100,
    };

    // Return a PagingResponse with a closure to fetch more pages
    Ok(PagingResponse::new(page, move |next_offset, next_limit| {
        Box::pin(fetch_page(next_offset, next_limit))
    }))
}
```

The key insight is that `PagingResponse::new` takes two arguments:

1. The current `Page` of data
2. A closure that can fetch additional pages given an offset and limit

### 3. Sequential vs Batch Loading

The example shows two ways to fetch remaining pages:

**Sequential (one at a time):**

```rust
let remaining_pages = response.rest_of_pages().await?;
```

This fetches pages one by one, waiting for each to complete before starting the next.

**Batch/Parallel (all at once):**

```rust
let remaining_pages = response.rest_of_pages_in_batches().await?;
```

This determines all remaining pages upfront and fetches them concurrently, which is much faster for large datasets.

### 4. Pages vs Items

The API provides methods for both:

- `rest_of_pages()` / `rest_of_pages_in_batches()` - Returns `Vec<Page<T>>`
- `rest_of_items()` / `rest_of_items_in_batches()` - Returns `Vec<T>`

Use pages when you need pagination metadata; use items when you just want the data.

### 5. Data Transformation

The `map` method transforms data in the current page and all future pages:

```rust
let transformed = response.map(|item| item.to_uppercase());
```

This is useful for converting between types or applying business logic to paginated results.

### 6. Including the Current Page

Methods prefixed with `with_` include the current page in the results:

- `with_rest_of_pages()` - Current page + remaining pages
- `with_rest_of_items()` - Current page items + remaining items
- `with_rest_of_pages_in_batches()` - Parallel version

## Key Concepts

### Page Types

`moosicbox_paging` supports two pagination strategies:

1. **Total-based pagination (`WithTotal`)**: Use when you know the exact total count of items across all pages. Common in SQL queries with `COUNT(*)`. Provides `total()` and `remaining()` methods.

2. **Cursor-based pagination (`WithHasMore`)**: Use when you only know if more items exist. Common in NoSQL databases and streaming APIs. Only provides `has_more()`.

### Lazy Loading

`PagingResponse` implements lazy loading - it only fetches additional pages when explicitly requested. This saves bandwidth and resources when users don't need all pages.

### Error Handling

The fetch closure returns `Result<PagingResponse<T, E>, E>`, allowing for error propagation at the page level. If any page fetch fails, the entire operation fails and returns the error.

### Performance Considerations

- Use `rest_of_pages_in_batches()` when you need all pages and can benefit from parallelism
- Use `rest_of_pages()` when fetching pages one at a time (e.g., for progress indicators)
- Consider memory usage when calling `with_rest_of_items()` on large datasets
- The library uses `Arc<Mutex<>>` internally to safely share the fetch function across async tasks

### Type Transformations

Beyond `map()`, the library provides several conversion methods:

- `into()` - Convert items using the `Into` trait
- `try_into()` - Fallible conversion with error handling
- `transpose()` - Convert `PagingResponse<Result<T, E>, E>` to `Result<PagingResponse<T, E>, E>`
- Various `ok_into`, `err_into`, `inner_into` variants for fine-grained control

## Testing the Example

You can modify the example to experiment with different scenarios:

1. **Change the total count**: Modify the `total` value in `fetch_page` to see how pagination handles different dataset sizes.

2. **Adjust page size**: Change the `limit` parameter when calling `fetch_page(0, 10)` to see how it affects the number of pages.

3. **Add errors**: Modify `fetch_page` to return errors under certain conditions to see error handling in action:

    ```rust
    if offset > 50 {
        return Err("Offset too large".to_string());
    }
    ```

4. **Measure performance**: Uncomment or add timing code to compare sequential vs batch loading performance.

5. **Try different transformations**: Experiment with the `map` function to parse strings to numbers, filter data, or apply custom business logic.

## Troubleshooting

### "function cannot be sent between threads safely"

If you see this error, ensure your fetch closure captures only `Send` types. Use `move` closures and avoid non-`Send` types like `Rc`.

### "cannot borrow as mutable"

The fetch function is wrapped in `Arc<Mutex<>>` internally. If you need to access mutable state, use `Arc<Mutex<>>` or other async-safe synchronization primitives.

### Performance is slow

- Make sure you're using `rest_of_pages_in_batches()` for parallel loading
- Check if your data source supports concurrent requests
- Consider implementing connection pooling for database queries
- Use `rest_of_items()` instead of `rest_of_pages()` if you don't need page metadata

### Out of memory errors

When working with very large datasets:

- Fetch and process pages incrementally instead of calling `with_rest_of_items()`
- Consider streaming or processing pages as they arrive
- Implement pagination limits in your application logic

## Related Examples

This is currently the only example for `moosicbox_paging`. For related pagination patterns in the MoosicBox ecosystem, see:

- `packages/database/examples/` - Database pagination examples
- `packages/http/examples/` - HTTP API pagination patterns
- Web server examples that use pagination in their responses
