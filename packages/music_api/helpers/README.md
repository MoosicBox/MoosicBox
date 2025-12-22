# MoosicBox Music API Helpers

Helper utilities for working with MoosicBox music APIs.

This crate provides high-level helper functions for common music API operations,
simplifying tasks like enabling scanning, checking scan status, and performing
scans across different music sources.

## Features

- `scan` (default) - Enables music library scanning functionality

## Installation

```bash
cargo add moosicbox_music_api_helpers
```

## Usage

The `scan` module provides functions for managing music library scanning:

- `enable_scan` - Enables scanning for a music API's source in the library database
- `scan_enabled` - Checks whether scanning is enabled for a music API's source
- `scan` - Performs a music library scan for a music API's source

```rust
use moosicbox_music_api_helpers::scan;

// Enable scanning for a music source
scan::enable_scan(&music_api, &db).await?;

// Check if scanning is enabled
let enabled = scan::scan_enabled(&music_api, &db).await?;

// Perform a library scan
scan::scan(&music_api, &db).await?;
```

## License

See the [LICENSE](../../../LICENSE) file for license details.
