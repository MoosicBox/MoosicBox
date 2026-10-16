# Imported behavioral corpus

This directory is the only location containing external project names, source
URLs, imported comments, attribution, and provenance. The native harnesses use
behavioral names and do not link or execute either external implementation.

## Exact sources

Both branch heads were refreshed with `git ls-remote` on 2026-10-16 and matched
these pins:

| ID | Repository | Branch | Revision | Named tests |
|---|---|---|---|---:|
| upstream | https://github.com/bnjbvr/cargo-machete | main | `135555c9cd9f3b4723dee2766f0cacb99aac092b` | 18 |
| fork | https://github.com/BSteffaniak/cargo-machete | ignored-dirs | `87dd88aa744c5950fe155666ced33766ad62a338` | 25 |

Original source files, integration fixtures, READMEs and MIT license bytes are
retained once per unique content in `files/` (or `LICENSE`). Each source file's
`sources.json` entry records its original path, exact blob URL, SHA-256 and
`stored` path. Shared files point to the same stored bytes; the source paths can
be reconstructed exactly without keeping duplicate trees.

The ledger retains all 43 named test functions and 183 assertion sites, original
line numbers/text and executable mappings. `case_origins` maps all 187 source
case variants onto 110 unique executable cases. No assertion is ignored or
deferred. `sha256.json` covers every retained file except itself.

## Corpus and shared variations

The original union is 21 fixture directories (15 shared plus 6 fork-only).
`trees` maps each unique input tree's relative paths to stored payloads; the
harness materializes these into temporary directories. The 159 source input
trees become 92 unique trees. Equivalent executable cases merge their source
IDs and run only once. Distinct input bytes, options and expected findings
remain separate. `special_inputs` selects unique target/removal inputs and
removal oracles. `shared_variations` preserves every changed shared file.

`python3 rebuild.py` reconstructs the original evidence into a temporary tree,
regenerates adaptations and assertion mappings, and deterministically compacts
the result. No network or installed external analyzer is required. Payload,
tree and executable-case uniqueness are regression-tested.

The newer upstream inputs update rand from 0.8.5 to 0.9.3, bytes from 1.10.0
to 1.11.1, and rustls-webpki from 0.102.8 to 0.103.13, with associated lockfile
changes. Both sets are retained and exercised; library-name adaptation is not
inferred from the old version. Lexical assertion lists in the pins each contain
57 assertions, including all negatives. The ledger preserves both sources
rather than silently deduplicating their evidence.

`rebuild.py` rebuilds deterministic adaptations, the assertion ledger, and hash
inventory using only retained source evidence and Python's standard library.
Run from any directory: `python3 path/to/this/directory/rebuild.py`. It does not
fetch, execute Cargo, or create packages in the repository workspace.

## Offline adaptation and executable checks

* `adapted` is independent of `original`; original inputs are never edited.
  Leading `./` in workspace member strings is normalized to the equivalent
  native glob spelling; package membership and findings remain unchanged.
  All harnesses explicitly select Normal, Dev and Build dependency kinds.
  A scoped `.gitignore` preserves the eight intentional target-tree input files.
* Legacy package/workspace `metadata.cargo-machete.ignored` and `ignored-dirs`
  become local `clippier.toml` `[machete]` settings at the same scope. Legacy
  tables are removed from adapted manifests. Legacy `renamed` maps become
  explicitly supplied name-resolution metadata, not a new Clippier option.
* Every source `check_analysis` callback executes without imported metadata and
  with synthetic resolved metadata. Tests with explicit Yes/No keep that mode.
  The rename-only legacy cases import only their configured names, without
  enabling general registry library-name knowledge. Each case retains its
  original metadata-mode label in the ledger.
* Synthetic resolved graphs have real temporary local package manifests and
  library files and are imported through `MacheteMetadata::from_cargo_json`.
  This tests the metadata import boundary without Cargo, an installed external
  tool, registry cache, network, environment mutation, or subprocesses.
  Both library/package name differences and explicit dependency aliases remain
  distinct. Workspace inherited ownership remains asserted.
* Each case checks exact manifest-relative dependency keys, actual package
  names, dependency kinds, target predicates, inherited ownership and statuses.
  No expected finding is represented only as a count. Package names inside
  imported fixtures are data, not new harness test names.
* All 114 source lexical inputs map to shared executable inputs that preserve the original assertion text as source input,
  including invalid/incomplete Rust, mixed casing, the `:log` negative, extern
  crates, macro invocations, absolute roots, compound imports and nested negative
  paths. Every case checks analysis AND plan reports without input mutation.
* Used-ignore assertions are checked independently by removing ignore settings
  only in a second temporary copy and verifying the exact used subset in the
  original package-local ignore scope. In particular workspace-ignored futures
  is not misclassified as an original package-local used-ignore warning.
* Nine ancestor-pattern examples execute discovery through nested temporary
  workspace packages. Exact exclusion/non-exclusion is asserted. All six real
  exclusion fixtures and both workspace nested/deep discovery assertions run.
* Relative-path discovery uses a truly relative path without process-global
  current-directory changes, making parallel tests safe.
* Removal executes on temporary copies only, asserts exact declaration rows and
  original byte-for-byte stripped output, preserves every other file including
  Cargo.lock, and checks idempotence. Additional safety cases reject stale
  source/lock evidence and modified plans without partial writes.

## Explicit intentional differences (not silent oracle relaxation)

1. `*:src/main.rs:test_ignore_target:assertion-3` originally permits discovering
   packages below target when the skip option is false. Clippier has no such
   escape hatch: target trees are always pruned, even with `no_ignore=true`.
   The harness explicitly requires empty exact discovery for all three modes.
   The first two original assertions also require empty discovery.
2. `*:src/main.rs:test_remove_dependencies:assertion-1` is originally a raw table
   editor, allowed to remove used log-once declarations. Clippier is a checked
   removal engine. The byte-exact original output is tested after replacing
   only the temporary source with a log-only use; a second executable test keeps
   the original source and requires both log-once occurrences to remain USED.
   Selectors must not force a used dependency out. No output byte oracle changed.
3. Legacy renamed configuration is supplied as explicit synthetic resolution
   evidence. This preserves all original dependency expectations (including bad
   bytes -> u8s yielding unused bytes) without introducing legacy production
   configuration compatibility.

These are API/safety adaptations, not permission to change baseline lexical or
exclusion outcomes. Advanced expansion, cfg solving and proc-macro execution are
outside baseline parity and remain deferred; no speculative crates are added.

## Anticipated strict failures / engine handoff

These are inspection-derived expectations to validate, not observed test results.
No Cargo build/test was run by the fixture worker (coordinator owns builds).
Every mismatch remains an ordinary failing assertion, never `#[ignore]`.

* `*:src/search_unused.rs:test_regexp:assertion-8`: `use :log;` must leave log
  UNUSED; assertions 50, 51, 52 and 56 must not treat nested non-root futures
  as external roots. Positive compound/root assertions must stay USED.
* `*:src/search_unused.rs:test_regexp:assertion-36` through `assertion-45` include
  deliberately incomplete macro token snippets. They retain original positive
  USED expectations. Absence or malformed syntax must not invent unrelated
  lexical usage. The original exact snippets and booleans are in the ledger.
* `*:src/search_unused.rs:test_renamed_field_works` expects bytes/log UNUSED,
  rustls-webpki USED, flagset IGNORED. The workspace variant expects bytes/flagset
  UNUSED and rustls-webpki USED. The rename map is intentionally authoritative.
* `*:src/search_unused.rs:test_crate_renaming_works`: xml-rs USED with resolved xml
  library metadata, UNUSED without it. The manifest dependency key must remain
  xml-rs in findings. `test_unused_renamed_in_spec` expects tracing UNUSED;
  `test_unused_renamed_in_registry` expects xml-rs UNUSED.
* `fork:src/search_unused.rs:test_ignore_dirs_nested_works`: regex/rand/tokio/uuid
  UNUSED, log/serde USED. The glob case expects regex/serde/uuid UNUSED and
  log/tokio USED. Explicit module declarations into excluded trees must not
  turn these exact baseline findings into unrelated incompleteness.
* `fork:src/main.rs:test_ignore_dirs_workspace_nested_works` and
  `test_ignore_dirs_workspace_nested_deep_works`: only inner/Cargo.toml findings
  are discoverable; grep UNUSED with log-once/serde USED, and no nested package
  rows. Generic ancestor matching must not be replaced with basename matching.
* `*:src/search_unused.rs:test_multi_key_dep`: cc/rand UNUSED; log and both
  log-once declarations USED. Build dependencies and target occurrences must
  keep their original kinds and target predicate.

## Validation

The harness verifies byte hashes, source-file references, complete assertion
mappings and uniqueness of payloads, input trees and executable cases. Native
analysis and removal tests run offline; originals are evidence, not executable
external implementations.

```
cargo test -p clippier --offline --no-default-features --features machete \
  --test machete_analysis --test machete_removal
```
