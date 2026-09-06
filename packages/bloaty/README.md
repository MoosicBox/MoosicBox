# MoosicBox Bloaty

Bloaty measures how Cargo feature configurations change the size of a final compiled artifact.
It builds an explicit baseline and one or more comparison scenarios, uses Cargo's structured
compiler output to identify the exact emitted artifact, and reports absolute and relative size
differences.

## Current capabilities

- Measures executables, `cdylib`, `dylib`, and `staticlib` final artifacts
- Selects packages and targets through Cargo workspace metadata
- Supports `dev`, `release`, and custom Cargo profiles
- Supports optional Rust compilation target triples
- Compares individual features or explicit feature combinations
- Supports baselines with default features, no default features, or explicit features
- Produces terminal text, versioned JSON, and reconstructable JSONL reports
- Records Rust, Cargo, host, Git, profile, target, and scenario provenance
- Preserves Cargo build caching and records build failures without measuring stale artifacts

## Installation

From the MoosicBox workspace:

```bash
cargo build --package bloaty --release
```

The executable is written to `target/release/bloaty`.

## Feature specifications

A baseline or named scenario accepts one of these forms:

| Specification   | Meaning                                                  |
| --------------- | -------------------------------------------------------- |
| `none`          | Disable default features and enable no explicit features |
| `default`       | Enable default features                                  |
| `qobuz,tidal`   | Disable default features and enable both named features  |
| `default,qobuz` | Enable default features and the named feature            |

Feature deltas are contextual. A feature's cost can change depending on the baseline and other
enabled features, so individual deltas must not be assumed to be additive.

## Usage

### Compare an individual feature

```bash
bloaty \
  --package moosicbox_server \
  --target moosicbox_server \
  --profile release \
  --baseline none \
  --feature qobuz
```

`--feature` adds the named feature to the baseline configuration. It is repeatable and also
accepts comma-delimited values:

```bash
bloaty -p moosicbox_server --baseline none \
  --feature qobuz --feature tidal
```

### Compare explicit combinations

Named scenarios use `NAME=FEATURE_SPECIFICATION`:

```bash
bloaty \
  --package moosicbox_server \
  --target moosicbox_server \
  --baseline none \
  --scenario sources=qobuz,tidal \
  --scenario all-sources=all-sources
```

Scenarios are explicit; Bloaty does not generate an unbounded feature powerset.

### Analyze default features

```bash
bloaty -p moosicbox_server \
  --baseline none \
  --scenario defaults=default
```

### Select another profile

Any profile accepted by Cargo can be selected:

```bash
bloaty -p bloaty --profile dev --baseline none --feature fail-on-warnings
bloaty -p moosicbox_server --profile small --baseline default --feature telemetry
```

### Select a compilation target

```bash
bloaty -p bloaty \
  --compilation-target aarch64-apple-darwin \
  --baseline none \
  --feature fail-on-warnings
```

The selected Rust target must already be installed and all required cross-compilation tools must
be available.

## CI integration

The workflow uses the repository's Clippier action for matrix generation, package/dependency
setup, streamed execution, and its aggregate execution report. Analysis commands live in
`.github/clippier/run-matrix/bloaty.yml`; there is no separate Python adapter or JSON case format.

Every run discovers all public features for `aconverter`, the server, and the tunnel server
through Clippier and measures each individually against its package baseline (respectively:
no features, no features, and `postgres-raw`). The server baseline disables default features
and enables no database backend; database features are measured independently, not added to
SQLite/SQLx. Every run also measures `shipping=default`
as a separate configuration. This is not an all-features-at-once build or an exponential
feature powerset. Feature build failures remain visible and fail the analysis after reports
are written; features requiring other prerequisites are not silently omitted.

Discovery excludes `fail-on-warnings` (lint policy), `_`-prefixed private features, and
Windows-only `asio` on Ubuntu. `default` is excluded from the individual list only because
it is measured separately. Clippier owns platform filtering and dependency setup for the
discovered features. Desktop/mobile packaging and library-only crate attribution are not
covered by this package matrix.

Clippier splits discovery into chunks of eight features, with at most four shard jobs running
concurrently. Single-shard packages publish their report and historical comparison directly
from the analysis job; no merge job is created for them. Multi-shard packages publish only
one consolidated size summary after merging, while individual shard results remain available
in live logs and downloadable artifacts. Clippier's redundant per-shard summaries are disabled.
Each isolated job measures the same package baseline; only its first shard
measures shipping defaults. The final per-package job runs even when a shard fails, downloads
the retained reports, and calls Bloaty's Rust `--merge-reports` mode with the complete scenario
list from the saved Clippier plan. Historical comparison uses only the consolidated report.

Merging rejects missing/duplicate/unexpected scenarios, incompatible report provenance or
baseline configurations, and differing baseline byte sizes. Failed comparison measurements
are preserved in a merged report and fail CI after publication. A missing shard or invalid
baseline prevents a trustworthy merge; its diagnostics and individual shard artifacts remain
available. This intentionally does not silently tolerate cross-runner size variance.

For local merging:

```bash
bloaty --merge-reports shard-a.json shard-b.json \
  --expected-scenarios flac,mp3,shipping --output-format all --report-file merged
```

Measurements use Ubuntu 24.04 and Rust 1.95.0. This reduces, but does not eliminate, variance:
OS packages and runner images can change. Master artifacts are retained for 90 days and PRs
search the latest 30 successful master runs for the matching artifact identity. Comparisons
remain advisory; no empirically unsupported regression thresholds are enabled.

`--github-summary` appends the canonical report as Markdown to `GITHUB_STEP_SUMMARY`.
Failures/unavailable measurements appear first, followed by one baseline section. Successful
comparisons have collapsible views sorted by largest increase (open by default), total size,
scenario name, and build duration. Tables show exact bytes, signed byte deltas, percentage
deltas, feature configurations, and milliseconds. Numeric ties sort by scenario name; missing
deltas sort last and display as `—`. Durations include cache reuse, not clean-build benchmarks.
These are static views, not clickable column sorting; JSON/JSONL and live logs are unchanged.
`--fail-on-incomplete` makes unavailable/failed scenarios fail the command **after** writing
reports and the summary. CI uses both flags; the default local failure-reporting behavior is
unchanged. Pre-analysis failures appear in Clippier's execution summary/logs.

## Reports

Each scenario's result is printed to stderr as soon as its build and measurement finish,
including failures. This provides live progress without mixing diagnostics into machine-readable
stdout. In GitHub Actions, watch the **Analyze final artifact** step for these results.

The Bloaty workflow also publishes the text report to the run summary immediately after the
analysis step, plus advisory master comparisons on PR runs. GitHub publishes step summaries
when their steps finish; the live job log is the streaming view while analysis is running.
Downloadable text, JSON, and JSONL artifacts remain available.

Terminal text is the default and does not create files:

```bash
bloaty -p bloaty --profile dev --baseline none --feature fail-on-warnings
```

JSON and JSONL require a report base path:

```bash
bloaty -p bloaty --baseline none --feature fail-on-warnings \
  --output-format json --report-file report

bloaty -p bloaty --baseline none --feature fail-on-warnings \
  --output-format jsonl --report-file report
```

Use `all` to print text and create `report.txt`, `report.json`, and `report.jsonl`:

```bash
bloaty -p bloaty --baseline none --feature fail-on-warnings \
  --output-format all --report-file report
```

JSON reports include a schema version, selected build dimensions, environment provenance, the
explicit baseline, each comparison scenario, scenario durations, exact artifact paths, byte
sizes, signed deltas, percentage deltas, and structured build failures.

### Compare saved reports

Compatible JSON reports can be compared locally without rebuilding:

```bash
bloaty --compare-reports baseline.json candidate.json
```

Comparison requires matching schema version, package, target, target kind, Cargo profile,
compilation target, Rust compiler, host operating system, host architecture, and metric. Bloaty
rejects incompatible reports instead of presenting misleading deltas. Compatible reports show
absolute and percentage drift for each scenario and classify added, removed, or unavailable
scenarios.

### Characterize repeated-report variance

Use two or more compatible reports from unchanged source and build dimensions to measure observed
noise before selecting CI thresholds:

```bash
bloaty --characterize-variance run-1.json run-2.json run-3.json
```

The command reports each scenario's sample count, minimum, maximum, absolute byte spread, and
percentage spread. It rejects reports from different profiles, targets, platforms, or toolchains.
CI thresholds should be selected only after enough repeated reports show a stable upper bound.

### Enforce explicit comparison thresholds

Thresholds are opt-in and apply to every successfully measured matching scenario:

```bash
bloaty --compare-reports baseline.json candidate.json \
  --max-increase-bytes 1048576 \
  --max-increase-percent 2.0
```

A comparison exits unsuccessfully when either configured limit is exceeded. Bloaty has no implicit
threshold: users and CI must choose limits appropriate for the selected metric and environment.
Added, removed, unavailable, or incompatible scenarios remain separately classified rather than
being treated as zero-sized artifacts.

## Target selection

Bloaty supports final artifacts produced by binary, `cdylib`, `dylib`, and `staticlib` targets. If
a package has exactly one supported target, `--target` can be omitted. If it has multiple targets,
Bloaty reports the candidates and requires an explicit selection.

Target `required-features` are preserved from Cargo metadata. Cargo remains authoritative for
whether a scenario satisfies those requirements, including features activated transitively.

## Metric interpretation

The built-in metric is the exact final artifact's file size. It reflects the selected profile's
optimization, debug information, stripping, LTO, target platform, toolchain, and feature context.
Only reports with compatible build dimensions should be compared.

Bloaty currently records capability and version information for optional `cargo-size`, `cargo-bloat`,
and `cargo-llvm-lines` collectors. Unavailable collectors are represented as unsupported metric
outcomes and never invalidate the built-in final-artifact measurement. Supported JSON and JSONL
collector output can be normalized into typed label/value records; LLVM lines are explicitly
modeled as attribution, not binary bytes. External collectors execute through the same typed
boundary when explicitly invoked by an integration, producing parsed records or metric-specific
failures rather than terminal-only side effects.

## Development

```bash
cargo fmt --all
cargo test -p bloaty
cargo clippy -p bloaty --all-targets -- -D warnings
```
