//! Example demonstrating automatic migration reversal

#[cfg(all(feature = "auto-reverse", feature = "code"))]
#[switchy_async::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use switchy_database::schema::auto_reversible::add_column;
    use switchy_database::schema::{Column, DataType, create_index, create_table};
    use switchy_schema::discovery::code::{CodeMigration, ReversibleCodeMigration};
    use switchy_schema::migration::Migration;

    println!("=== Auto-Reversible Migration Example ===\n");

    // Create an in-memory database for demonstration
    let db = switchy_database_connection::init_sqlite_sqlx(None).await?;

    // Example 1: Auto-reversible table creation
    println!("Example 1: CREATE TABLE → DROP TABLE");
    println!("---------------------------------------");

    let create_table_stmt = create_table("users")
        .column(Column {
            name: "id".to_string(),
            data_type: DataType::Int,
            nullable: false,
            auto_increment: true,
            default: None,
        })
        .column(Column {
            name: "name".to_string(),
            data_type: DataType::Text,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .primary_key("id");

    // Create a reversible migration - DOWN is automatically generated
    let migration: CodeMigration =
        ReversibleCodeMigration::new("001_create_users", create_table_stmt).into();

    println!("Running UP migration...");
    migration.up(&*db).await?;
    println!("✓ Table 'users' created");

    assert!(db.table_exists("users").await?);

    println!("Running DOWN migration (auto-generated)...");
    migration.down(&*db).await?;
    println!("✓ Table 'users' dropped\n");

    assert!(!db.table_exists("users").await?);

    // Example 2: Auto-reversible index creation
    println!("Example 2: CREATE INDEX → DROP INDEX");
    println!("--------------------------------------");

    // First, create a table for the index
    db.exec_raw("CREATE TABLE products (id INT PRIMARY KEY, name TEXT, price REAL)")
        .await?;

    let create_index_stmt = create_index("idx_products_name")
        .table("products")
        .columns(vec!["name"]);

    let migration: CodeMigration =
        ReversibleCodeMigration::new("002_add_products_index", create_index_stmt).into();

    println!("Running UP migration...");
    migration.up(&*db).await?;
    println!("✓ Index 'idx_products_name' created");

    println!("Running DOWN migration (auto-generated)...");
    migration.down(&*db).await?;
    println!("✓ Index 'idx_products_name' dropped\n");

    // Example 3: Auto-reversible column addition
    println!("Example 3: ADD COLUMN → DROP COLUMN");
    println!("------------------------------------");

    let add_column_op = add_column(
        "products",
        "description",
        DataType::Text,
        true, // nullable
        None, // default
    );

    let migration: CodeMigration =
        ReversibleCodeMigration::new("003_add_description", add_column_op).into();

    println!("Running UP migration...");
    migration.up(&*db).await?;
    println!("✓ Column 'description' added to 'products'");

    assert!(db.column_exists("products", "description").await?);

    println!("Running DOWN migration (auto-generated)...");
    migration.down(&*db).await?;
    println!("✓ Column 'description' removed from 'products'\n");

    assert!(!db.column_exists("products", "description").await?);

    println!("=== All examples completed successfully ===");

    Ok(())
}

#[cfg(not(all(feature = "auto-reverse", feature = "code")))]
fn main() {
    println!("This example requires the 'auto-reverse' feature");
    println!("Run with: cargo run --example auto_reverse --features auto-reverse");
}
