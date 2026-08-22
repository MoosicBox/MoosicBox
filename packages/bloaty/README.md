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

## Reports

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
explicit baseline, each comparison scenario, exact artifact paths, byte sizes, signed deltas,
percentage deltas, and structured build failures.

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

Bloaty does not currently provide section-level, symbol-level, or crate-level attribution. It also
does not use rlib archive size as a proxy for final binary cost.

## Development

```bash
cargo fmt --all
cargo test -p bloaty
cargo clippy -p bloaty --all-targets -- -D warnings
```
