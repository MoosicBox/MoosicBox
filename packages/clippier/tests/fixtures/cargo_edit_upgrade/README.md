# Pinned cargo-edit upgrade corpus

Adapted from MIT-licensed cargo-edit tests, not linked to or executed by Clippier.
See `LICENSE-MIT`. `sources.json` records each scenario's source membership:

* Upstream https://github.com/killercup/cargo-edit at
  `138d4e2948ddcbf249c31c584b02e1b161a86221`.
* Fork https://github.com/BSteffaniak/cargo-edit, `add-skip-git-dependencies`, at
  `d6b8eb6990f8a70ffe0d318b3e025328ea59aa7d`.

The union contains 38 scenarios. Upstream adds `upgrade_verbose_hint`; the fork
adds `patched`. Shared input/output fixture trees are byte-identical. Both driver
versions are retained when different (`original.rs.txt`, `fork-original.rs.txt`).
`registry-original.rs.txt` preserves the published registry setup. `sha256.json`
protects the imported data, including original manifests, lockfiles and drivers.
Fixtures are never regenerated from native output to make tests pass.

## Executable coverage

`tests/upgrade_cargo_edit.rs` runs all 38 named scenarios plus inventory and digest
checks. Native planning uses injected original registry versions, including build
metadata and MSRV information; it needs neither cargo-edit nor registry networking.
Manifest comparisons check all semantic TOML fields (only quote/decor spelling is
normalized). All other fixture files remain byte-identical to input. Planning is
asserted read-only; apply exercises the native transaction. Dry-run preserves
original bytes. Formatting/comment preservation is also covered by native editing
tests, not claimed from semantic corpus comparisons alone.

`tests/upgrade_cargo_edit_requirements.rs` runs all 52 requirement-rewrite vectors
from `version-original.rs.txt`, nine pin-boundary cases from
`upgrade-original.rs.txt`, and a build-metadata regression.
`tests/upgrade_cargo_edit_dependencies.rs` adapts 11 serialization/source shapes
from `dependency-original.rs.txt` to native edit-in-place behavior, comparing every
non-version field. This is not cargo-edit's entire unit suite: constructors, version
incrementing, dependency addition and platform path normalization are not native
upgrade APIs and are not claimed as full parity.

## Explicit differences and gaps

The harness changes only named fields in the original output oracle for deliberate
native policies, then compares the rest of the manifest without exemptions:

* `preserve_op`: `<0.2` and `<=0.2` stay constrained, rather than broadening them
  into an unrelated version requirement.
* `implicit_prerelease`: an existing prerelease does not opt into an unrelated
  future prerelease line; native selects stable `99999.0.0`.
* `upgrade_everything`, `exclude_dep`, `upgrade_verbose`, `upgrade_verbose_hint`:
  default source policy preserves path dependencies' publish requirements.
* `exclude_renamed`: native upgrades underlying `toml_edit` through alias `te`;
  the original oracle leaves it unchanged. The excluded `regex`/`rx` remains
  unchanged in both. This documents the observable difference, not a claim that
  upstream's behavior is incorrect.
* Git declarations and patch tables are preserved, not converted into registry
  sources. Exact/wildcard/upper-bound pins require explicit opt-in.

Migration gaps are tested as rejection with **all inputs unchanged**, not parity:

* `locked`, `locked_dry_run`: no native `--locked` flag.
* `ignore_rust_version`: no native `--ignore-rust-version` flag.
* `to_version`: no exact `name@version` target or cargo-edit boolean mode flags;
  no independent native `--target` override either.

`invalid_dep`, `invalid_flag`, `invalid_manifest` and
`invalid_workspace_root_manifest` likewise assert actual native errors, not output
text identity. `lockfile` checks manifest equivalence while explicitly preserving
the original lock: resolver parity with cargo-edit's fixture registry is **not**
claimed. Native lockfile resolution/refresh is covered separately in `upgrade_cli`
and native upgrade tests. Cargo-edit verbose stdout/stderr, resolver package
publishing, and exact diagnostic wording are not snapshot-parity requirements.

## Validation

From the workspace root:

```sh
cargo test -p clippier --test upgrade_cargo_edit --test upgrade_cargo_edit_requirements --test upgrade_cargo_edit_dependencies --offline
cargo test -p clippier --offline
cargo clippy --all-targets
cargo fmt --check
```
