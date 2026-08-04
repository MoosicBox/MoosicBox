# Performance instrumentation interpretation

`benchmark-instrumentation` exposes counters for parser calls, exceptional
transformed-source reparses, scanned input bytes, processed/changed files,
owned changed outcomes, logical per-file in-flight bytes, final bytes written,
and peak final output capacity.

Run:

```bash
cargo bench -p clippier_md --features benchmark-instrumentation \
  --bench formatter_benchmarks instrumentation/
```

Interpretation:

- `parse_count` must be at most one on an ordinary AST path.
- `exceptional_reparse_count` identifies source transforms that invalidate AST
  byte offsets and require an explicit replacement parse.
- `outputs_allocated` counts owned changed outcomes, not allocator calls.
- `peak_in_flight_bytes` is the largest logical input-plus-output pair for one
  worker; `peak_batch_in_flight_bytes` measures all retained diff content in a
  completed bounded batch and must remain at most worker-count times that
  per-file bound.
- `output_bytes_written` and `peak_output_capacity` expose final-writer copy and
  growth behavior. They do not replace an external allocation/RSS profiler.

The existing `markdown` MDAST parser remains intentional. The workspace's
`pulldown-cmark` alternative supplies CommonMark/GFM events and spans but not the
enabled MDX ESM/expression/JSX node model or equivalent opaque source-range
ownership. Do not compare parser microbenchmarks as though they represented a
valid formatter migration. Revisit parser replacement only after end-to-end
profiling shows parser dominance and a candidate first satisfies MDX, exact
UTF-8 byte positions, references, source-preserving opaque ranges, and full
parity.
