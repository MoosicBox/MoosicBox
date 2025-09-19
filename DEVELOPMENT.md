# MoosicBox Development Environment

This project uses **Nix Flakes** for reproducible development environments across Linux and macOS, with specialized shells for different GUI backends and components.

## Quick Start

### Main Development Environment

```bash
# Full development environment (all GUI backends)
nix develop

# Minimal CI environment
nix develop .#ci
```

### Component-Specific Environments

```bash
# Server components
nix develop .#server           # Main server with SQLite, libclang
nix develop .#tunnel-server    # Tunnel server (minimal)

# GTK-based applications
nix develop .#gtk-marketing-site    # Marketing site

# Tauri applications (all use GTK/WebKit base)
nix develop .#tauri-solidjs              # Tauri with SolidJS/Astro frontend
nix develop .#tauri-hyperchad-fltk       # Tauri with HyperChad FLTK backend
nix develop .#tauri-hyperchad-egui       # Tauri with HyperChad Egui backend
nix develop .#tauri-solidjs-bundled      # Tauri SolidJS with embedded server
nix develop .#tauri-hyperchad-fltk-bundled  # Tauri HyperChad FLTK with embedded server
nix develop .#tauri-hyperchad-egui-bundled  # Tauri HyperChad Egui with embedded server
nix develop .#tauri-full                 # Full Tauri development (all backends)

# FLTK-based applications
nix develop .#fltk-renderer        # FLTK renderer
nix develop .#fltk-hyperchad       # Hyperchad FLTK interface

# Egui-based applications
nix develop .#egui-native          # Native app with Vulkan/WGPU
nix develop .#egui-player          # Egui music player

# Android development
nix develop .#android              # Android SDK, NDK, Java, Gradle
```

### List All Available Environments

```bash
nix flake show
```

## GUI Backend-Specific Dependencies

### GTK Backend (`gtk-*`)

- **Graphics**: GTK3, WebKit, GStreamer, Cairo, Pango
- **Display**: X11, Wayland support
- **Audio**: ALSA, PulseAudio, GStreamer plugins
- **Use cases**: Web-based UIs, traditional Linux desktop apps

### FLTK Backend (`fltk-*`)

- **Graphics**: FLTK, OpenGL, Mesa, Cairo
- **Display**: X11 (primarily)
- **Audio**: ALSA, PortAudio
- **Use cases**: Lightweight native GUIs, cross-platform compatibility

### Egui Backend (`egui-*`)

- **Graphics**: Vulkan, WGPU, OpenGL
- **Display**: X11, Wayland (Linux), Metal (macOS)
- **Audio**: ALSA, PortAudio, PipeWire
- **Use cases**: Modern GPU-accelerated interfaces, immediate mode GUIs

### Android Development (`android`)

- **SDK**: Android SDK with API levels 33, 34
- **NDK**: Android NDK for native development
- **Tools**: ADB, Gradle, platform tools
- **Java**: OpenJDK 17 for Android builds
- **Rust targets**: Automatically installs Android Rust targets
- **Use cases**: Building Android apps with Tauri v2

## What's Included

### Core Tools (All Environments)

- **Rust toolchain** (stable, with rust-analyzer, clippy, rustfmt)
- **Build tools** (cmake, ninja, pkg-config, gcc)
- **Development tools** (cargo-watch, cargo-edit, cargo-audit)

### Backend-Specific Tools

- **GTK environments**: WebKit, GStreamer, desktop integration
- **FLTK environments**: FLTK toolkit, OpenGL, minimal dependencies
- **Egui environments**: Vulkan validation layers, GPU drivers, WGPU

### Platform Support

- **Linux**: Full support for all backends
- **macOS**: Automatic clang setup, Metal framework for Egui

## Development Workflow

### Working on Specific Components

```bash
# Work on server code
nix develop .#server --command cargo build -p moosicbox_server

# Work on FLTK interface
nix develop .#fltk-renderer --command cargo run --bin fltk_renderer

# Work on Tauri app with SolidJS
nix develop .#tauri-solidjs --command cargo tauri dev

# Work on Tauri app with HyperChad
nix develop .#tauri-hyperchad-fltk --command cargo run --features moosicbox-app-native

# Work on Android development
nix develop .#android --command bash -c 'cargo tauri android init && cargo tauri android build'
```

### Using direnv (Automatic Loading)

Create `.envrc` files in specific packages:

```bash
# packages/server/.envrc
use flake ../..#server

# packages/app/tauri/.envrc
use flake ../../..#tauri-solidjs
```

Then:

```bash
direnv allow
cd packages/server  # Environment loads automatically
```

## Environment Details

### Rust Toolchain

- **Version**: Latest stable via rust-overlay
- **Extensions**: rust-src, rust-analyzer, clippy, rustfmt
- **Consistency**: Same version across all environments

### Platform Detection

- **Linux**: Automatic GUI backend library configuration
- **macOS**: Clang compiler, Metal/OpenGL framework setup
- **Library paths**: Automatically configured per backend

### Environment Variables

- **GTK**: `GDK_BACKEND=x11,wayland`
- **Egui**: `VK_ICD_FILENAMES` for Vulkan
- **macOS**: `CC`, `CXX` set to Nix clang

## Building and Testing

```bash
# Build specific components
nix develop .#server --command cargo build -p moosicbox_server
nix develop .#fltk-renderer --command cargo build -p hyperchad

# Run tests with appropriate environment
nix develop .#tauri-solidjs --command cargo test -p moosicbox_app

# Format code (any environment)
nix develop .#ci --command cargo fmt

# Lint code (any environment)
nix develop .#ci --command cargo clippy
```

## Database Backend Differences

MoosicBox supports multiple database backends with some behavior differences to be aware of:

### Transaction Error Handling

| Database   | Savepoint After Error | Recovery Method |
|------------|----------------------|-----------------|
| SQLite     | ✅ Supported         | Create savepoint after error |
| MySQL      | ✅ Supported         | Create savepoint after error |
| PostgreSQL | ❌ Not Supported     | Must use savepoint BEFORE error, then rollback |

### PostgreSQL Transaction Semantics

PostgreSQL enforces strict transaction semantics. When any error occurs within a transaction, the entire transaction enters an **ABORTED** state:

- **No new operations allowed** (including savepoint creation)
- **Only ROLLBACK or ROLLBACK TO SAVEPOINT commands work**
- **Error message**: "current transaction is aborted, commands ignored until end of transaction block"

#### Correct PostgreSQL Error Recovery Pattern

```rust
// Create savepoint BEFORE risky operation
let sp = tx.savepoint("safety").await?;

match risky_operation(&tx).await {
    Ok(result) => {
        sp.release().await?;  // Success - release savepoint
        Ok(result)
    }
    Err(error) => {
        sp.rollback().await?; // Must rollback to continue transaction
        // Transaction is now viable again
        handle_error(error)
    }
}
```

#### SQLite/MySQL Pattern (for comparison)

```rust
// Can create savepoint AFTER error occurs
let _error = risky_operation(&tx).await.unwrap_err();
let sp = tx.savepoint("recovery").await?; // Works in SQLite/MySQL
// Continue with recovery operations...
```

### Testing Implications

Some tests are backend-specific due to these differences:
- `test_savepoint_after_failed_operation` - Excluded from PostgreSQL test suites
- Use `nix develop --command cargo test -p switchy_database --test savepoint_integration` to run backend-specific tests

## Package Building

Build MoosicBox as a Nix package:

```bash
# Build the complete package
nix build

# Run the built package
nix run
```

## Maintenance

### Updating Dependencies

```bash
# Update all flake inputs
nix flake update

# Update specific input
nix flake update rust-overlay
```

### Cache Management

```bash
# Clean Nix store
nix store gc

# Clean build outputs
cargo clean
```

## Troubleshooting

### GUI Backend Issues

- **GTK not loading**: Check `GDK_BACKEND` environment variable
- **FLTK compilation errors**: Ensure OpenGL libraries are available
- **Egui/Vulkan errors**: Verify GPU drivers and Vulkan support

### Audio Issues

- **Linux**: Check ALSA/PulseAudio configuration
- **macOS**: Verify PortAudio/CoreAudio access

### Missing Dependencies

Add missing packages to the appropriate backend section in `flake.nix`:

- `gtkPackages` for GTK backend dependencies
- `fltkPackages` for FLTK backend dependencies
- `eguiPackages` for Egui backend dependencies

## Architecture

The development environment is organized around GUI backends to avoid dependency conflicts:

```
MoosicBox Project
├── Server Components (headless)
│   ├── server (SQLite, libclang, bindgen)
│   └── tunnel-server (minimal)
├── GTK Backend Apps
│   └── marketing-site (WebKit)
├── Tauri Apps (GTK/WebKit base + specific backends)
│   ├── tauri-solidjs (SolidJS/Astro frontend)
│   ├── tauri-hyperchad-fltk (FLTK native backend)
│   ├── tauri-hyperchad-egui (Egui native backend)
│   └── tauri-*-bundled (with embedded server)
├── FLTK Backend Apps
│   ├── renderer (OpenGL)
│   └── hyperchad (FLTK toolkit)
└── Egui Backend Apps
    ├── native (Vulkan/WGPU)
    └── player (GPU-accelerated)
```

This separation ensures each backend gets exactly the dependencies it needs without conflicts or bloat.
