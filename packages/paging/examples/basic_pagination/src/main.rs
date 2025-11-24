#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic pagination example demonstrating core features of `moosicbox_paging`.
//!
//! This example shows how to:
//! - Create pages with known totals and "has more" indicators
//! - Use `PagingResponse` to fetch additional pages asynchronously
//! - Transform paginated data using `map()` and `try_into()`
//! - Fetch all remaining items sequentially and in batches

use moosicbox_paging::{Page, PagingResponse};

/// Simulates a database with 50 items.
/// In a real application, this would be a database query or API call.
fn simulate_database() -> Vec<String> {
    (1..=50).map(|i| format!("Item {i}")).collect()
}

type FetchFuture = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<PagingResponse<String, String>, String>> + Send>,
>;

/// Helper function to create a fetch closure for `PagingResponse`.
///
/// This returns a closure that can be used to fetch additional pages.
fn make_fetch_fn() -> impl FnMut(u32, u32) -> FetchFuture + Send + 'static {
    move |offset, limit| {
        Box::pin(async move {
            let db = simulate_database();
            let total = u32::try_from(db.len()).unwrap_or(u32::MAX);

            let items: Vec<String> = db
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect();

            println!(
                "  Fetched {} items (offset: {}, limit: {})",
                items.len(),
                offset,
                limit
            );

            let page = Page::WithTotal {
                items,
                offset,
                limit,
                total,
            };

            Ok(PagingResponse::new(page, make_fetch_fn()))
        })
    }
}

/// Fetches a page of items from our simulated database.
///
/// This function demonstrates creating a `PagingResponse` with a fetch function
/// that can retrieve subsequent pages.
fn fetch_page(offset: u32, limit: u32) -> PagingResponse<String, String> {
    let db = simulate_database();
    let total = u32::try_from(db.len()).unwrap_or(u32::MAX);

    // Simulate fetching items from the database
    let items: Vec<String> = db
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect();

    println!(
        "  Fetched {} items (offset: {}, limit: {})",
        items.len(),
        offset,
        limit
    );

    // Create a page with known total
    let page = Page::WithTotal {
        items,
        offset,
        limit,
        total,
    };

    // Create a PagingResponse that knows how to fetch the next page
    PagingResponse::new(page, make_fetch_fn())
}

/// Demonstrates basic page creation and inspection.
fn demonstrate_basic_page() {
    println!("\n=== Basic Page Creation ===");

    // Create a page with known total
    let page = Page::WithTotal {
        items: vec!["item1", "item2", "item3"],
        offset: 0,
        limit: 10,
        total: 100,
    };

    println!("Page with total:");
    println!("  Items: {:?}", page.items());
    println!("  Offset: {}", page.offset());
    println!("  Limit: {}", page.limit());
    println!("  Total: {:?}", page.total());
    println!("  Has more: {}", page.has_more());
    println!("  Remaining: {:?}", page.remaining());

    // Create a page with "has more" indicator (used when total is unknown)
    let page = Page::WithHasMore {
        items: vec![1, 2, 3, 4, 5],
        offset: 20,
        limit: 5,
        has_more: true,
    };

    println!("\nPage with 'has more' indicator:");
    println!("  Items: {:?}", page.items());
    println!("  Offset: {}", page.offset());
    println!("  Limit: {}", page.limit());
    println!("  Total: {:?}", page.total());
    println!("  Has more: {}", page.has_more());
}

/// Demonstrates data transformation using map and `try_into`.
fn demonstrate_transformations() {
    println!("\n=== Data Transformations ===");

    // Transform strings to uppercase
    let page = Page::WithTotal {
        items: vec!["hello", "world", "rust"],
        offset: 0,
        limit: 3,
        total: 3,
    };

    let uppercase_page = page.map(str::to_uppercase);
    println!("Original strings mapped to uppercase:");
    println!("  Items: {:?}", uppercase_page.items());

    // Parse strings to numbers
    let numbers_page = Page::WithTotal {
        items: vec!["1", "2", "3", "4", "5"],
        offset: 0,
        limit: 5,
        total: 5,
    };

    let parsed_page: Page<i32> = numbers_page.map(|s| s.parse().expect("Valid number"));
    println!("\nStrings parsed to numbers:");
    println!("  Items: {:?}", parsed_page.items());
}

/// Demonstrates fetching a single page with `PagingResponse`.
fn demonstrate_single_page() {
    println!("\n=== Fetching a Single Page ===");

    let response = fetch_page(0, 10);

    println!("First page details:");
    println!("  Items count: {}", response.items().len());
    println!("  First item: {}", response.items()[0]);
    println!("  Last item: {}", response.items()[9]);
    println!("  Total available: {:?}", response.total());
    println!("  Has more: {}", response.has_more());
}

/// Demonstrates fetching all remaining items sequentially.
async fn demonstrate_sequential_fetching() -> Result<(), String> {
    println!("\n=== Sequential Fetching of All Items ===");

    let response = fetch_page(0, 10);
    println!("Initial page loaded with {} items", response.items().len());

    // Fetch all remaining items (not including the current page)
    println!("\nFetching remaining items sequentially...");
    let remaining_items = response.rest_of_items().await?;

    println!("Total remaining items fetched: {}", remaining_items.len());
    println!("Last item: {}", remaining_items.last().unwrap());

    Ok(())
}

/// Demonstrates fetching all items (including current page) in parallel batches.
async fn demonstrate_batch_fetching() -> Result<(), String> {
    println!("\n=== Batch Fetching (Parallel) ===");

    let response = fetch_page(0, 10);
    println!("Initial page loaded with {} items", response.items().len());

    // Fetch all items in parallel batches (including the current page)
    println!("\nFetching all items in parallel batches...");
    let all_items = response.with_rest_of_items_in_batches().await?;

    println!("Total items fetched: {}", all_items.len());
    println!("First item: {}", all_items[0]);
    println!("Last item: {}", all_items[all_items.len() - 1]);

    Ok(())
}

/// Demonstrates working with pages (not just items).
async fn demonstrate_working_with_pages() -> Result<(), String> {
    println!("\n=== Working with Pages ===");

    let response = fetch_page(0, 15);
    println!("Initial page loaded");

    // Fetch all remaining pages
    println!("\nFetching remaining pages...");
    let pages = response.rest_of_pages().await?;

    println!("Total pages fetched: {}", pages.len());

    for (i, page) in pages.iter().enumerate() {
        println!(
            "  Page {}: {} items (offset: {}, limit: {})",
            i + 1,
            page.items().len(),
            page.offset(),
            page.limit()
        );
    }

    Ok(())
}

/// Demonstrates transforming `PagingResponse` data.
async fn demonstrate_response_mapping() -> Result<(), String> {
    println!("\n=== Transforming PagingResponse ===");

    let response = fetch_page(0, 5);

    // Transform all items (current and future pages) to uppercase
    let uppercase_response = response.map(|s| s.to_uppercase());

    println!("Transformed items in current page:");
    for item in uppercase_response.items() {
        println!("  {item}");
    }

    // Fetch the rest and see they're also transformed
    println!("\nFetching remaining items (also transformed)...");
    let remaining = uppercase_response.rest_of_items().await?;
    println!("Remaining items count: {} (all uppercase)", remaining.len());

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), String> {
    println!("===========================================");
    println!("  MoosicBox Paging - Basic Example");
    println!("===========================================");

    // Demonstrate basic page creation and inspection
    demonstrate_basic_page();

    // Demonstrate data transformations
    demonstrate_transformations();

    // Demonstrate fetching a single page
    demonstrate_single_page();

    // Demonstrate sequential fetching
    demonstrate_sequential_fetching().await?;

    // Demonstrate batch fetching
    demonstrate_batch_fetching().await?;

    // Demonstrate working with pages
    demonstrate_working_with_pages().await?;

    // Demonstrate response mapping
    demonstrate_response_mapping().await?;

    println!("\n===========================================");
    println!("  All demonstrations completed!");
    println!("===========================================");

    Ok(())
}
