# Basic Usage Example

This example demonstrates the core path management functionality of the `moosicbox_config` package.

## Summary

This example shows how to use `moosicbox_config` to manage configuration directory paths for MoosicBox applications, including getting and creating configuration directories, working with profiles, and managing cache directories.

## What This Example Demonstrates

- Getting configuration directory paths for different contexts
- Setting a custom root directory for configuration
- Working with app-specific configuration directories (App, Server, Local)
- Creating configuration directories automatically
- Managing profile-specific configuration directories
- Working with cache directories
- Understanding the MoosicBox configuration directory structure

## Prerequisites

- Basic understanding of Rust
- Familiarity with filesystem paths and directory structures
- No special dependencies or setup required

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/config/examples/basic_usage/Cargo.toml
```

Or from within the example directory:

```bash
cd packages/config/examples/basic_usage
cargo run
```

## Expected Output

The example will print output showing:

1. The default configuration directory path (typically `~/.local/moosicbox`)
2. A custom temporary root directory being set
3. App-specific configuration directory paths for App, Server, and Local types
4. Creation and verification of the config directory
5. Profile directory paths for different profiles (default, production, development)
6. Creation of an example profile directory
7. Cache directory path and creation
8. A visual representation of the directory structure
9. Examples of complete configuration file paths

Example output:

```
=== MoosicBox Config - Basic Usage Example ===

1. Default Configuration Directory:
   Config directory: /home/user/.local/moosicbox

2. Setting Custom Root Directory:
   Setting root to: /tmp/moosicbox_example
   New config directory: /tmp/moosicbox_example

3. App-Specific Configuration Directories:
   App: /tmp/moosicbox_example/app
   Server: /tmp/moosicbox_example/server
   Local: /tmp/moosicbox_example/local

4. Creating Configuration Directory:
   Created/verified: /tmp/moosicbox_example
   Exists: true

5. Profile Directories:
   default: /tmp/moosicbox_example/server/profiles/default
   production: /tmp/moosicbox_example/server/profiles/production
   development: /tmp/moosicbox_example/server/profiles/development

6. Creating Profile Directory:
   Created/verified: /tmp/moosicbox_example/server/profiles/example_profile
   Exists: true
   Config file would be at: /tmp/moosicbox_example/server/profiles/example_profile/config.json5

7. Cache Directory:
   Cache directory: /tmp/moosicbox_example/cache
   Created/verified: /tmp/moosicbox_example/cache
   Exists: true

8. Example Directory Structure:
   ~/.local/moosicbox/                (or custom root)
   ├── server/
   │   ├── config.json5              (global server config)
   │   └── profiles/
   │       ├── default/
   │       │   └── config.json5      (profile-specific config)
   │       └── production/
   │           └── config.json5
   ├── app/
   │   └── config.json5
   └── cache/                         (shared cache directory)

=== Example Complete ===
```

## Code Walkthrough

### Setting Up the Root Directory

The example demonstrates how to override the default configuration root directory:

```rust
let temp_root = std::env::temp_dir().join("moosicbox_example");
set_root_dir(temp_root.clone());
```

This is useful for testing or when you want to store configuration in a non-standard location.

### Getting Configuration Paths

The package provides several functions to get configuration directory paths:

```rust
// Get the base config directory
let config_dir = get_config_dir_path();

// Get app-specific config directories
let server_dir = get_app_config_dir_path(AppType::Server);
let app_dir = get_app_config_dir_path(AppType::App);

// Get profile-specific directories
let profile_dir = get_profile_dir_path(AppType::Server, "production");

// Get the cache directory
let cache_dir = get_cache_dir_path();
```

These functions return `Option<PathBuf>`, returning `None` if the home directory cannot be determined.

### Creating Directories

The `make_*` functions not only return paths but also create the directories if they don't exist:

```rust
// Create the config directory
let config_dir = make_config_dir_path();

// Create a profile directory
let profile_dir = make_profile_dir_path(AppType::Server, "example_profile");

// Create the cache directory
let cache_dir = make_cache_dir_path();
```

These functions return `None` if the directory cannot be created or the path cannot be determined.

### Working with App Types

The `AppType` enum distinguishes between different MoosicBox application contexts:

```rust
pub enum AppType {
    App,    // Mobile or desktop application
    Server, // Server application
    Local,  // Local development instance
}
```

Each app type has its own configuration directory, allowing different configurations for different deployment contexts.

## Key Concepts

### Configuration Directory Hierarchy

MoosicBox uses a hierarchical directory structure:

- **Root**: `~/.local/moosicbox` (configurable via `set_root_dir`)
- **App-specific**: `{root}/{app_type}/` - Configuration for a specific application type
- **Profiles**: `{root}/{app_type}/profiles/{profile_name}/` - Per-user or per-environment configuration
- **Cache**: `{root}/cache/` - Shared cache directory for temporary data

### Global vs Profile Configuration

- **Global configuration** (`{app_type}/config.json5`): Settings that apply to all profiles
- **Profile configuration** (`{app_type}/profiles/{name}/config.json5`): Settings specific to a profile (user, environment, etc.)

### Path vs Make Functions

- `get_*` functions: Return the path without creating directories
- `make_*` functions: Return the path and create directories if they don't exist

## Testing the Example

After running the example:

1. Check that the temporary directory was created and cleaned up
2. Try modifying the code to use the default root directory (remove the `set_root_dir` call)
3. Create actual configuration files in the directories and verify the structure
4. Experiment with different profile names

## Troubleshooting

### "Could not determine config directory"

This typically means the home directory cannot be determined. Ensure your system has a valid `HOME` environment variable (Linux/macOS) or `USERPROFILE` (Windows).

### Permission Denied

If you see permission errors, ensure you have write access to the directory where configurations are being created. When using the default root (`~/.local/moosicbox`), you should have write permissions to your home directory.

### Directory Already Exists

This is not an error. The `make_*` functions will succeed if the directory already exists, making them safe to call multiple times.

## Related Examples

This is the only example for `moosicbox_config` currently. For more advanced usage:

- See the package README for examples of loading configuration files with the `file` feature
- See the package documentation for examples of database-backed configuration with the `db` feature
- See the package README for examples of the REST API with the `api` feature
