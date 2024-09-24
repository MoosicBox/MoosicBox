# openport - find a free unused port

[![Crates.io](https://img.shields.io/crates/v/openport)](https://crates.io/crates/openport)
[![Documentation](https://docs.rs/openport/badge.svg)](https://docs.rs/openport)

Finds a free port, that is unused on both TCP and UDP.

Usage:

```rust
use openport::pick_unused_port;
let port: u16 = pick_unused_port(15000..16000).expect("No ports free");
```

## License

openport is provided under the MPL v2.0 license. Please refer to the LICENSE file for more details.
