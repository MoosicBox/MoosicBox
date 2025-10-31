# Basic Pagination Example

## Summary

This example demonstrates the fundamental features of the `moosicbox_paging` library, including creating pages, fetching data asynchronously, transforming paginated results, and loading all items sequentially or in parallel batches.

## What This Example Demonstrates

- Creating `Page` instances with known totals and "has more" indicators
- Using `PagingResponse` to fetch additional pages asynchronously
- Accessing page metadata (offset, limit, total, has_more, remaining)
- Transforming paginated data using `map()`
- Fetching all remaining items sequentially with `rest_of_items()`
- Fetching all items in parallel batches with `with_rest_of_items_in_batches()`
- Working with individual pages using `rest_of_pages()`
- Mapping transformations across all pages (current and future)

## Prerequisites

- Basic understanding of Rust and async/await syntax
- Familiarity with pagination concepts (offset, limit, total)
- Tokio runtime basics (the example uses `#[tokio::main]`)

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

The example produces output showing:

1. **Basic Page Creation**: Details of pages with totals and "has more" indicators
2. **Data Transformations**: String to uppercase and string to number conversions
3. **Single Page Fetch**: Loading the first page with metadata
4. **Sequential Fetching**: Loading all items one page at a time
5. **Batch Fetching**: Loading all items in parallel batches (faster)
6. **Working with Pages**: Accessing individual pages instead of just items
7. **Response Mapping**: Transforming data across all pages

Example output:

```
===========================================
  MoosicBox Paging - Basic Example
===========================================

=== Basic Page Creation ===
Page with total:
  Items: ["item1", "item2", "item3"]
  Offset: 0
  Limit: 10
  Total: Some(100)
  Has more: true
  Remaining: Some(90)

Page with 'has more' indicator:
  Items: [1, 2, 3, 4, 5]
  Offset: 20
  Limit: 5
  Total: None
  Has more: true

=== Data Transformations ===
Original strings mapped to uppercase:
  Items: ["HELLO", "WORLD", "RUST"]

Strings parsed to numbers:
  Items: [1, 2, 3, 4, 5]

=== Fetching a Single Page ===
  Fetched 10 items (offset: 0, limit: 10)
First page details:
  Items count: 10
  First item: Item 1
  Last item: Item 10
  Total available: Some(50)
  Has more: true

...
```

## Code Walkthrough

### 1. Creating Basic Pages

The example shows two types of pages:

```rust
// Page with known total (best when you know the total count)
let page = Page::WithTotal {
    items: vec!["item1", "item2", "item3"],
    offset: 0,
    limit: 10,
    total: 100,
};

// Page with "has more" indicator (when total is unknown)
let page = Page::WithHasMore {
    items: vec![1, 2, 3, 4, 5],
    offset: 20,
    limit: 5,
    has_more: true,
};
```

Use `Page::WithTotal` when you know the exact total count (e.g., from a database COUNT query). Use `Page::WithHasMore` when you only know if more items exist (e.g., checking if result size equals limit).

### 2. Simulating a Data Source

The `fetch_page()` function simulates fetching data from a database or API:

```rust
async fn fetch_page(offset: u32, limit: u32) -> Result<PagingResponse<String, String>, String> {
    let db = simulate_database();
    let items: Vec<String> = db.into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();

    let page = Page::WithTotal {
        items,
        offset,
        limit,
        total: 50,
    };

    // PagingResponse wraps the page and knows how to fetch more
    let response = PagingResponse::new(page, move |next_offset, next_limit| {
        Box::pin(fetch_page(next_offset, next_limit))
    });

    Ok(response)
}
```

The fetch function passed to `PagingResponse::new()` is called automatically when fetching additional pages.

### 3. Transforming Paginated Data

You can transform page data using `map()`:

```rust
let page = Page::WithTotal {
    items: vec!["hello", "world", "rust"],
    offset: 0,
    limit: 3,
    total: 3,
};

let uppercase_page = page.map(|s| s.to_uppercase());
// uppercase_page.items() is now ["HELLO", "WORLD", "RUST"]
```

### 4. Fetching All Items Sequentially

To load all remaining items one page at a time:

```rust
let response = fetch_page(0, 10).await?;
let remaining_items = response.rest_of_items().await?;
```

This fetches pages sequentially: page 2, then page 3, then page 4, etc.

### 5. Fetching All Items in Parallel Batches

For better performance with known totals, fetch in parallel:

```rust
let response = fetch_page(0, 10).await?;
let all_items = response.with_rest_of_items_in_batches().await?;
```

This issues all remaining page requests concurrently, which is much faster for remote data sources.

### 6. Working with Pages Directly

Sometimes you need access to individual pages, not just items:

```rust
let response = fetch_page(0, 15).await?;
let pages = response.rest_of_pages().await?;

for page in pages {
    println!("Page with {} items at offset {}", page.items().len(), page.offset());
}
```

### 7. Mapping Transformations Across All Pages

`PagingResponse::map()` transforms both the current page and all future pages:

```rust
let response = fetch_page(0, 5).await?;
let uppercase_response = response.map(|s| s.to_uppercase());

// Current page is transformed
println!("{:?}", uppercase_response.items());

// Future pages will also be transformed
let remaining = uppercase_response.rest_of_items().await?;
```

## Key Concepts

### Page vs PagingResponse

- **`Page<T>`**: A snapshot of a single page of data with metadata (offset, limit, total/has_more)
- **`PagingResponse<T, E>`**: A page plus a fetch function to load additional pages asynchronously

### Offset and Limit

- **Offset**: The starting position in the full result set (0-indexed)
- **Limit**: The maximum number of items per page
- Example: offset=20, limit=10 means "give me 10 items starting at position 20"

### Total vs Has More

- **Total**: Use when you know the exact count of all items (enables parallel batch fetching)
- **Has More**: Use when you only know if more items exist (requires sequential fetching)

### Sequential vs Batch Fetching

- **Sequential** (`rest_of_items()`): Fetches pages one at a time, waiting for each to complete
- **Batch** (`rest_of_items_in_batches()`): Fetches all remaining pages concurrently (requires known total)

Batch fetching is significantly faster for remote data sources but requires `Page::WithTotal`.

### Error Handling

All async fetch methods return `Result<T, E>` where `E` is your error type. The example uses `String` for simplicity, but you should use proper error types in production code.

## Testing the Example

To verify the example works correctly:

1. **Run the example**: You should see output from all 7 demonstration sections
2. **Check page counts**: Sequential and batch fetching should load all 50 items
3. **Verify transformations**: Uppercase and number parsing should work correctly
4. **Observe async behavior**: Batch fetching output shows multiple concurrent requests

## Troubleshooting

### Compilation Errors

- Ensure you're using a recent Rust version (1.70+)
- Verify tokio is available with the correct features
- Check that workspace dependencies are properly configured

### Runtime Errors

- If fetching fails, check the simulated database range (1..=50)
- Ensure offset + limit doesn't exceed available items
- Verify async runtime is initialized with `#[tokio::main]`

## Related Examples

- For real database integration, see database-specific examples (if available)
- For API pagination patterns, see HTTP client examples (if available)
- For advanced transformations, see the mapping and error handling examples (if available)
