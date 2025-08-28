# MoosicBox Development Environment

This project now uses **Nix Flakes** for reproducible development environments across Linux and macOS.

## Quick Start

### Using Nix Flakes (Recommended)

```bash
# Enter the development environment
nix develop

# Run a command in the dev environment
nix develop --command cargo build

# Enter the minimal CI environment
nix develop .#ci
```

### Using direnv (Automatic)

If you have direnv installed:

```bash
# Allow the project to use direnv
direnv allow

# The environment will now load automatically when you cd into the project
cd /path/to/MoosicBox  # Environment loads automatically
```

### Using legacy nix-shell (Backwards compatible)

```bash
nix-shell  # Still works via flake-compat wrapper
```

## What's Included

The development environment provides:

### Core Tools

- **Rust toolchain** (stable, with rust-analyzer, clippy, rustfmt)
- **Build tools** (cmake, ninja, pkg-config, gcc)
- **Development tools** (cargo-watch, cargo-edit, cargo-audit)

### Platform-specific Dependencies

#### Linux

- Audio: ALSA, PulseAudio, GStreamer
- GUI: GTK3, WebKit, X11, Wayland
- Graphics: OpenGL, Vulkan
- System: udev, systemd

#### macOS

- Audio: PortAudio, Core Audio
- Compiler: Clang (Nix-provided for compatibility)
- System: Security framework, SystemConfiguration

## Environment Details

- **Rust version**: Managed by rust-overlay (stable channel)
- **Platform detection**: Automatic Linux/macOS dependency selection
- **Library paths**: Automatically configured for GUI applications
- **Compiler**: GCC on Linux, Clang on macOS

## Building the Project

```bash
# Using the flake environment
nix develop --command cargo build

# Or enter the shell first
nix develop
cargo build

# Build specific packages
cargo build -p moosicbox_server

# Run tests
cargo test

# Format code
cargo fmt

# Lint code
cargo clippy
```

## Package Building

You can also build MoosicBox as a Nix package:

```bash
# Build the package
nix build

# Run the package
nix run
```

## Updating Dependencies

```bash
# Update all flake inputs
nix flake update

# Update specific input
nix flake update rust-overlay
```

## Troubleshooting

### Missing Dependencies

If you encounter missing system dependencies, they should be added to the appropriate platform section in `flake.nix`.

### Rust Version Issues

The rust-overlay ensures you get the latest stable Rust. If you need a specific version:

```bash
# Check current version
nix develop --command rustc --version
```

### Cache Issues

```bash
# Clean Nix cache if needed
nix store gc
```

## Migration Notes

- **Old shell.nix**: Still works as a compatibility wrapper
- **Environment variables**: Now set automatically per platform
- **Library paths**: Automatically configured in the flake

## Development Workflow

1. Clone the repository
2. Run `nix develop` or set up direnv
3. Use standard Cargo commands
4. All dependencies are automatically available

The flake ensures consistent, reproducible development environments across all platforms and team members.
