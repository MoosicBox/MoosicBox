#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Query Builder Basic Example
//!
//! This example demonstrates the core query builder API of `switchy_database`,
//! which provides database-agnostic operations across `SQLite`, `PostgreSQL`, and `MySQL`.

use switchy_database::{
    DatabaseValue,
    query::{FilterableQuery as _, SortDirection},
    schema::{Column, DataType, create_table},
};

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Query Builder API - Basic Usage Example");
    println!("========================================\n");

    // Initialize an in-memory SQLite database
    // Note: You can also use file-based SQLite or other backends (PostgreSQL, MySQL)
    println!("Creating in-memory SQLite database...");
    let db = switchy_database_connection::init_sqlite_sqlx(None).await?;
    println!("✓ Database created\n");

    // === SCHEMA CREATION ===
    // Use the schema API to create tables in a database-agnostic way
    println!("Creating 'products' table using schema API...");
    create_table("products")
        .column(Column {
            name: "id".to_string(),
            nullable: false,
            auto_increment: true,
            data_type: DataType::BigInt,
            default: None,
        })
        .column(Column {
            name: "name".to_string(),
            nullable: false,
            auto_increment: false,
            data_type: DataType::VarChar(100),
            default: None,
        })
        .column(Column {
            name: "price".to_string(),
            nullable: false,
            auto_increment: false,
            data_type: DataType::Real,
            default: None,
        })
        .column(Column {
            name: "stock".to_string(),
            nullable: false,
            auto_increment: false,
            data_type: DataType::Int,
            default: Some(DatabaseValue::Int32(0)),
        })
        .primary_key("id")
        .execute(&*db)
        .await?;
    println!("✓ Table created\n");

    // === INSERT OPERATIONS ===
    println!("Inserting products using query builder...");

    // Insert returns the created row with the auto-generated ID
    let laptop = db
        .insert("products")
        .value("name", "Laptop")
        .value("price", 999.99)
        .value("stock", 10)
        .execute(&*db)
        .await?;

    let laptop_id = laptop.id().and_then(|v| v.as_i64()).unwrap();
    println!("  * Inserted Laptop with ID: {laptop_id}");

    let mouse = db
        .insert("products")
        .value("name", "Mouse")
        .value("price", 25.50)
        .value("stock", 50)
        .execute(&*db)
        .await?;

    let mouse_id = mouse.id().and_then(|v| v.as_i64()).unwrap();
    println!("  * Inserted Mouse with ID: {mouse_id}");

    let keyboard = db
        .insert("products")
        .value("name", "Keyboard")
        .value("price", 75.00)
        .value("stock", 30)
        .execute(&*db)
        .await?;

    let keyboard_id = keyboard.id().and_then(|v| v.as_i64()).unwrap();
    println!("  * Inserted Keyboard with ID: {keyboard_id}");
    println!();

    // === SELECT OPERATIONS ===
    println!("Querying products...");

    // Select all products
    let all_products = db.select("products").execute(&*db).await?;
    println!("  Found {} products:", all_products.len());
    for product in &all_products {
        let id_val = product.get("id").unwrap();
        let id = id_val.as_i64().unwrap();
        let name_val = product.get("name").unwrap();
        let name = name_val.as_str().unwrap();
        let price_val = product.get("price").unwrap();
        let price = price_val.as_f64().unwrap();
        let stock_val = product.get("stock").unwrap();
        let stock = stock_val.as_i32().unwrap();
        println!("    - ID {id}: {name} - ${price:.2} (Stock: {stock})");
    }
    println!();

    // Select specific columns with WHERE clause
    println!("Querying products with price > $50...");
    let expensive_products = db
        .select("products")
        .columns(&["name", "price"])
        .where_gt("price", 50.0)
        .execute(&*db)
        .await?;

    println!("  Found {} expensive products:", expensive_products.len());
    for product in &expensive_products {
        let name_val = product.get("name").unwrap();
        let name = name_val.as_str().unwrap();
        let price_val = product.get("price").unwrap();
        let price = price_val.as_f64().unwrap();
        println!("    - {name}: ${price:.2}");
    }
    println!();

    // Select with ORDER BY
    println!("Querying products ordered by price (descending)...");
    let sorted_products = db
        .select("products")
        .sort("price", SortDirection::Desc)
        .execute(&*db)
        .await?;

    println!("  Products by price:");
    for product in &sorted_products {
        let name_val = product.get("name").unwrap();
        let name = name_val.as_str().unwrap();
        let price_val = product.get("price").unwrap();
        let price = price_val.as_f64().unwrap();
        println!("    - {name}: ${price:.2}");
    }
    println!();

    // === UPDATE OPERATIONS ===
    println!("Updating stock for Laptop...");
    let updated_rows = db
        .update("products")
        .value("stock", 15) // Increase stock from 10 to 15
        .where_eq("id", laptop_id)
        .execute(&*db)
        .await?;

    println!("  * Updated {} row(s)", updated_rows.len());

    // Verify the update
    let updated_laptop = db
        .select("products")
        .where_eq("id", laptop_id)
        .execute_first(&*db)
        .await?
        .expect("Laptop should exist");

    let new_stock = updated_laptop
        .get("stock")
        .and_then(|v| v.as_i32())
        .unwrap();
    println!("  * New stock for Laptop: {new_stock}");
    println!();

    // === UPSERT OPERATIONS ===
    println!("Performing upsert (insert or update)...");

    // Try to insert a new monitor, or update if it already exists
    let _monitor = db
        .upsert("products")
        .value("name", "Monitor")
        .value("price", 350.00)
        .value("stock", 20)
        .unique(&["name"]) // Use name as the unique constraint
        .execute(&*db)
        .await?;

    println!("  * Upserted Monitor");

    // Upsert again with same name - should update instead of insert
    let _monitor_updated = db
        .upsert("products")
        .value("name", "Monitor")
        .value("price", 325.00) // Updated price
        .value("stock", 25) // Updated stock
        .unique(&["name"])
        .execute(&*db)
        .await?;

    println!("  * Updated existing Monitor with new price");
    println!();

    // === DELETE OPERATIONS ===
    println!("Deleting Mouse product...");
    let deleted_rows = db
        .delete("products")
        .where_eq("id", mouse_id)
        .execute(&*db)
        .await?;

    println!("  * Deleted {} row(s)", deleted_rows.len());
    println!();

    // === TRANSACTIONS ===
    println!("Demonstrating transaction with rollback...");

    // Begin a transaction
    let tx = db.begin_transaction().await?;

    // Perform operations within the transaction
    tx.update("products")
        .value("price", 0.0) // Set all prices to 0
        .execute(&*tx)
        .await?;

    println!("  * Set all prices to 0 within transaction");

    // Check prices within transaction
    let zero_price_products = tx.select("products").execute(&*tx).await?;
    println!("  * Products in transaction:");
    for product in &zero_price_products {
        let name_val = product.get("name").unwrap();
        let name = name_val.as_str().unwrap();
        let price_val = product.get("price").unwrap();
        let price = price_val.as_f64().unwrap();
        println!("    - {name}: ${price:.2}");
    }

    // Rollback the transaction - prices should revert
    tx.rollback().await?;
    println!("  * Transaction rolled back");
    println!();

    // Verify prices are unchanged outside the transaction
    println!("Verifying prices after rollback...");
    let final_products = db.select("products").execute(&*db).await?;
    println!("  * Products after rollback:");
    for product in &final_products {
        let name_val = product.get("name").unwrap();
        let name = name_val.as_str().unwrap();
        let price_val = product.get("price").unwrap();
        let price = price_val.as_f64().unwrap();
        println!("    - {name}: ${price:.2}");
    }
    println!();

    // === TRANSACTION WITH COMMIT ===
    println!("Demonstrating transaction with commit...");

    let tx = db.begin_transaction().await?;

    // Apply a 10% discount to all products
    let products_to_discount = tx.select("products").execute(&*tx).await?;

    for product in products_to_discount {
        let id_val = product.get("id").unwrap();
        let id = id_val.as_i64().unwrap();
        let price_val = product.get("price").unwrap();
        let current_price = price_val.as_f64().unwrap();
        let discounted_price = current_price * 0.9; // 10% off

        tx.update("products")
            .value("price", discounted_price)
            .where_eq("id", id)
            .execute(&*tx)
            .await?;
    }

    println!("  * Applied 10% discount to all products");

    // Commit the transaction
    tx.commit().await?;
    println!("  * Transaction committed");
    println!();

    // Show final state
    println!("Final product listing:");
    let final_products = db.select("products").execute(&*db).await?;
    for product in &final_products {
        let name_val = product.get("name").unwrap();
        let name = name_val.as_str().unwrap();
        let price_val = product.get("price").unwrap();
        let price = price_val.as_f64().unwrap();
        let stock_val = product.get("stock").unwrap();
        let stock = stock_val.as_i32().unwrap();
        println!("  * {name}: ${price:.2} (Stock: {stock})");
    }

    println!("\n✓ Example completed successfully!");

    Ok(())
}
