# clippier-md performance benchmarks

The benchmark suite separates in-process formatter latency, `run_fmt` filesystem
throughput, and cold process startup. It reads the two large repository plans in
place rather than copying them into fixtures.

## Environment

Record this metadata with every saved baseline:

```bash
rustc -Vv
cargo -V
uname -a
sysctl -n machdep.cpu.brand_string 2>/dev/null || lscpu
pwd
find spec packages -type f \( -name '*.md' -o -name '*.mdx' -o -name '*.markdown' \) -print0 \
  | sort -z | xargs -0 shasum -a 256 | shasum -a 256
```

Also record storage type/location, power mode, other system load, the git commit,
and whether filesystem caches are cold or warm. Criterion uses its normal warmup
and sampling unless explicit command-line options are recorded.

## In-process latency and throughput

Build and run the Criterion target in release benchmark mode:

```bash
cargo bench -p clippier_md --features benchmark-instrumentation \
  --bench formatter_benchmarks
```

The groups cover:

- canonical tiny input (fixed per-call cost),
- representative mixed Markdown,
- both large real plan documents,
- a clean repository-sized corpus check,
- changed check mode with no diff,
- changed check mode with capped diff output,
- changed-file write mode, and
- benchmark-only parse/allocation/file/peak-byte counters.

Criterion reports wall time and byte throughput. The instrumentation benchmark
returns counters to profilers and debuggers without changing non-benchmark
builds. Use an allocation or peak-RSS profiler alongside Criterion when recording
memory baselines; `peak_in_flight_bytes` is the formatter's logical retained
input/output measure, not process RSS.

## Cold process startup

Build once, then execute a fresh process per sample:

```bash
cargo build -p clippier_md --release
python3 packages/clippier/md/benches/process_startup.py \
  --binary target/release/clippier-md --samples 30
```

Save the JSON output alongside Criterion's baseline. Do not mix these startup
numbers with `format_markdown` latency.

## Baseline and targets

Store raw Criterion output and the environment metadata before optimization.
Lock numerical targets only after inspecting the baseline, profiler output,
counter snapshots, and run-to-run variance. A valid optimization must retain the
existing parity and idempotence test results; throughput alone is insufficient.
