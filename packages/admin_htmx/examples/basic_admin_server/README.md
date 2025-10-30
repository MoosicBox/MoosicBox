# Basic Admin Server Example

A minimal web server demonstrating how to integrate the MoosicBox admin HTMX interface into an Actix-web application with profile and configuration database setup.

## Summary

This example shows the complete setup required to run a web server with the MoosicBox admin interface, including database initialization, profile management, and endpoint registration. It provides a working foundation that you can build upon for your own admin applications.

## What This Example Demonstrates

- Setting up configuration and profile databases for the admin interface
- Registering databases with the PROFILES manager
- Integrating admin HTMX endpoints into an Actix-web server
- Configuring middleware for logging HTTP requests
- Creating a minimal but functional admin web interface

## Prerequisites

Before running this example, you should be familiar with:

- Basic Rust programming and async/await syntax
- Actix-web framework concepts (applications, scopes, middleware)
- SQLite database basics
- HTTP request/response patterns

No special setup is required beyond having Rust installed.

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/admin_htmx/examples/basic_admin_server/Cargo.toml
```

The server will start on `http://127.0.0.1:8080`. You should see output similar to:

```
🎵 MoosicBox Admin Example Server
=================================

Setting up configuration database...
  ✓ Configuration database created at: config.db

Setting up default profile database...
  ✓ Profile 'default' created at: default_profile.db

Starting HTTP server...

✅ Server ready!
📍 Admin interface: http://127.0.0.1:8080/admin

Press Ctrl+C to stop the server
```

## Expected Output

When you navigate to `http://127.0.0.1:8080/admin` in your web browser, you'll see:

1. **MoosicBox Admin** - The main admin page header
2. **Profile Selector** - A dropdown to select or create profiles
3. **Server Info** - Information about the server identity
4. **Profiles Section** - Management interface for creating and deleting profiles
5. **Service Integration** - Sections for Tidal and Qobuz (if default features are enabled)
6. **Scan Management** - Library scanning controls (if scan feature is enabled)

The interface uses HTMX for dynamic updates, so interactions like creating profiles or changing selections will update the page without full reloads.

## Code Walkthrough

### Step 1: Initialize Logging

```rust
env_logger::Builder::from_env(
    env_logger::Env::default().default_filter_or("info")
).init();
```

Sets up logging to see HTTP requests and application events. The default level is `info`, which shows important events without being too verbose.

### Step 2: Create Configuration Database

```rust
let config_db = TursoDatabase::new(config_db_path).await?;
config::init(Arc::new(Box::new(config_db) as Box<dyn Database>));
```

The configuration database stores server-wide settings that apply to all profiles. We use `TursoDatabase`, which is a SQLite-compatible database backend. The `config::init` function registers it globally so endpoints can access it via Actix-web's dependency injection.

### Step 3: Set Up Profile Database

```rust
let profile_db = TursoDatabase::new(profile_db_path).await?;
PROFILES.add(
    default_profile.to_string(),
    Arc::new(Box::new(profile_db) as Box<dyn Database>),
);
```

Profiles enable multiple isolated library configurations. This example creates a single "default" profile using `TursoDatabase`. The `PROFILES` global manager stores all registered profiles and makes them available to request handlers via the profile name in HTTP headers.

### Step 4: Configure Actix-Web Server

```rust
HttpServer::new(move || {
    App::new()
        .wrap(middleware::Logger::default())
        .service(api::bind_services(web::scope("/admin")))
})
```

- `Logger::default()` - Logs each HTTP request
- `bind_services(web::scope("/admin"))` - Registers all admin endpoints under `/admin`

The `bind_services` function from `moosicbox_admin_htmx::api` automatically registers all admin-related endpoints including server info, profile management, and optional service integrations. The config database is accessed via the global `config::init` setup, so we don't need to pass it as `app_data`.

## Key Concepts

### Database Architecture

MoosicBox uses two types of databases:

1. **Config Database** - Stores server-wide configuration and settings
2. **Profile Databases** - Each profile has its own database for library data, allowing multiple isolated music libraries

### Profile Management

The `PROFILES` global manager (from `switchy_database::profiles`) maintains a registry of all profile databases. When endpoints receive requests with a `moosicbox-profile` header, they automatically retrieve the corresponding database using Actix-web's `FromRequest` trait implementation.

### HTMX Integration

The admin interface uses HTMX for dynamic updates:

- Responses contain HTML fragments, not JSON
- HTMX attributes on elements trigger HTTP requests
- Server responses update specific parts of the page
- No custom JavaScript required for basic interactions

### Dependency Injection

Actix-web's `app_data` and `FromRequest` traits enable automatic dependency injection. Endpoints can request `ConfigDatabase` or `LibraryDatabase` as parameters, and Actix-web automatically extracts them from application data or request headers.

## Testing the Example

Once the server is running, try these interactions:

1. **View the Admin Interface**
    - Navigate to `http://127.0.0.1:8080/admin`
    - Verify you see the admin dashboard with sections for profiles, server info, and services

2. **Create a New Profile**
    - Enter a profile name in the "New Profile" form
    - Click "Create"
    - The profile list should update dynamically to show the new profile

3. **Switch Profiles**
    - Use the dropdown at the top to select different profiles
    - The page should update via HTMX to show the selected profile's data

4. **Check Server Logs**
    - Watch the terminal where the server is running
    - You should see HTTP request logs showing GET and POST requests
    - Example: `[INFO] actix_web::middleware::logger - 127.0.0.1 "GET /admin HTTP/1.1" 200`

## Troubleshooting

### Port Already in Use

If you see an error about port 8080 being in use:

- Change the `server_addr` in `main.rs` to a different port (e.g., `"127.0.0.1:8081"`)
- Or stop the process using port 8080

### Database File Permissions

If you see errors about database file access:

- Ensure the current directory is writable
- Check that `config.db` and `default_profile.db` are not locked by another process
- Delete the `.db` files and restart to create fresh databases

### Missing Features

If certain admin sections don't appear:

- The example uses default features which include `scan`, `tidal`, and `qobuz`
- To disable features, modify the dependency in `Cargo.toml`:
    ```toml
    moosicbox_admin_htmx = { workspace = true, default-features = false, features = ["api"] }
    ```

### Browser Not Showing HTMX Updates

If the interface doesn't update dynamically:

- Check the browser console for JavaScript errors
- Verify that HTMX script loaded correctly (look for `htmx.org` in Network tab)
- Try a hard refresh (Ctrl+Shift+R or Cmd+Shift+R)

## Related Examples

- **Web Server Examples** - See `packages/web_server/examples/` for more advanced server setups
- **Database Examples** - See `packages/database/examples/` for database usage patterns
- **Hyperchad Examples** - See `packages/hyperchad/examples/` for HTMX component patterns
