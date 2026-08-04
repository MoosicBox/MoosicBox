# Optimized instrumentation snapshot

Recorded on 2026-04-25 using the environment in `BASELINE.md` and
`benchmark-instrumentation`.

## Representative latency

```text
instrumentation/mixed_markdown: 65.65 µs median
baseline: 111.32 µs
change: approximately 41.0% lower
```

## Optimized end-to-end results

| Workload                       |         Baseline | Optimized |                                       Change |
| ------------------------------ | ---------------: | --------: | -------------------------------------------: |
| Mixed Markdown                 |        111.32 µs |  65.65 µs |                                  41.0% lower |
| `spec/opus-native/plan.md`     | noisy 680–972 ms | 395.96 ms | materially lower; 51.8% Criterion comparison |
| Generic schema migrations plan |        426.47 ms | 250.32 ms |                                  41.3% lower |
| Clean repository-sized corpus  |           1.93 s |   0.881 s |                                  54.3% lower |
| Changed check, no diff         |         18.47 ms |  12.46 ms |                                  32.6% lower |
| Changed check, capped diff     |         21.91 ms |  15.31 ms |                                  30.2% lower |
| Changed write                  |         19.45 ms |  14.48 ms |                                  25.6% lower |

All measured product workloads exceed the locked 15–30% improvement targets.
No persistent cache is present, so warm behavior is the same correct formatter
path rather than a separate cache mode.

## Session and final-writer counters

| Input                                    |   Bytes |      Normal parses/session | Exceptional reparses | Final bytes written per formatting pass | Peak final capacity |
| ---------------------------------------- | ------: | -------------------------: | -------------------: | --------------------------------------: | ------------------: |
| Mixed Markdown                           |     226 | at most 1 per body session |                    0 |                                     201 |                 203 |
| `spec/opus-native/plan.md`               | 918,874 |                          1 |                    0 |                                 918,874 |             918,875 |
| `spec/generic-schema-migrations/plan.md` | 802,165 |                          1 |                    0 |                                 802,165 |             802,166 |

The large canonical files classify unchanged and allocate no owned changed
outcome. Their final writer grows once to approximately input size and writes one
final document's worth of bytes per pass. The final AST path now streams source
slices and normalized blocks directly into that writer; it no longer creates a
complete rendered document and rescans it for output policy. A six-large-file
integration test with two workers verifies retained diff content never exceeds
twice the largest per-file input/output pair and remains below complete-corpus
retention.

Counter collection itself formats an input and must not be mixed into latency
samples. The table reports per-session metrics and per-pass writer values after
accounting for the separate metrics and public-format calls used to inspect the
result.

## Parser decision

Ordinary large-file sessions invoke MDAST once with no exceptional reparse.
Combined with the 54% clean repository improvement and 40% mixed-input
improvement, this does not justify replacing the parser with an alternative that
lacks the required MDX and source-preservation model. Retain `markdown` MDAST;
reconsider only with a parity-capable candidate and new end-to-end profiling.
