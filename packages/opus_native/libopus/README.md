# moosicbox_opus_native_libopus

**⚠️ TEMPORARY PACKAGE - NOT FOR PRODUCTION USE**

This package is an internal FFI wrapper around the official libopus reference implementation from Xiph.Org Foundation. It exists solely to generate test vectors for validating the pure Rust Opus decoder implementation in the parent `moosicbox_opus_native` package.

## Purpose

This package serves a temporary role in the development process:

- **Test Vector Generation**: Encodes and decodes audio samples using the official libopus implementation to create reference outputs

## Current Implementation

The package provides:

- **CMake Build Integration**: Compiles the official libopus library from source using the `build.rs` script (with fixed-point mode enabled for bit-exact decoding)
- **Raw FFI Bindings**: Unsafe extern functions for `opus_encoder_create`, `opus_encode`, `opus_decoder_create`, `opus_decode`, and their corresponding destroy functions
- **Safe Wrapper Module**: The `safe` module provides `Encoder` and `Decoder` types with memory-safe Rust interfaces
- **Minimal API Surface**: Only exposes the functionality needed for test vector generation

## Future Migration Plan

**This package will be removed once the pure Rust Opus decoder is stable.**

The migration timeline:

1. **Current Phase**: Using libopus FFI for test vector generation during `moosicbox_opus_native` development
2. **Transition Phase**: Once the Rust decoder passes all validation tests and achieves bit-exact output
3. **Final Phase**: Replace this FFI wrapper with a pure Rust encoder implementation for test vector generation
4. **Removal**: Delete this package entirely when no longer needed

See `../../../spec/opus-native/plan.md` for the detailed implementation roadmap of the pure Rust decoder.

## Upstream Source

This package builds the official Opus codec from Xiph.Org Foundation:

- **Repository**: https://gitlab.xiph.org/xiph/opus
- **Specification**: RFC 6716 - https://datatracker.ietf.org/doc/html/rfc6716

The upstream source is included as a git submodule in the `opus/` directory.

## Build Requirements

- CMake (for building libopus)
- C compiler toolchain
- Standard math library (`libm` on Unix systems)

The build process is handled automatically by `build.rs` and produces a static library that is linked into the Rust crate.

## Installation

This crate is internal and is not published to crates.io (`publish = false`).

Add it as a workspace dependency:

```toml
[dependencies]
moosicbox_opus_native_libopus = { workspace = true }
```

## Usage

Primary entry points are the safe wrappers in `safe`:

- `safe::Encoder::new(sample_rate, channels, application)`
- `safe::Encoder::encode(pcm, frame_size, output)`
- `safe::Decoder::new(sample_rate, channels)`
- `safe::Decoder::decode(data, output, frame_size, decode_fec)`

Use `OPUS_APPLICATION_AUDIO` or `OPUS_APPLICATION_VOIP` for encoder mode selection.
All fallible operations return `Result<_, safe::OpusError>`.

```rust
use moosicbox_opus_native_libopus::{OPUS_APPLICATION_AUDIO, safe::{Decoder, Encoder}};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut encoder = Encoder::new(48_000, 1, OPUS_APPLICATION_AUDIO)?;
let mut decoder = Decoder::new(48_000, 1)?;

let input_pcm = vec![0i16; 960];
let mut packet = vec![0u8; 4000];
let packet_len = encoder.encode(&input_pcm, 960, &mut packet)?;

let mut output_pcm = vec![0i16; 960];
let _decoded_samples = decoder.decode(&packet[..packet_len], &mut output_pcm, 960, false)?;
# Ok(())
# }
```

## Portable live reference oracle

This unpublished test-only package is **not a production dependency** of the native
codec. Normal native contract tests must not invoke/build the C oracle. Consumers
must opt in explicitly (for example ignored live tests plus an explicit executable
path); missing prerequisites in a requested live run are errors, never silent skips.
Frozen test vectors and their manifest/hash verification are unchanged.

Initialize the pinned `opus` submodule before building. Requires Git, a C compiler,
CMake and a Makefiles or Ninja generator (`compile_commands.json` is mandatory).
The runner uses Cargo static linking, not Linux-only `dlopen` or whole-archive tricks.
Visual Studio/Xcode generators are not currently qualified; use Ninja on those hosts.

```sh
CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/tmp/opus-oracle-portable-example \
  cargo build -p moosicbox_opus_native_libopus --locked --offline --example reference_oracle
/tmp/opus-oracle-portable-example/debug/examples/reference_oracle provenance
python3 -B packages/opus_native/libopus/src/oracle_tests.py \
  /tmp/opus-oracle-portable-example/debug/examples/reference_oracle
```

Use a fresh target directory for clean reproduction; never delete an active cache.
For cross-target/profile builds obtain the example executable from Cargo's
`--message-format=json` compiler-artifact record, or specify the known target/profile
path explicitly. No archive glob, newest-file selection or source `.libs` is used.

### Qualification and provenance

`provenance` emits JSON schema `moosicbox.opus.reference.v1`. It includes source
revision/cleanliness, exact Cargo archive/build paths, target/host/profile, compiler
identity/configuration, full compile commands/cache, and run-specific SHA-256 hashes.
The required source revision is `34bba701ae97c913de719b1f7c10686f62cddb15`.
Every compile command is checked for fixed-point, disabled float API, no RES24/QEXT
or architecture-specific intrinsics definitions, alongside generated config and
CMake settings. CMake places important defines on compiler command lines, not
necessarily in `config.h`. Unsupported undefines fail qualification.

Archive/config hashes are **this build's evidence**, not a universal machine-binary
identity. The runner rechecks tracked source cleanliness/revision and exact artifact
hashes at startup; keep the Cargo build artifacts and source available. This is
local build provenance, not signed supply-chain attestation. Rust callers can use
`reference::reference_build()` and `reference::verify_reference_build()`.

### Persistent JSON-lines protocol (v1)

Without arguments the executable accepts one JSON object per line, returns one
JSON object per line, and flushes immediately. State persists until replaced or
reset. Any invalid command/C error terminates nonzero (diagnostic on stderr).
Maximum request size is 1 MiB; PCM is bounded to 11,520 i16 values, packets to
65,535 bytes, decode frame size to 5,760 samples/channel. Encode output is at most
4,000 bytes. No files or fixtures are written.

| `op`        | Request fields                                                             | Response                                          |
| ----------- | -------------------------------------------------------------------------- | ------------------------------------------------- |
| `encoder`   | `rate`, `channels`, optional `application` (default 2049)                  | `{"ok":true}`                                     |
| `decoder`   | `rate`, `channels`                                                         | `{"ok":true}`                                     |
| `configure` | `controls`: up to 32 `[request,value]` pairs                               | `{"ok":true}`                                     |
| `encode`    | `frame_size`, interleaved `pcm` i16 array                                  | `{"packet":[u8,...]}`                             |
| `decode`    | `frame_size`, `packet` u8 array or `null` for loss, optional `fec` boolean | `{"samples":samples_per_channel,"pcm":[i16,...]}` |
| `reset`     | `state`: `encoder` or `decoder`                                            | `{"ok":true}`                                     |

Rates: 8000/12000/16000/24000/48000; channels: 1/2. Encoder controls are limited to
4002 bitrate, 4004 maximum bandwidth, 4006 VBR, 4008 bandwidth, 4010 complexity,
4012 in-band FEC, 4014 loss percentage, 4016 DTX, 4020 constrained VBR,
4022 forced channels and the **pinned-reference-only private** force-mode request
11002 (1000 SILK, 1001 hybrid, 1002 CELT). Public encode/decode do the actual work;
private mode selection must not be described as a portable public libopus CTL.

For Python, load `src/oracle.py` and use `with Oracle(explicit_executable) as oracle`;
`oracle.manifest` holds qualified metadata and `oracle.request(op, **fields)` sends
commands. Use `python3 -B` to avoid source-tree bytecode artifacts. The helper does
not auto-build, find caches, substitute system libopus, or skip missing references.
Pure native tests remain enabled normally; consumer owners must separately migrate
live comparisons to this explicitly requested facility.

## License

Licensed under MPL-2.0.

## Related Packages

- **`moosicbox_opus_native`**: Parent package containing the pure Rust Opus decoder implementation
- **`moosicbox_opus_native_test_vectors`**: Test vector data generated using this package
