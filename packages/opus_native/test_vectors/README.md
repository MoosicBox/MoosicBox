# Opus reference fixtures

Generated at build time by the independent vendored C libopus encoder/decoder;
no native Rust codec code is used by this oracle. This corpus is not a claim
that the native decoder passes these cases.

## Reference and identity

Pinned source: `34bba701ae97c913de719b1f7c10686f62cddb15` (the libopus
submodule). The build rejects another revision or tracked source modifications.
Configuration: fixed-point ON, intrinsics OFF, float API OFF. Encoder settings:
private force-mode CTL 11002, explicit bandwidth/max bandwidth, forced channel
count, CBR, complexity 10; bitrate 16000/32000/64000 for SILK/hybrid/CELT.
The private CTL is reference-testing-only, tied to that source revision.

`generated/provenance.json` records this configuration. Every packet's metadata
also records source revision, settings, and FNV-1a 64-bit checksums of packet and
PCM bytes. FNV is a stable accidental-change identity, not cryptographic proof.
Historical named signals retain their floating-point synthesis; the new
`integer_triangle_v1` stateful signals use integer arithmetic only. Cross-platform
identity of the old floating-point signals is not established.

## Inventory and loading

`test_vectors_dir()` locates `generated/`. `inventory.json` v1 lists **all** cases:
`{"format_version":1,"cases":[{"id":"...","path":"...","kind":"single|sequence","packet_count":1}]}`.
Paths and IDs are relative to `generated/`; entries are sorted by path. There are
38 single cases (18 SILK, 13 CELT, 7 hybrid) and five four-packet sequences.
Inventory counts are asserted by the harness, not inferred from success reports.

`TestVector` and its existing load APIs remain compatible. Each case contains
`packet.bin`, `expected.pcm`, `meta.json`. Metadata v2 adds `frame_samples`
(per channel), `bandwidth`, `packet_index`, `decoder_state`, `pcm_semantics`,
`reference_revision`, `reference_config`, `bitrate`, `packet_fnv1a64`,
`pcm_fnv1a64`. The loader checks TOC mode/bandwidth/channels/duration, byte
length, checksums, and PCM semantics. Legacy metadata without v2 still loads.
`load_all` retains its historical behavior of skipping unreadable cases;
acceptance runners should instead use the mandatory inventory and fail on any
load error, rather than treating skipped cases as passing coverage.

SILK has **no SWB wire mode**. The two retained `silk/swb/*` historical paths
now explicitly contain WB-coded SILK at 24 kHz output, with `bandwidth:"wb"`.
Do not infer wire bandwidth from those directory names.

## Stateful sequences

Discovery marker: `generated/sequences/<id>/sequence.json`. Manifest v1:

```json
{"format_version":1,"decoder_state":"persistent","pcm_semantics":"raw_decoder_output","packets":[".","packet_001","packet_002","packet_003"],"signal":"integer_triangle_v1"}
```

`TestSequence::load(dir)` returns `{name: String, packets: Vec<TestVector>}` in
explicit decode order. Use **one decoder**, reset only before index 0. Root
packet files are the compatibility first packet decoded from reset; subsequent
packets have their own files under `packet_NNN/`. Per-packet metadata indices
are contiguous and state is `reset` at index 0, `continued` thereafter.
Sequence inventory IDs are `sequences/<id>`; an optional first-packet-only
report should use `<sequence-id>#packet_000`, not count it as another case.

Baseline sequences: `celt_fb_mono`, `celt_fb_stereo` (48 kHz),
`hybrid_fb_mono` (48 kHz), `hybrid_swb_stereo` (24 kHz), and `silk_wb_mono`
(16 kHz). All contain four 20 ms packets. Stereo signals have distinct channels.

PCM is raw reference decoder output, signed little-endian i16, exactly
`frame_samples * channels` samples per packet. **No delay is removed.**
Legacy compensated native checks and raw reference parity are different tests.
There are no transition/PLC/FEC sequences in this increment.

## Focused validation

```sh
CARGO_BUILD_JOBS=2 cargo test --offline -p moosicbox_opus_native_test_vectors -p moosicbox_opus_native_libopus
```

Tests reject mislabeled TOCs and load every mandatory inventory entry, including
all sequence packets. They do not establish native-code parity.

## Hardened metadata and frozen core

V2 packets must match exported `REFERENCE_REVISION` and `REFERENCE_CONFIG`, and
mode-specific bitrate. Packet index/state must agree; explicit unknown versions
are rejected. PCM byte sizes and durations use checked arithmetic before TOC
validation. Sequence milliseconds and frame samples must agree just as for
single cases. Tests mutate valid generated sequences (including huge integers,
provenance, duration, state, indices, ordering, hashes, and output configuration)
and require errors rather than panics.

`frozen/` pins three four-packet 48 kHz core sequences (CELT mono/stereo, hybrid
mono), including actual packet/PCM data. Builds compare regeneration byte-for-byte
and fail on drift; they never update source fixtures. See `frozen/README.md` for
explicit regeneration/review provenance and `frozen/SHA256SUMS` for identities.
The full mandatory corpus is now frozen as well (see below); diagnostic runner
acceptance remains separate from corpus integrity.

## Complete mandatory corpus freeze (A9)

`frozen/corpus/` preserves **38 single cases + 5 stateful sequences = 43 cases,
58 packets**, exactly as present in the audited inventory. Every original fixture
ID, metadata byte, encoded packet and full raw reference PCM sample is unchanged.
No warm-up/drop, alignment search, SNR threshold, or native-code output was used.
The legacy three-sequence freeze remains unchanged.

`frozen/manifest.json` records SHA-256 and byte count for **181 files** (117,581
bytes): all packets, full interleaved little-endian signed-i16 PCM, metadata,
sequence ordering, root inventory and generation provenance. Its reviewed SHA-256
is `8361b967ee0ac2fc4385e3b52d46dafd96921077aea3b23928b45f3d6251e8c3`, pinned in
`src/frozen.rs`. Changing a manifest entry without updating the reviewed code pin
fails. The manifest is an integrity/review anchor, not an external signature.

Every build validates both source-controlled frozen files and fresh generator
output against this manifest. Generator drift **fails**, never updates the
expectations. The verifier also checks inventory coverage/counts, mode vs ID,
packet TOC mode/bandwidth/channels/duration, nonempty packets, and exact complete
PCM sizes with checked arithmetic. Unsafe relative paths and symlinked file paths
are rejected. Additional unrelated files outside the manifest are not treated as
mandatory cases. Existing FNV format/checks and loading APIs remain compatible.

Consumers can explicitly validate a generated corpus with
`frozen::verify_frozen_corpus(&root) -> Result<(), String>` and report
`frozen::FROZEN_MANIFEST_SHA256`. This does not establish native codec parity.

### Independent pinned-C proof (offline)

The A9 audit independently repeated E3's public `opus_decode` test. Each single
case gets a fresh decoder; all four packets in each sequence share one decoder.
The audit checks complete concatenated raw PCM bytes, including counts; a failed
C call or zero packets fails. It never writes expected files. It verifies source
revision, clean tracked source, fixed-point/scalar/no-float configuration, and the
**exact** supplied archive before compiling the independent C driver:

```sh
python3 packages/opus_native/test_vectors/audit_reference.py \
  --build target/debug/build/moosicbox_opus_native_libopus-4d7c577870a89d38/out/build
```

Required archive SHA-256:
`4f2a11b2264e7172169207a29a35e624bda97fdfbd3e2cdb4026bff658b4d500`.
Source revision: `34bba701ae97c913de719b1f7c10686f62cddb15`.
Result: **43 cases, 58 packets, 39,040 interleaved samples, all raw PCM exact**.
Archive identities may differ across build paths/toolchains; this proof command
intentionally refuses substitutions. Normal Rust builds require exact corpus
regeneration, not byte-identical cross-toolchain archives.

### CI validation

```sh
CARGO_BUILD_JOBS=2 cargo test --offline -p moosicbox_opus_native_test_vectors --features fail-on-warnings
CARGO_BUILD_JOBS=2 cargo clippy --offline -p moosicbox_opus_native_test_vectors --all-targets --features fail-on-warnings -- -D warnings
```

Tamper tests cover formerly-unfrozen single packet/PCM and sequence packet/PCM,
manifest hash changes/removals, corrupt metadata/mode labels, zero packet lists,
missing/empty files, unsafe paths and symlinks. Deliberately revising the corpus
requires an explicit independent C audit and review of fixture bytes, complete
manifest and code hash pin together; never regenerate expectations to fit native
output. The package adds only existing workspace `sha2`/`serde_json` dependencies.

### A12 bounded frozen verification

`verify_frozen_corpus` keeps its existing API and manifest identity. Before opening
any corpus input it now requires regular files, real directory components, and
exact pinned byte lengths. Directories, FIFOs (even with no writer), sockets,
devices and symlinks fail rather than entering an unbounded `fs::read`. Opened
handles are rechecked for regular type/length; Unix also compares device/inode
identity. Every read, including the second metadata/packet parsing pass, is limited
to its trusted manifest length **plus one byte** and requires the exact length.
This catches truncation/growth without allocating according to untrusted size.

The manifest is embedded via `include_bytes!`, never opened from the supplied
corpus directory. A 128 KiB hard cap precedes its hash/JSON verification; the
reviewed manifest remains exactly 34,134 bytes. Malformed/oversized manifests
fail before any corpus access. No pinned fixtures or manifest hashes changed.

Linux x86, x86-64 and AArch64 additionally use UAPI `O_NONBLOCK | O_NOFOLLOW`
flags, preventing a final-component FIFO/symlink replacement race from blocking
open. Other targets retain pre-open checks and opened-handle validation. This
is not a general adversarial-filesystem sandbox: ancestor replacement races and
remote filesystem stalls are not fully eliminated. No new dependency or unsafe
FFI is introduced.

The regression clones the genuine full corpus, replaces
`celt/fb/silence_mono/expected.pcm` with a FIFO and checks verifier rejection in a
killable **two-second subprocess deadline**. It repeats FIFO/socket/directory/
symlink/1 GiB sparse regular-file mutations for packet, root inventory and
sequence metadata paths. Tests also reject `/dev/null`, malformed/oversized
manifest inputs, truncated reads and an infinite reader (bounded to length+1).
The child must actually execute one test; a zero-test filter cannot pass.
