# Optimized instrumentation snapshot

Recorded on 2026-04-25 using the environment in `BASELINE.md` and
`benchmark-instrumentation`.

## Representative latency

```text
instrumentation/mixed_markdown: 66.04 µs median
baseline: 111.32 µs
change: approximately 40.6% lower
```

## Session and final-writer counters

| Input                                    |   Bytes |      Normal parses/session | Exceptional reparses | Final bytes written per formatting pass | Peak final capacity |
| ---------------------------------------- | ------: | -------------------------: | -------------------: | --------------------------------------: | ------------------: |
| Mixed Markdown                           |     226 | at most 1 per body session |                    0 |                                     201 |                 203 |
| `spec/opus-native/plan.md`               | 918,874 |                          1 |                    0 |                                 918,874 |             918,875 |
| `spec/generic-schema-migrations/plan.md` | 802,165 |                          1 |                    0 |                                 802,165 |             802,166 |

The large canonical files classify unchanged and allocate no owned changed
outcome. Their final writer grows once to approximately input size and writes one
final document's worth of bytes per pass. The pipeline's conservative logical
memory bound remains one input/output pair per bounded worker; the default caps
workers at eight.

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
