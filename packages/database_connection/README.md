# Database Connection

Database connection initialization and management with support for multiple database backends.

## Overview

The Database Connection package provides:

- **Multi-backend Support**: PostgreSQL and SQLite database connections
- **Feature-gated Backends**: Choose specific database implementations
- **TLS Support**: Native TLS and OpenSSL options for PostgreSQL
- **Connection Management**: Unified connection initialization interface
- **Credential Handling**: Secure credential management
- **Simulator Mode**: Mock database connections for testing

## Features

### Database Backends
- **PostgreSQL**: Raw tokio-postgres and SQLx implementations
- **SQLite**: Rusqlite and SQLx implementations
- **Simulator**: Mock database for testing and development

### PostgreSQL Options
- **Raw Implementation**: Direct tokio-postgres with connection pooling
- **SQLx Implementation**: SQLx-based PostgreSQL connections
- **TLS Support**: Native TLS, OpenSSL, or no TLS options
- **Connection Pooling**: Managed connection pools

### SQLite Options
- **Rusqlite**: Synchronous SQLite with async wrapper
- **SQLx**: Async SQLite via SQLx
- **In-memory**: Support for in-memory databases
- **File-based**: Persistent SQLite database files

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
database_connection = { path = "../database_connection" }

# PostgreSQL with native TLS
database_connection = {
    path = "../database_connection",
    features = ["postgres-raw", "postgres-native-tls"]
}

# SQLite with rusqlite
database_connection = {
    path = "../database_connection",
    features = ["sqlite", "sqlite-rusqlite"]
}

# Multiple backends
database_connection = {
    path = "../database_connection",
    features = [
        "postgres-sqlx",
        "sqlite-sqlx",
        "sqlite"
    ]
}
```

## Usage

### Basic Database Initialization

```rust
use database_connection::{init, Credentials};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // PostgreSQL connection
    let pg_creds = Some(Credentials::new(
        "localhost".to_string(),
        "mydb".to_string(),
        "user".to_string(),
        Some("password".to_string()),
    ));

    let db = init(None, pg_creds).await?;

    // Use database...
    Ok(())
}
```

### SQLite Database

```rust
use database_connection::init;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // File-based SQLite
    let db_path = Path::new("./database.db");
    let db = init(Some(db_path), None).await?;

    // In-memory SQLite
    let db = init(None, None).await?;

    // Use database...
    Ok(())
}
```

### Credential Management

```rust
use database_connection::Credentials;

// Create credentials manually
let creds = Credentials::new(
    "database.example.com".to_string(),  // host
    "production_db".to_string(),         // database name
    "app_user".to_string(),              // username
    Some("secure_password".to_string()), // password (optional)
);

// Or parse from connection string
let creds = Credentials::from_url("postgres://user:pass@localhost:5432/mydb")?;

// Use with database initialization
let db = database_connection::init(None, Some(creds)).await?;
```

### Environment Variables

The package supports multiple ways to provide credentials:

```bash
# Option 1: Connection string (recommended)
export DATABASE_URL="postgres://user:password@localhost:5432/mydb"

# Option 2: Individual environment variables
export DB_HOST="localhost"
export DB_NAME="mydb"
export DB_USER="user"
export DB_PASSWORD="password"

# Option 3: AWS SSM Parameters (requires 'creds' feature)
export SSM_DB_HOST_PARAM_NAME="myapp_db_host"
export SSM_DB_NAME_PARAM_NAME="myapp_db_name"
export SSM_DB_USER_PARAM_NAME="myapp_db_user"
export SSM_DB_PASSWORD_PARAM_NAME="myapp_db_password"
```

### AWS Credentials (Optional)

To use AWS SSM parameter store for credentials, enable the `creds` feature:

```toml
[dependencies]
database_connection = {
    path = "../database_connection",
    features = ["postgres", "creds"]
}
```

```rust
use database_connection::creds::get_db_creds;

// Automatically fetches from DATABASE_URL, env vars, or AWS SSM
let creds = get_db_creds().await?;
let db = database_connection::init(None, Some(creds)).await?;
```

### PostgreSQL with TLS

```rust
// Feature: postgres-raw + postgres-native-tls
use database_connection::{init_postgres_raw_native_tls, Credentials};

let creds = Credentials::new(
    "secure-db.example.com".to_string(),
    "mydb".to_string(),
    "user".to_string(),
    Some("password".to_string()),
);

let db = init_postgres_raw_native_tls(creds).await?;
```

### PostgreSQL with OpenSSL

```rust
// Feature: postgres-raw + postgres-openssl
use database_connection::{init_postgres_raw_openssl, Credentials};

let creds = Credentials::new(
    "ssl-db.example.com".to_string(),
    "mydb".to_string(),
    "user".to_string(),
    Some("password".to_string()),
);

let db = init_postgres_raw_openssl(creds).await?;
```

### PostgreSQL without TLS

```rust
// Feature: postgres-raw
use database_connection::{init_postgres_raw_no_tls, Credentials};

let creds = Credentials::new(
    "local-db".to_string(),
    "mydb".to_string(),
    "user".to_string(),
    Some("password".to_string()),
);

let db = init_postgres_raw_no_tls(creds).await?;
```

### PostgreSQL with SQLx

```rust
// Feature: postgres-sqlx
use database_connection::{init_postgres_sqlx, Credentials};

let creds = Credentials::new(
    "localhost".to_string(),
    "mydb".to_string(),
    "user".to_string(),
    Some("password".to_string()),
);

let db = init_postgres_sqlx(creds).await?;
```

### SQLite with Rusqlite

```rust
// Feature: sqlite-rusqlite
use database_connection::init_sqlite_rusqlite;
use std::path::Path;

// File-based database
let db_path = Path::new("./app.db");
let db = init_sqlite_rusqlite(Some(db_path))?;

// In-memory database
let db = init_sqlite_rusqlite(None)?;
```

### SQLite with SQLx

```rust
// Feature: sqlite-sqlx
use database_connection::init_sqlite_sqlx;
use std::path::Path;

// File-based database
let db_path = Path::new("./app.db");
let db = init_sqlite_sqlx(Some(db_path)).await?;

// In-memory database
let db = init_sqlite_sqlx(None).await?;
```

### Non-SQLite Initialization

```rust
use database_connection::{init_default_non_sqlite, Credentials};

// Initialize any non-SQLite database based on features
let creds = Some(Credentials::new(
    "localhost".to_string(),
    "mydb".to_string(),
    "user".to_string(),
    Some("password".to_string()),
));

let db = init_default_non_sqlite(creds).await?;
```

### Simulator Mode

```rust
// Feature: simulator
use database_connection::init;

// Always returns a mock database for testing
let db = init(None, None).await?;

// Use mock database in tests
#[tokio::test]
async fn test_database_operations() {
    let db = init(None, None).await.unwrap();
    // Test database operations without real database
}
```

## Error Handling

```rust
use database_connection::{init, InitDbError, Credentials};

match init(None, None).await {
    Ok(db) => {
        // Use database
    }
    Err(InitDbError::CredentialsRequired) => {
        eprintln!("Database credentials are required");
    }
    Err(InitDbError::InitSqlite(e)) => {
        eprintln!("SQLite initialization error: {}", e);
    }
    Err(InitDbError::InitPostgres(e)) => {
        eprintln!("PostgreSQL initialization error: {}", e);
    }
    Err(InitDbError::Database(e)) => {
        eprintln!("Database error: {}", e);
    }
}
```

## Feature Flags

### PostgreSQL Features
- **`postgres-raw`**: Raw tokio-postgres implementation
- **`postgres-sqlx`**: SQLx PostgreSQL implementation
- **`postgres-native-tls`**: Native TLS support for PostgreSQL
- **`postgres-openssl`**: OpenSSL support for PostgreSQL

### SQLite Features
- **`sqlite`**: Enable SQLite support
- **`sqlite-rusqlite`**: Rusqlite implementation
- **`sqlite-sqlx`**: SQLx SQLite implementation

### Other Features
- **`simulator`**: Mock database for testing
- **`creds`**: AWS SSM credential management (optional)

## Connection Strings

### Supported URL Formats

```bash
# PostgreSQL
postgres://user:password@host:port/database
postgresql://user:password@host:port/database

# MySQL
mysql://user:password@host:port/database

# Examples
DATABASE_URL="postgres://myuser:mypass@localhost:5432/mydb"
DATABASE_URL="mysql://root:secret@127.0.0.1:3306/app_db"
```

### Individual Environment Variables

```bash
# Individual variables
DB_HOST="localhost"
DB_NAME="mydb"
DB_USER="username"
DB_PASSWORD="password"
```

## Dependencies

- **Switchy Database**: Generic database trait abstraction
- **Tokio Postgres**: PostgreSQL async driver (optional)
- **SQLx**: Multi-database async driver (optional)
- **Rusqlite**: SQLite synchronous driver (optional)
- **Native TLS**: TLS implementation (optional)
- **OpenSSL**: Alternative TLS implementation (optional)
- **Thiserror**: Error handling

## Use Cases

- **Web Applications**: Database connections for web servers
- **Microservices**: Service-specific database connections
- **CLI Tools**: Command-line database utilities
- **Testing**: Mock database connections for unit tests
- **Data Migration**: Database migration and setup tools
- **Multi-tenant Applications**: Dynamic database connections
