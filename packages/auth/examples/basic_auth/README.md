# Basic Authentication Example

This example demonstrates the fundamental authentication workflow in `moosicbox_auth`, including client registration, token management, and credential storage.

## Summary

This example shows how to use the `moosicbox_auth` package to obtain client credentials (client ID and access token), store them in a database, and optionally fetch signature tokens for request signing. It demonstrates both the high-level API and manual database operations for credential management.

## What This Example Demonstrates

- Initializing an in-memory database for credential storage
- Creating the required database schema for client access tokens
- Using `get_client_id_and_access_token()` to obtain or create client credentials
- Fetching signature tokens with `fetch_signature_token()`
- Manual credential storage and retrieval using the database API
- Proper error handling for authentication operations
- Integration with `ConfigDatabase` for dependency injection

## Prerequisites

- Basic understanding of Rust async/await syntax
- Familiarity with database operations (helpful but not required)
- Understanding of authentication tokens and client credentials

## Running the Example

Run the example from the repository root:

```bash
cargo run --manifest-path packages/auth/examples/basic_auth/Cargo.toml
```

Or from the example directory:

```bash
cd packages/auth/examples/basic_auth
cargo run
```

## Expected Output

The example will produce output similar to:

```
MoosicBox Authentication - Basic Usage Example
================================================

1. Setting up in-memory database...
   ✓ Database created

2. Creating client_access_tokens table...
   ✓ Table created

3. Initializing global database configuration...
   ✓ Database configuration initialized

4. Getting client ID and access token...
   Note: In this example, client registration will fail since we're
   not connecting to a real server. This demonstrates the workflow.
   ✗ Failed to get client credentials: [error details]
     This is expected in this example since we're not connecting
     to a real authentication server.

   Demonstrating manual credential storage...
   Creating sample credentials...
   ✓ Sample credentials stored:
     - Client ID: demo-client-123
     - Access Token: demo-token-abc

   Verifying credential retrieval...
   ✓ Retrieved credentials match:
     - Client ID: demo-client-123
     - Access Token: demo-token-abc


✓ Example completed!

Key Takeaways:
  1. Use get_client_id_and_access_token() to obtain credentials
  2. Credentials are automatically stored in the database
  3. Subsequent calls return existing credentials if valid
  4. Use fetch_signature_token() for request signing
  5. All functions work with ConfigDatabase for easy integration
```

## Code Walkthrough

### 1. Database Initialization

The example starts by creating an in-memory Turso database:

```rust
let db = TursoDatabase::new(":memory:").await?;
```

This creates a SQLite-based database in memory for demonstration purposes. In production, you would use a persistent database connection.

### 2. Creating the Schema

The `client_access_tokens` table is required for storing authentication credentials:

```rust
db.exec_raw(
    "CREATE TABLE client_access_tokens (
        client_id TEXT NOT NULL,
        token TEXT NOT NULL,
        expires INTEGER,
        updated INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
        PRIMARY KEY (client_id, token)
    )",
)
.await?;
```

This table schema includes:

- `client_id`: Unique identifier for the client
- `token`: The access token for authentication
- `expires`: Optional expiration timestamp
- `updated`: Last update time for credential rotation

### 3. Global Database Configuration

The `moosicbox_auth` package uses a global database configuration:

```rust
let db_boxed: Box<dyn switchy_database::Database> = Box::new(db);
config::init(Arc::new(db_boxed));
let config_db = config::ConfigDatabase::from(config::get().clone());
```

This pattern allows the authentication functions to access the database without explicit parameter passing in every call. The `ConfigDatabase` wrapper is used throughout the auth package for database operations.

### 4. Obtaining Client Credentials

The main authentication workflow uses `get_client_id_and_access_token()`:

```rust
let (client_id, access_token) = get_client_id_and_access_token(&config_db, auth_host).await?;
```

This function:

- Checks if credentials already exist in the database
- If found, returns the existing credentials
- If not found, generates a new client ID and registers with the auth server
- Stores the new credentials in the database
- Returns the client ID and access token

### 5. Fetching Signature Tokens

For operations requiring request signing, use `fetch_signature_token()`:

```rust
match fetch_signature_token(auth_host, &client_id, &access_token).await {
    Ok(Some(signature_token)) => {
        // Use the signature token for signing requests
    }
    Ok(None) => {
        // Server didn't provide a signature token
    }
    Err(e) => {
        // Handle authentication errors
    }
}
```

### 6. Manual Credential Management

The example also demonstrates direct database operations for credential storage:

```rust
db.upsert("client_access_tokens")
    .where_eq("client_id", sample_client_id)
    .where_eq("token", sample_token)
    .value("client_id", sample_client_id)
    .value("token", sample_token)
    .value("expires", DatabaseValue::Null)
    .execute_first(&**db)
    .await?;
```

This shows how credentials are stored internally, which can be useful for testing or custom authentication flows.

## Key Concepts

### Client Registration Flow

1. **Check Existing Credentials**: The auth package first queries the database for existing valid credentials
2. **Generate Client ID**: If no credentials exist, a new UUID-based client ID is generated
3. **Server Registration**: The client ID is sent to the authentication server for registration
4. **Store Credentials**: The server responds with an access token, which is stored in the database
5. **Return Credentials**: The client ID and access token are returned to the caller

### Token Storage

Credentials are stored in the `client_access_tokens` table with:

- **Automatic expiration checking**: Expired tokens are automatically excluded from queries
- **Update tracking**: The `updated` field tracks when credentials were last modified
- **Upsert semantics**: Multiple registration attempts with the same credentials won't create duplicates

### ConfigDatabase Pattern

The `ConfigDatabase` wrapper provides:

- **Dependency injection**: Works seamlessly with Actix-web extractors
- **Global access**: Configured once, accessible throughout the application
- **Type safety**: Implements `Deref` to `dyn Database` for transparent usage

### Error Handling

The `AuthError` enum covers various failure scenarios:

- `DatabaseFetch`: Database operation failures
- `Parse`: JSON parsing errors from server responses
- `Http`: Network or HTTP request failures
- `RegisterClient`: Server registration failures
- `Unauthorized`: Invalid or expired credentials

## Testing the Example

Since this example uses an in-memory database and simulated operations, it's safe to run multiple times without any cleanup. Each run creates a fresh database instance.

To modify the example:

1. **Change the auth host**: Update the `auth_host` variable to point to your authentication server
2. **Use a persistent database**: Replace `TursoDatabase::new(":memory:")` with a file path
3. **Add environment variables**: Set `TUNNEL_ACCESS_TOKEN` if registering with a real server
4. **Implement retry logic**: Add retry mechanisms for transient network failures

## Troubleshooting

### "TUNNEL_ACCESS_TOKEN not set" error

**Problem**: The `register_client()` function expects this environment variable.

**Solution**: In this example, registration fails before reaching that check. In production, set:

```bash
export TUNNEL_ACCESS_TOKEN="your-tunnel-token"
```

### Database connection errors

**Problem**: Database initialization or table creation fails.

**Solution**: Ensure you have write permissions if using a file-based database, or check that the in-memory database is properly initialized.

### HTTP request errors

**Problem**: Network requests to the auth server fail.

**Solution**: This is expected in the example since `https://example.com/api` is not a real auth server. In production, ensure:

- The auth server is reachable
- Network firewalls allow outbound HTTPS
- The server URL is correct

### Credential not persisting

**Problem**: Credentials don't persist between runs.

**Solution**: This is expected with `:memory:` databases. Use a file path for persistence:

```rust
let db = TursoDatabase::new("./auth.db").await?;
```

## Related Examples

- **Database Examples**: See `packages/database/examples/turso_basic/` for more database operations
- **HTTP Examples**: See `packages/http/examples/simple_get/` for HTTP client usage
- **Web Server Examples**: See `packages/web_server/examples/` for integrating auth with web servers

For more advanced authentication scenarios, refer to the `moosicbox_auth` package documentation and explore the API endpoints feature for magic token workflows.
