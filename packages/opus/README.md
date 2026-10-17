# MoosicBox Opus Codec

RFC 6716 compliant Opus audio codec decoder for Symphonia.

## Overview

The MoosicBox Opus package provides an Opus audio codec decoder that integrates with the Symphonia multimedia framework. It implements RFC 6716 packet parsing in Rust and uses libopus (via the `audiopus` crate) for audio decoding.

## Features

- ✅ Complete RFC 6716 packet structure parsing (all code types 0-3)
- ✅ TOC (Table of Contents) byte interpretation
- ✅ Frame length decoding with VBR and CBR support
- ✅ Padding extraction and handling
- ✅ DTX (Discontinuous Transmission) frame detection
- ✅ Symphonia `AudioDecoder` trait implementation
- ✅ Support for all Opus modes (SILK, CELT, Hybrid) via libopus
- ✅ Codec registry integration

## Backend selection

- Default features enable `libopus` (audiopus/C); existing registry behavior is unchanged.
- `default-features = false, features = ["native"]` enables only the native Rust
  production decoder, including SILK/CELT/hybrid and resampling support. It does
  not enable audiopus. Native audio correctness is still under development.
- Enabling both features keeps libopus as the default. Use
  `OpusDecoder::try_new_with_backend(&params, OpusBackend::Native)` or
  `OpusBackend::Libopus` for explicit comparisons. Disabled backends return
  `Unsupported`; decode failures never trigger backend fallback.
- With neither feature enabled, packet inspection remains available but decoder
  construction returns `Unsupported`.

Each backend receives the complete packet once, including TOC and framing.
Buffers support up to 120 ms at the selected output rate. Successful output is
converted from interleaved i16 to planar f32 with exact division by 32768 and
trimmed using Symphonia's packet `trim_start`/`trim_end` values (output samples).
Codec delay is not separately trimmed: the demuxer must supply packet trimming,
so pre-skip is not applied twice. The `AudioDecoderOptions::gapless` option (default true) controls packet trimming;
explicit backend construction uses gapless trimming. Reset clears predictive state and last output;
a reset failure makes subsequent decoding fail explicitly. Finalization has no
buffered audio to flush.

Mandatory `OpusHead` output gain (signed Q7.8 dB) is applied after integer-to-float
conversion on both backends, without clipping floating-point output. Unsupported
multistream mapping headers are rejected rather than decoded as single streams.

### Ogg boundary limitation

Symphonia 0.6.1 preserves `OpusHead` pre-skip but its Ogg start-bound calculation
can omit that trim for ordinary valid granules. The generated Ogg stream tests
retain the correct duration expectation and currently expose this upstream defect:
three 960-sample packets, pre-skip 312, end trim 111 must yield 2457 samples, not
2769. The adapter cannot unambiguously repair already-shifted packet timestamps
across arbitrary demuxers and seeks; the format reader must supply correct start
trimming. Output gain, end trim, registry/native routing, and seek-to-start/reset
replay are separately exercised. Installed FFmpeg 9.0.1 independently confirms 2457 samples
for the same generated file. RFC 7845 sections 4.2 and 4.5 distinguish pre-skip
from granule positions; the first granule of 960 is valid. Native PCM parity
remains unproven.

## Installation

```toml
[dependencies]
moosicbox_opus = "0.1.1"
```

## Usage

Register the Opus decoder with a Symphonia codec registry:

```rust
use symphonia::core::codecs::registry::CodecRegistry;
use moosicbox_opus::register_opus_codec;

let mut registry = CodecRegistry::new();
register_opus_codec(&mut registry);
```

Or create a registry with default Symphonia codecs plus Opus:

```rust
use moosicbox_opus::create_opus_registry;

let registry = create_opus_registry();
```

Parse Opus packets directly when you need RFC 6716 packet inspection:

```rust
use moosicbox_opus::OpusPacket;

let packet_bytes: &[u8] = &[0b0000_0000];
let packet = OpusPacket::parse(packet_bytes)?;
let toc = packet.toc;
let frame_count = packet.frames.len();
let padding_len = packet.padding.len();
```

Core public API items:

- `register_opus_codec` and `create_opus_registry` for Symphonia integration
- `OpusDecoder` (`symphonia::core::codecs::audio::AudioDecoder` implementation for Opus)
- `OpusPacket` and `OpusPacket::parse` for packet/frame parsing
- `TocByte`, `OpusMode`, and `Bandwidth` for TOC and mode/bandwidth interpretation
- `OpusFrame`, `FramePacking`, and `decode_frame_length` for frame-level packet handling

## Implementation

This package implements:

- **Packet parsing** (`packet.rs`) - Pure Rust parsing of Opus packet structures
- **TOC handling** (`toc.rs`) - Configuration and mode extraction
- **Frame processing** (`frame.rs`) - Frame length decoding and packing modes
- **Decoder integration** (`decoder.rs`) - Symphonia codec interface with libopus backend
- **Registry** (`registry.rs`) - Codec registration helpers

Audio decoding is performed by libopus through the `audiopus` crate.

## License

Licensed under the same terms as the MoosicBox project.

## See Also

- [MoosicBox Audio Decoder](../audio_decoder/README.md) - Audio decoding framework
- [RFC 6716](https://tools.ietf.org/html/rfc6716) - Opus codec specification
- [Symphonia](https://github.com/pdeljanov/Symphonia) - Multimedia decoding framework
