# MoosicBox mDNS

MoosicBox mDNS service scanner package for discovering MoosicBox servers on the local network.

## Features

- **scanner** - mDNS service scanner for discovering MoosicBox servers
- **simulator** - Simulated mDNS daemon for testing purposes

## Installation

```toml
[dependencies]
moosicbox_mdns = "0.1.0"
```

## Usage

```rust
use moosicbox_mdns::scanner::{MoosicBox, Context, service};
use moosicbox_async_service::Service as _;

// Create a channel to receive discovered servers
let (tx, rx) = kanal::unbounded_async();

// Create and start the scanner service
let scanner = service::Service::new(Context::new(tx));
let handle = scanner.start();

// Process discovered servers as they arrive
while let Ok(server) = rx.recv().await {
    println!("Found server: {} at {}", server.name, server.host);
}
```
