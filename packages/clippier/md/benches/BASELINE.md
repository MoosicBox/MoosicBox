# clippier-md performance baseline

Recorded on 2026-04-25 before production performance changes. These numbers are
directional because the intentionally short smoke sampling used to validate the
new harness is noisier than the full command documented in `README.md`.

## Environment

- Git commit: `777c6723ff60f7112723810a546318217c5535d1`
- Toolchain: `rustc 1.95.0 (59807616e 2026-04-14)`, Cargo 1.95.0
- Host: Apple M1 Max, arm64, macOS 26.6 / Darwin 25.6.0
- Storage: external `/Volumes/ehdd`, APFS volume, 64% used
- Tracked Markdown corpus: 4,276,585 bytes
- Corpus SHA-256 manifest hash: `fe88d711c33bf02f03c2f6defc2ad110f3c5d57e7f8e2c2b35a487429069d3cd`
- Profile: Cargo `bench` (optimized)
- Criterion smoke method: 100 ms warmup, 200 ms measurement, 10 samples; focused
  tiny/mixed reruns used 200 ms warmup, 500 ms measurement, 20 samples
- Cache state: warm filesystem cache; no persistent formatter cache

## Initial observations

| Workload                                          |                      Baseline |
| ------------------------------------------------- | ----------------------------: |
| Cold process startup, canonical tiny file         | 9.88 ms median (20 processes) |
| `format_markdown`, canonical tiny                 |                      14.67 µs |
| `format_markdown`, mixed Markdown                 |                     114.11 µs |
| `format_markdown`, `spec/opus-native/plan.md`     |             noisy: 680–972 ms |
| `format_markdown`, generic schema migrations plan |                     426.47 ms |
| `run_fmt`, clean repository-sized corpus, no diff |                        1.93 s |
| `run_fmt`, changed check, no diff                 |                      18.47 ms |
| `run_fmt`, changed check, capped diff             |                      21.91 ms |
| `run_fmt`, changed write                          |                      19.45 ms |

The large opus sample and process-startup maximum showed substantial system
noise, so optimization comparisons must use the full documented sampling period
and fresh Criterion baselines. The initial evidence nevertheless identifies
repository throughput and large-document parsing/rendering as the dominant
costs; capped diff generation is secondary for the representative changed file.

## Locked initial targets

Measured against the same corpus/configuration/environment with full Criterion
sampling:

- improve clean repository check median throughput by at least 30%,
- improve each large-plan median latency by at least 25%,
- improve changed check/write median latency by at least 15%,
- do not regress tiny or mixed single-file median latency by more than 5%,
- reduce normal AST parse count to at most one per file,
- make clean `run_fmt` files allocate no owned final output,
- keep peak logical in-flight input/output bounded by configured worker count,
  rather than total corpus size.

All performance targets remain subordinate to byte parity, idempotence,
deterministic reporting, and existing CLI/write semantics.
