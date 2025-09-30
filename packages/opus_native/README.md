# moosicbox_opus_native

Pure Rust implementation of the Opus audio decoder (RFC 6716).

## Features

- `silk` (default): SILK decoder for speech/narrowband content
- `celt` (default): CELT decoder for music/wideband content
- `hybrid` (default): Combined SILK+CELT decoder mode

## License

See workspace LICENSE file.
