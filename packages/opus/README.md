# MoosicBox Opus Codec

RFC 6716 compliant Opus audio codec decoder implementation for Symphonia.

## Overview

The MoosicBox Opus package provides a pure Rust implementation of the Opus audio codec decoder, designed to integrate seamlessly with the Symphonia multimedia framework.

## Development Status

This package is under active development. Implementation progress:

- 🚧 Packet structure parsing
- 🚧 TOC byte interpretation
- 🚧 Frame length decoding
- 🚧 Symphonia codec trait implementation
- 🚧 SILK mode decoding
- 🚧 CELT mode decoding
- 🚧 Hybrid mode support

## License

Licensed under the same terms as the MoosicBox project.

## See Also

- [MoosicBox Audio Decoder](../audio_decoder/README.md) - Audio decoding framework
- [RFC 6716](https://tools.ietf.org/html/rfc6716) - Opus codec specification
- [Symphonia](https://github.com/pdeljanov/symphonia) - Multimedia decoding framework
