#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic pagination example demonstrating the core features of `moosicbox_paging`.
//!
//! This example shows:
//! - Creating pages with total counts and "has more" indicators
//! - Async pagination with lazy loading
//! - Fetching remaining pages and items
//! - Data transformation with map operations
//! - Sequential vs batch (parallel) page loading

use moosicbox_paging::{Page, PagingResponse};

/// Simulates fetching data from a data source.
/// In a real application, this might be a database query or API call.
async fn fetch_items(offset: u32, limit: u32) -> Result<(Vec<String>, u32), String> {
    println!("  [Fetching] offset={offset}, limit={limit}");

    // Simulate some work (like a database query)
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    // Generate items for this page
    let total: u32 = 100;
    let remaining = total.saturating_sub(offset);
    let items_count = limit.min(remaining);

    let items: Vec<String> = (offset..offset + items_count)
        .map(|i| format!("Item #{i}"))
        .collect();

    Ok((items, total))
}

/// Creates a `PagingResponse` from fetched data.
async fn fetch_page(offset: u32, limit: u32) -> Result<PagingResponse<String, String>, String> {
    let (items, total) = fetch_items(offset, limit).await?;

    // Create a page with total count
    let page = Page::WithTotal {
        items,
        offset,
        limit,
        total,
    };

    // Return a PagingResponse that knows how to fetch more pages
    Ok(PagingResponse::new(page, |next_offset, next_limit| {
        Box::pin(async move {
            let (items, total) = fetch_items(next_offset, next_limit).await?;
            let page = Page::WithTotal {
                items,
                offset: next_offset,
                limit: next_limit,
                total,
            };
            Ok(PagingResponse::new(page, |_, _| {
                Box::pin(async { Ok(PagingResponse::empty()) })
            }))
        })
    }))
}

/// Demonstrates basic page creation and inspection.
fn demo_basic_pages() {
    println!("\n=== Demo 1: Basic Page Creation ===");

    // Create a page with known total
    let page = Page::WithTotal {
        items: vec!["Apple", "Banana", "Cherry"],
        offset: 0,
        limit: 3,
        total: 10,
    };

    println!("Page with total:");
    println!("  Items: {:?}", page.items());
    println!("  Offset: {}", page.offset());
    println!("  Limit: {}", page.limit());
    println!("  Total: {:?}", page.total());
    println!("  Has more: {}", page.has_more());
    println!("  Remaining: {:?}", page.remaining());

    // Create a page with "has more" indicator
    let page = Page::WithHasMore {
        items: vec![1, 2, 3, 4, 5],
        offset: 20,
        limit: 5,
        has_more: true,
    };

    println!("\nPage with has_more:");
    println!("  Items: {:?}", page.items());
    println!("  Offset: {}", page.offset());
    println!("  Has more: {}", page.has_more());
    println!("  Total: {:?}", page.total()); // None for WithHasMore
}

/// Demonstrates fetching a single page asynchronously.
async fn demo_single_page() -> Result<(), String> {
    println!("\n=== Demo 2: Fetching a Single Page ===");

    let response = fetch_page(0, 10).await?;

    println!("First page loaded:");
    println!("  Items: {:?}", &response.items()[..3]); // Show first 3
    println!("  Total items in page: {}", response.items().len());
    println!("  Total across all pages: {:?}", response.total());
    println!("  Has more pages: {}", response.has_more());

    Ok(())
}

/// Demonstrates fetching all remaining pages sequentially.
async fn demo_sequential_pagination() -> Result<(), String> {
    println!("\n=== Demo 3: Sequential Pagination ===");
    println!("Loading first page...");

    let response = fetch_page(0, 10).await?;
    println!(
        "First page: {} items (offset={})",
        response.items().len(),
        response.offset()
    );

    println!("\nFetching remaining pages sequentially...");
    let remaining_pages = response.rest_of_pages().await?;
    println!("Loaded {} additional pages", remaining_pages.len());

    for (i, page) in remaining_pages.iter().enumerate() {
        println!(
            "  Page {}: {} items (offset={})",
            i + 2,
            page.items().len(),
            page.offset()
        );
    }

    Ok(())
}

/// Demonstrates fetching all remaining items (not pages) at once.
async fn demo_fetch_all_items() -> Result<(), String> {
    println!("\n=== Demo 4: Fetching All Remaining Items ===");

    let response = fetch_page(0, 10).await?;
    println!("First page: {} items", response.items().len());

    println!("\nFetching all remaining items...");
    let all_remaining = response.rest_of_items().await?;
    println!("Loaded {} additional items", all_remaining.len());
    println!("First remaining item: {}", all_remaining[0]);
    println!(
        "Last remaining item: {}",
        all_remaining[all_remaining.len() - 1]
    );

    Ok(())
}

/// Demonstrates batch (parallel) loading of pages.
async fn demo_batch_pagination() -> Result<(), String> {
    println!("\n=== Demo 5: Batch (Parallel) Pagination ===");
    println!("Loading first page...");

    let response = fetch_page(0, 10).await?;
    println!(
        "First page: {} items (offset={})",
        response.items().len(),
        response.offset()
    );

    println!("\nFetching remaining pages in parallel batches...");
    let start = std::time::Instant::now();
    let remaining_pages = response.rest_of_pages_in_batches().await?;
    let duration = start.elapsed();

    println!(
        "Loaded {} additional pages in {duration:?}",
        remaining_pages.len()
    );
    println!("(Compare this to sequential loading in Demo 3)");

    Ok(())
}

/// Demonstrates data transformation with map operations.
async fn demo_data_transformation() -> Result<(), String> {
    println!("\n=== Demo 6: Data Transformation ===");

    let response = fetch_page(0, 5).await?;
    println!("Original items: {:?}", &response.items()[..3]);

    // Transform the data using map
    let transformed = response.map(|item| item.to_uppercase());
    println!("Transformed items: {:?}", &transformed.items()[..3]);

    // The transformation applies to all future pages too
    println!("\nFetching next page (will also be transformed)...");
    let next = transformed.rest_of_pages().await?;
    if let Some(page) = next.first() {
        println!("Next page (transformed): {:?}", &page.items()[..3]);
    }

    Ok(())
}

/// Demonstrates including the current page in results.
async fn demo_with_current_page() -> Result<(), String> {
    println!("\n=== Demo 7: Including Current Page ===");

    let response = fetch_page(0, 10).await?;
    println!("Fetching ALL items (including current page)...");

    // This includes the current page's items plus all remaining
    let all_items = response.with_rest_of_items().await?;
    println!("Total items loaded: {}", all_items.len());
    println!("First item: {}", all_items[0]);
    println!("Last item: {}", all_items[all_items.len() - 1]);

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), String> {
    println!("MoosicBox Paging - Basic Pagination Example");
    println!("============================================");

    // Run all demos
    demo_basic_pages();
    demo_single_page().await?;
    demo_sequential_pagination().await?;
    demo_fetch_all_items().await?;
    demo_batch_pagination().await?;
    demo_data_transformation().await?;
    demo_with_current_page().await?;

    println!("\n=== All demos completed successfully! ===");

    Ok(())
}
