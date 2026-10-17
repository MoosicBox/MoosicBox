# Frozen reference core v1

These 39 files (39,156 bytes) freeze 12 packets, raw PCM, metadata and manifests
for `celt_fb_mono`, `celt_fb_stereo`, and `hybrid_fb_mono`. They were captured from
the independent C generator at reference revision
`34bba701ae97c913de719b1f7c10686f62cddb15`, with fixed-point ON, intrinsics OFF,
float API OFF, force-mode/channel/bandwidth, CBR, complexity 10. CELT bitrate is
64000 and hybrid bitrate is 32000. Source signal is `integer_triangle_v1`.

Builds regenerate using C and **require exact agreement with these files**.
They never update this source directory. Missing files are errors too. This
freezes the core independently of generator changes; it does not establish that
the native codec passes. Other cases remain generated, not frozen.

`SHA256SUMS` contains identities of these 39 oracle files. Verify from here with
`sha256sum -c SHA256SUMS`. Initial capture used two independent generator runs
with byte-identical results (A1 handoff); A3 copies these same bytes into source.

## Explicit reviewed regeneration only

Do not replace PCM to match native output. A reference change requires a separate
architectural/review decision, explicit source revision/configuration updates,
and a documented reason. No automatic update switch is supplied.

1. Keep old frozen bytes and SHA256SUMS for comparison.
2. Run the focused Cargo vector test/build with `CARGO_BUILD_JOBS=2 --offline`.
   If reference bytes differ, the build deliberately fails **after** generating
   candidate files in the reported `OUT_DIR/generated/sequences/`.
3. Independently verify pinned C source/configuration and run generation twice
   in separate output directories. Inspect every changed packet, metadata and PCM
   identity; report parity changes, never weaken expectations.
4. Only after explicit approval, replace the three frozen sequence directories
   from that exact candidate output; record the generator source identity,
   reference revision/configuration, commands, and reason in this README.
5. Recompute SHA256SUMS for packet.bin, expected.pcm, meta.json and sequence.json
   files (exclude README and SHA256SUMS). Run focused tests and checksum validation.

Initial freeze provenance: generator `build.rs:v2`; raw decoder output, no delay
compensation; corpus inventory SHA-256
`7129c4811fb7e6019f23d78e7d34bd52baf46760bde9845c5576fe9166609f08`.
