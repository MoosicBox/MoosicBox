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
moosicbox_paging = "0.1.1"

# Enable OpenAPI schema generation
moosicbox_paging = { version = "0.1.1", features = ["openapi"] }
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

// Try converting with error handling
let strings = Page::WithTotal {
    items: vec!["1", "2", "invalid", "4"],
    offset: 0,
    limit: 4,
    total: 4,
};

let result: Result<Page<i32>, _> = strings.try_into();
match result {
    Ok(numbers) => println!("All parsed successfully"),
    Err(e) => println!("Parse error: {}", e.error),
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
use moosicbox_paging::PagingResponse;

async fn process_with_errors() -> Result<(), String> {
    let response: PagingResponse<Result<i32, String>, String> = /* ... */;

    // Transform error types
    let mapped = response.map_err(|e| format!("Database error: {}", e));

    // Transpose Result<PagingResponse<T, E>, E> to PagingResponse<T, E>
    let transposed = response.transpose()?;

    Ok(())
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

- `items()`: Get items in the current page
- `offset()`, `limit()`: Get pagination parameters
- `total()`: Get total count (if available)
- `has_more()`: Check if more data is available
- `rest_of_items()`: Load all remaining items
- `map()`: Transform item types
- `try_into()`: Convert with error handling

## Dependencies

- `serde`: Serialization support
- `futures`: Async utilities
- `tokio`: Async runtime support

This library provides efficient pagination utilities for building responsive APIs and handling large datasets in the MoosicBox ecosystem.
