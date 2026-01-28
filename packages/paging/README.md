# MoosicBox Paging

Pagination library for the MoosicBox ecosystem, providing basic data pagination utilities with async support for handling large datasets in web APIs and database queries.

## Features

- **Page Types**: Support for pages with total counts or "has more" indicators
- **Async Pagination**: Async-friendly pagination with lazy loading support
- **Result Mapping**: Transform paginated data with map and try_into operations
- **Batch Processing**: Load multiple pages or all remaining items at once
- **Error Handling**: Built-in error handling for pagination operations
- **Serialization**: Serde support for JSON API responses

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_paging = "0.1.4"

# OpenAPI schema generation is enabled by default
# To disable it:
moosicbox_paging = { version = "0.1.4", default-features = false }
```

## Usage

### Basic Page Creation

```rust
use moosicbox_paging::Page;

// Create a page with total count
let page = Page::WithTotal {
    items: vec!["item1", "item2", "item3"],
    offset: 0,
    limit: 10,
    total: 100,
};

println!("Page has {} items out of {} total", page.items().len(), page.total().unwrap());
println!("Has more: {}", page.has_more());

// Create a page with "has more" indicator
let page = Page::WithHasMore {
    items: vec![1, 2, 3, 4, 5],
    offset: 20,
    limit: 5,
    has_more: true,
};

println!("Current offset: {}, limit: {}", page.offset(), page.limit());
```

### Async Pagination with PagingResponse

```rust
use moosicbox_paging::{PagingResponse, Page};
use std::pin::Pin;
use futures::Future;

async fn fetch_data(offset: u32, limit: u32) -> Result<PagingResponse<String, String>, String> {
    // Simulate data fetching
    let items: Vec<String> = (offset..offset + limit)
        .map(|i| format!("Item {}", i))
        .collect();

    let page = Page::WithTotal {
        items,
        offset,
        limit,
        total: 1000,
    };

    let response = PagingResponse::new(page, move |next_offset, next_limit| {
        Box::pin(fetch_data(next_offset, next_limit))
    });

    Ok(response)
}

#[tokio::main]
async fn main() -> Result<(), String> {
    // Get first page
    let response = fetch_data(0, 10).await?;

    println!("First page: {} items", response.items().len());

    // Get all remaining items
    let all_items = response.rest_of_items().await?;
    println!("Total items loaded: {}", all_items.len());

    Ok(())
}
```

### Data Transformation

```rust
use moosicbox_paging::Page;

// Map data to different types
let numbers = Page::WithTotal {
    items: vec!["1", "2", "3", "4", "5"],
    offset: 0,
    limit: 5,
    total: 10,
};

let parsed: Page<i32> = numbers.map(|s| s.parse().unwrap());

// Try converting with error handling (i64 -> i32 can fail on overflow)
let large_numbers = Page::WithTotal {
    items: vec![1i64, 2, i64::MAX, 4],
    offset: 0,
    limit: 4,
    total: 4,
};

let result: Result<Page<i32>, _> = large_numbers.try_into();
match result {
    Ok(numbers) => println!("All converted successfully"),
    Err(e) => println!("Conversion error: {:?}", e),
}
```

### Batch Operations

```rust
async fn load_all_pages() -> Result<(), String> {
    let response = fetch_data(0, 10).await?;

    // Load remaining pages one by one
    let pages = response.rest_of_pages().await?;
    println!("Loaded {} additional pages", pages.len());

    // Or load all remaining items at once
    let response = fetch_data(0, 10).await?;
    let items = response.rest_of_items().await?;
    println!("Loaded {} total items", items.len());

    Ok(())
}
```

### Error Handling and Mapping

```rust
use moosicbox_paging::{PagingResponse, Page};

async fn process_with_errors(
    response: PagingResponse<Result<i32, String>, String>
) -> Result<(), String> {
    // Transform error types
    let mapped = response.map_err(|e| format!("Database error: {}", e));

    Ok(())
}

async fn transpose_example(
    response: PagingResponse<Result<i32, String>, String>
) -> Result<PagingResponse<i32, String>, String> {
    // Transpose PagingResponse<Result<T, E>, E> to Result<PagingResponse<T, E>, E>
    response.transpose()
}
```

### Serialization

```rust
use serde::{Serialize, Deserialize};
use moosicbox_paging::Page;

#[derive(Serialize, Deserialize)]
struct ApiResponse {
    data: Page<String>,
    success: bool,
}

// Pages automatically serialize to JSON with camelCase fields
let page = Page::WithTotal {
    items: vec!["a".to_string(), "b".to_string()],
    offset: 0,
    limit: 2,
    total: 10,
};

let json = serde_json::to_string(&page)?;
// Results in: {"items":["a","b"],"offset":0,"limit":2,"total":10,"hasMore":true}
```

## Core Types

### Page<T>

Represents a single page of data with either total count or "has more" information.

### PagingResponse<T, E>

Async-aware pagination with the ability to fetch additional pages lazily.

### PagingRequest

Simple struct for pagination parameters (offset and limit).

## Key Methods

### Page<T> Methods

- `items()`: Get items in the current page as a slice
- `into_items()`: Consume the page and return the items vector
- `offset()`, `limit()`: Get pagination parameters
- `total()`: Get total count (if available)
- `remaining()`: Get remaining items count (if total is available)
- `has_more()`: Check if more data is available
- `map()`: Transform item types
- `into()`: Convert items using Into trait
- `try_into()`: Convert items with error handling using TryInto trait
- `empty()`: Create an empty page

### PagingResponse<T, E> Methods

- `rest_of_pages()`: Load remaining pages one by one
- `rest_of_items()`: Load all remaining items
- `rest_of_pages_in_batches()`: Load remaining pages in parallel batches
- `rest_of_items_in_batches()`: Load all remaining items using parallel batches
- `with_rest_of_pages()`: Include current page and load remaining pages
- `with_rest_of_items()`: Include current page items and load remaining items
- `map()`: Transform item types
- `map_err()`: Transform error types
- `transpose()`: Convert PagingResponse<Result<T, E>, E> to Result<PagingResponse<T, E>, E>
- Various conversion methods: `ok_into`, `ok_try_into`, `err_into`, `inner_into`, `inner_try_into`

## Dependencies

- `serde`: Serialization and deserialization support
- `futures`: Async utilities for pagination
- `switchy_async`: Async utilities with tokio runtime support
- `log`: Logging support for batch operations
- `utoipa`: OpenAPI schema generation (optional, enabled by default)

This library provides efficient pagination utilities for building responsive APIs and handling large datasets in the MoosicBox ecosystem.
