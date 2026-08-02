# Stateless Cargo SemVer Release Pipeline Progress

## Purpose

This document tracks the implementation of a low-friction, production-ready Cargo release system built into Clippier. The system must identify changed publishable packages, use `cargo-semver-checks` internally to calculate correct minimum version increases, propagate dependency requirement and version changes through the workspace, prepare reviewable changes on demand, and publish selected or all pending packages locally or through GitHub Actions.

The release system must be stateless: repository contents and crates.io are the canonical sources of truth. It must not require a committed release plan, hidden state file, prior workflow artifact, release database, or remembered workflow run.

Keep this file as local working state. Do not stage or commit it unless explicitly decided later.

## Current status

- **State:** Complete — implementation and local validation finished
- **Last updated:** 2026-08-20
- **Owner/session:** Local working notes

## Requested product outcome

- **Capability:** Clippier can reconstruct release requirements, calculate correct independent package versions, apply the required manifest changes, verify the result, and publish packages safely in dependency order.
- **Intended entry points:** Manual Clippier CLI commands and manually dispatched Clippier GitHub workflows.
- **Observable result:** Preparing and publishing correct Cargo package updates becomes a boilerplate-light, push-button operation. The user may select specific package roots or all packages requiring publication and may publish through GitHub Actions or locally.
- **Operation model:** Version preparation and publication happen only on demand. Ordinary pushes do not automatically change versions or publish packages.
- **State model:** Every command reconstructs current release state from workspace contents and crates.io. No `release-plan.json` or equivalent persistent release state is required.
- **Boundaries / non-goals:**
  - Initial publishing remains focused on Cargo workspaces and crates.io, matching existing Clippier publishing.
  - No daemon, release database, committed release plan, required workflow artifact, or automatic push-triggered publication.
  - No changelog generator as part of this scope.
  - No generalized multi-registry framework unless separately requested.
  - No claim that `cargo-semver-checks` detects every theoretical Rust API incompatibility.
  - No unrelated redesign of Clippier's workspace abstractions, feature matrix system, or CI framework.

## Locked decisions

- No `release-plan.json` or equivalent persistent release-state file will be committed or required.
- Repository contents and registry contents are canonical. Status, preparation, verification, and publishing must be reconstructable at any point.
- Publishable MoosicBox crates will move from inherited shared versions to explicit independent versions in a dedicated migration.
- Centralized `[workspace.dependencies]` declarations remain supported.
- `cargo-semver-checks` is an internal Clippier implementation dependency and is not exposed through Clippier's public API.
- Add `cargo-semver-checks` to root `[workspace.dependencies]` using the latest stable full version available when implemented and `default-features = false`.
- The dependency declaration will use Cargo's normal compatible version semantics, not an exact `=` pin. Clippier consumes it with `workspace = true`.
- `Cargo.lock` supplies checkout-level dependency reproducibility through the repository's normal dependency workflow; Clippier will not implement separate tool downloading or version pinning.
- Use the upstream `cargo-semver-checks` default feature heuristic unless explicitly configured otherwise. This is upstream behavior, not a Clippier-invented release heuristic.
- The default feature heuristic is deterministic for a given manifest and tool version. It enables default and stable-looking features while excluding names such as `unstable`, `nightly`, `bench`, `no_std`, `_...`, `unstable-...`, and `unstable_...`.
- Release diagnostics must report the `cargo-semver-checks` version, baseline, and checked feature behavior sufficiently to explain results.
- No random feature selection participates in release analysis.
- Local and GitHub publication use the same Clippier implementation and reconstruction logic.
- Existing affected-package build-and-test workflows remain responsible for testing packages changed by a generated release PR. Do not create a duplicate release test matrix.
- Do not create a separate planner unit-test workstream. Release-specific correctness still requires focused Clippier integration fixtures and verification coverage for behavior not proven by affected-package package tests.

## Codebase findings

### Existing changed-package analysis

- `packages/clippier/src/git_diff.rs` already performs Git change and external dependency analysis and constructs reverse-dependency information.
- `packages/clippier/src/workspace/analysis.rs` contains reusable affected-package and transitive-analysis behavior.
- `packages/clippier/src/workspace/traits.rs` and `packages/clippier/src/workspace/types.rs` define workspace/package/dependency abstractions and affected-package output.
- `packages/clippier/src/lib.rs::handle_affected_packages_command` already combines manually supplied changes, Git ranges, external dependency changes, workspace package mapping, reasoning, and multiple output formats.
- Git analysis is useful as an advisory prioritization and explanation source, but registry-backed package comparison remains authoritative and still covers every selected package for stateless release correctness.

### Existing version handling

- `packages/clippier/src/versioning.rs` supports manual `set` and uniform `major`, `minor`, `patch`, prerelease, and release operations.
- It updates selected package versions and internal dependency requirements, including `[workspace.dependencies]` and path dependencies.
- Its current model determines one old version and one new version for the selected set. It cannot calculate independent per-package targets or propagate a fixed point of dependency-induced releases.
- Manifest mutation is line/regex-oriented. The release path requires structured TOML editing for independently calculated versions and complete Cargo dependency syntax coverage.

### Existing publishing

- `packages/clippier/src/publish.rs` already builds a deterministic dependency-first order from normal/build workspace dependencies while ignoring dev-dependencies for ordering.
- It supports selected packages plus their mandatory publish dependencies.
- It queries crates.io, skips versions that already exist, validates package categories, waits for newly published versions to become available, retries rate limits, and reports partial failures.
- It publishes from temporary sanitized workspace copies and handles workspace dependency inheritance.
- The new release publication command should reuse this machinery rather than create a second publisher.

### Existing GitHub integration

- `.github/actions/clippier/action.yml` and `.github/actions/clippier/action.sh` already provide external Clippier source checkout, source hashing, release-binary caching, Git-range detection, command execution, outputs, summaries, and diagnostics.
- Release commands are not currently included in the Action's supported command/build/caching branches.
- `.github/workflows/publish-crates.yml` currently invokes `cargo run -p clippier -- publish` directly after dependency setup. It has no semver preparation or reconstructed verification stage.
- Existing build-and-test workflows already use Clippier's affected-package analysis and should validate the packages touched by release preparation.

### Current workspace version architecture

- Root `Cargo.toml` defines `[workspace.package].version = "0.4.0"`.
- Most MoosicBox packages use `version = { workspace = true }`.
- Internal dependencies are centralized in `[workspace.dependencies]` with `0.4.0` requirements.
- A small set of packages have `package.metadata.workspaces.independent = true`, but that metadata does not override Cargo's inherited package version.
- Independent minimal releases require a deliberate one-time migration that materializes package versions for publishable crates without numerically changing them.

### `cargo-semver-checks` integration evidence

- The inspected local checkout at `/Volumes/ehdd/GitHub/cargo-semver-checks` exposes a library API through `Check`, `Rustdoc`, `GlobalConfig`, `Report`, `CrateReport`, `required_bump()`, and registry baseline support.
- `Rustdoc::from_registry_latest_crate_version()` supports a registry baseline, while package selection permits checking selected workspace crates.
- `CrateReport::required_bump()` reports an additional required release level relative to the detected or assumed release.
- `cargo-semver-checks` implements Cargo's left-most-non-zero pre-1.0 compatibility convention.
- Its default feature behavior is explicitly called `Heuristic` upstream and deterministically enables all features except names considered likely unstable/private.
- The upstream project documents incomplete detection for some feature-subset interactions and API forms. Clippier must expose this limitation rather than overstate certainty.
- At research time, the inspected checkout and crates.io identified `0.50.0` as the latest stable release. Implementation must re-check crates.io and use the latest stable full version then available, following repository dependency rules.

## Canonical source of truth

The release state for one invocation is derived from:

1. Current workspace manifests and packageable source contents.
2. Current Cargo metadata and normal/build workspace dependency graph.
3. Latest relevant crates.io package versions and downloadable package contents.
4. Optional package roots selected by the user.
5. Existing Clippier and `cargo-semver-checks` configuration.

Git history may narrow candidates or explain why files changed, but it must not be required to recover release correctness. Workflow artifacts and prior command output are diagnostic conveniences only and must never become authoritative state.

## Completion path

1. The user manually invokes a local Clippier command or dispatches a GitHub workflow.
2. Clippier inspects current publishable workspace packages and takes a consistent in-memory snapshot of relevant crates.io state.
3. Clippier compares normalized current packageable contents with the latest applicable published crates to find changed or unpublished release roots.
4. Optional package selection limits requested roots while mandatory dependency/reverse-dependency closure remains enforceable.
5. Clippier invokes `cargo-semver-checks` internally against registry baselines and converts its reports into Clippier-owned compatibility outcomes.
6. Clippier calculates exact Cargo-compatible target versions, including pre-1.0 behavior.
7. Clippier determines which dependency requirements stop accepting proposed versions, applies the minimum appropriate requirement updates in memory, includes affected consumers, and repeats semver analysis until the graph reaches a fixed point.
8. `release prepare` applies only the necessary independent package versions, dependency requirements, and lockfile changes.
9. `release verify` reconstructs the analysis from repository and registry state and confirms that the current manifests are sufficient, consistent, complete, and packageable.
10. In GitHub, a managed release PR contains only the reconstructed source-controlled changes and an ephemeral generated explanation.
11. After merge, `release publish` reconstructs pending packages again, verifies immediately before publishing, and publishes selected or all pending crates in dependency order.
12. Versions already present in crates.io are skipped, so a partial publication can be resumed safely either locally or through GitHub Actions.

## User-facing command model

The exact names may be refined during implementation, but the product surface should remain cohesive and stateless:

```bash
# Reconstruct changed packages, required versions, ripple effects, and publish order.
clippier release status

# Restrict requested roots; mandatory closure is still included.
clippier release status --package foo --package bar

# Apply all currently required version and dependency changes.
clippier release prepare

# Prepare selected roots and mandatory closure.
clippier release prepare --package foo --package bar

# Preview without mutation.
clippier release prepare --dry-run

# Reconstruct and verify that current repository versions are exactly sufficient.
clippier release verify

# Publish all pending packages.
clippier release publish

# Publish selected roots plus required pending dependencies.
clippier release publish --package foo

# Preview publication.
clippier release publish --dry-run
```

A separate one-time migration command should make shared-version workspaces independently versionable:

```bash
clippier release independentize --dry-run
clippier release independentize
```

Existing low-level `clippier version` and `clippier publish` behavior should remain compatible or delegate cleanly where appropriate.

## Version calculation rules

For each package whose normalized packageable contents differ from its latest applicable registry baseline:

1. Treat the latest published version as the baseline.
2. Determine whether the current local version is already a sufficient unpublished target.
3. Invoke `cargo-semver-checks` with the intended minimum release assumption and inspect whether a larger update is required.
4. Calculate the smallest valid next Cargo version from the published baseline.
5. Never increment an already-sufficient local version merely because `prepare` is run again.

Expected mapping includes:

| Published baseline | Change class                           | Minimum target |
| ------------------ | -------------------------------------- | -------------- |
| `1.2.3`            | implementation-only / patch-compatible | `1.2.4`        |
| `1.2.3`            | compatible public API addition         | `1.3.0`        |
| `1.2.3`            | incompatible public API change         | `2.0.0`        |
| `0.4.3`            | compatible change                      | `0.4.4`        |
| `0.4.3`            | incompatible change                    | `0.5.0`        |
| `0.0.3`            | releasable change                      | `0.0.4`        |

Never-published packages retain a valid declared initial version unless registry state creates a collision or another explicitly documented rule requires adjustment.

Packages with no checkable library API need a deterministic policy because `cargo-semver-checks` cannot classify them. The default should be the smallest Cargo-compatible release for changed packageable contents, with explicit diagnostics and a narrowly scoped configuration override if needed.

## Dependency ripple algorithm

When package `A` receives a proposed version:

1. Find normal/build workspace consumers of `A`.
2. Test whether each consumer's existing dependency requirement accepts `A`'s proposed version.
3. If accepted, leave the requirement unchanged and do not release the consumer solely because `A` changed.
4. If rejected, update the requirement minimally in the owning manifest or centralized workspace dependency declaration.
5. Include every consumer whose packaged manifest changes in the release closure with at least the smallest compatible release.
6. Semver-check each newly included consumer against its own registry baseline because dependency changes may alter public API.
7. Raise its target if required and propagate that result to its consumers.
8. Repeat until no target version, dependency requirement, or release membership changes.

Dev-dependencies do not drive release closure or publication order by default. Target-specific normal/build dependencies, renamed dependencies, optional dependencies, workspace-inherited dependencies, and both inline and expanded dependency tables must be represented correctly.

A centralized `[workspace.dependencies]` update may affect multiple package manifests after Cargo inheritance is resolved. Each package whose published package metadata changes must be assessed independently.

## Feature analysis policy

The default release analysis uses `cargo-semver-checks`' upstream default feature heuristic. Clippier must not invent a separate implicit heuristic.

For a given manifest and dependency version, upstream default selection is deterministic. It generally enables default and regular features while excluding likely unstable/private names:

- `unstable`
- `nightly`
- `bench`
- `no_std`
- names beginning with `_`
- names beginning with `unstable-`
- names beginning with `unstable_`

Clippier must:

- Avoid randomized feature selection in release analysis.
- Report the active feature policy and relevant selected/skipped feature information in diagnostics and GitHub summaries.
- Report the `cargo-semver-checks` version used.
- Respect native `cargo-semver-checks` lint configuration and overrides.
- Permit explicit upstream-supported feature policy overrides only where a repository needs them.
- Clearly report upstream coverage limitations.

## Architectural obligations

- [x] Package/content comparison uses normalized packageable contents, not timestamps, prior workflow runs, or only source-directory Git changes.
- [x] Package version fields, `.cargo_vcs_info.json`, generated manifest formatting, and equivalent inherited-versus-explicit metadata are normalized where necessary so syntactic release preparation remains idempotent.
- [x] Real changes to source, features, dependencies, README/license inclusion, build configuration, and other packaged content remain visible.
- [x] Registry lookups are captured consistently within one command invocation and reused during that invocation.
- [x] Re-running preparation does not repeatedly increment versions.
- [x] A partial publish is safely reconstructable and resumable from workspace and registry state.
- [x] A registry change between preparation and publication triggers fresh verification or recalculation rather than trusting stale output.
- [x] Package, dependency, reason, changed-file, and publish ordering is deterministic.
- [x] Normal/build dependencies participate in release closure and publication order; dev-dependencies do not by default.
- [x] Existing requirements that accept a dependency target remain unchanged.
- [x] Requirements that reject a dependency target are updated minimally, and affected consumers are included and reanalyzed.
- [x] Dependency propagation continues to a fixed point.
- [x] Structured TOML editing replaces regex mutation for release preparation.
- [x] The independent-version migration does not falsely classify every crate as changed solely because inherited versions became explicit.
- [x] Publication fails closed on insufficient versions, invalid manifests, unresolved cycles, semver-analysis failures, inconsistent dependency requirements, package construction failures, or registry races that invalidate assumptions.
- [x] Untrusted PR workflows never receive publication credentials.
- [x] Local and GitHub workflows call the same release-domain implementation.
- [x] No required production behavior depends on an undocumented manual bridge or generated state file.

## Definition of done

### Product closure

- [x] The user can request release status for all changed packages or explicitly selected package roots.
- [x] Clippier identifies real unpublished package-content changes without relying on prior release state.
- [x] Every changed package receives the smallest version sufficient for its detected compatibility requirements.
- [x] Dependency requirement and version changes ripple through every required consumer and stop without including unrelated packages.
- [x] Preparation creates only necessary manifest and lockfile changes and is idempotent.
- [x] A GitHub workflow can create or update a reviewable release PR on demand without committing a release plan.
- [x] After merge, the user can publish through a manually dispatched GitHub workflow or the equivalent local command.
- [x] Selected publication includes mandatory pending dependencies in dependency order.
- [x] Partial publication can be rerun safely.
- [x] Documentation makes the default path boilerplate-light and transparent while preserving configuration for exceptional repositories.

### Architectural integrity

- [x] Repository and registry state remain the canonical sources of truth.
- [x] Publishable packages are independently versioned after the dedicated migration.
- [x] `cargo-semver-checks` remains a private implementation detail behind Clippier-owned models.
- [x] Dependency declarations follow repository workspace dependency conventions.
- [x] Release analysis and output are deterministic for a given checkout, registry snapshot, configuration, and locked dependency graph.
- [x] Manifest ownership and Cargo dependency semantics remain correct across workspace inheritance, aliases, target tables, and requirement forms.
- [x] Existing publishing behavior is reused rather than duplicated.
- [x] Existing affected-package CI remains the package test owner; release workflows do not duplicate its matrix.
- [x] No shortcut, duplicate source of truth, hardcoded package list, temporary wiring, or knowingly incomplete integration remains.

## Practical checklist

### Phase 1 — Release domain and registry-backed status

**Goal:** Establish a transient release model and reconstruct package status from workspace and registry state.

- [x] Introduce a cohesive `release` domain in Clippier while keeping `cargo-semver-checks` types private.
- [x] Model workspace packages, registry baselines, content changes, compatibility level, target versions, dependency reasons, and publish order as transient in-memory data.
- [x] Reuse or extract Cargo workspace/package graph loading shared by existing version and publish code.
- [x] Build a crates.io snapshot once per command invocation.
- [x] Download or otherwise obtain the latest applicable published package contents for comparison.
- [x] Compare normalized current `cargo package` output with published package contents.
- [x] Distinguish unchanged, changed, unpublished, locally prepared/pending, already-published, and publish-disabled packages.
- [x] Use Git affected-package analysis as optional advisory prioritization without narrowing authoritative registry-backed coverage.
- [x] Support all changed packages by default and explicit package roots through repeatable/comma-separated selection.
- [x] Emit deterministic human and JSON status without requiring a saved plan.
- [x] Include baseline and reasoning details sufficient to explain every status decision.

**Exit criteria:** `clippier release status` can reconstruct and explain current release requirements from a clean checkout and registry state.

### Phase 2 — Independent-version migration

**Goal:** Allow each publishable MoosicBox crate to receive its own correct version.

- [x] Add a dry-runnable `release independentize` migration command.
- [x] Identify publishable packages inheriting `[workspace.package].version`.
- [x] Materialize each package's current effective version in its own manifest without changing the numeric version.
- [x] Preserve the shared workspace package version for unpublished packages where inheritance remains appropriate.
- [x] Preserve centralized `[workspace.dependencies]` declarations.
- [x] Update `Cargo.lock` consistently.
- [x] Ensure normalized package metadata and packageable contents remain semantically equivalent before and after migration.
- [x] Make migration idempotent.
- [x] Apply the migration to MoosicBox in a dedicated reviewable change before enabling independent automated preparation.

**Exit criteria:** Every publishable MoosicBox crate can be versioned independently without changing its effective pre-migration release.

### Phase 3 — Private `cargo-semver-checks` adapter

**Goal:** Derive correct minimum compatibility requirements from registry baselines.

- [x] Re-check crates.io for the latest stable `cargo-semver-checks` release at implementation time.
- [x] Add its full stable version to root workspace dependencies with `default-features = false` and normal compatible Cargo semantics.
- [x] Add the optional Clippier dependency using `workspace = true` and wire a focused release/semver feature.
- [x] Keep all upstream types and configuration translation behind a private adapter.
- [x] Map reports into Clippier-owned compatibility outcomes and diagnostics.
- [x] Use latest applicable crates.io versions as normal baselines.
- [x] Handle never-published packages explicitly.
- [x] Handle packages without checkable library targets explicitly.
- [x] Use upstream default feature behavior unless explicitly configured otherwise.
- [x] Respect native workspace/package `cargo-semver-checks` lint overrides.
- [x] Report baseline versions, active feature policy, upstream tool version, failures, and coverage limitations.
- [x] Convert upstream results into Cargo-compatible target versions, including `0.y.z` and `0.0.z`.
- [x] Fail closed when analysis required for a release cannot complete.

**Exit criteria:** Status reports the minimum correct next version for every directly changed release root.

### Phase 4 — Fixed-point dependency propagation

**Goal:** Produce a complete minimal release closure with correct dependency requirements.

- [x] Build normal/build forward and reverse workspace dependency graphs from Cargo metadata.
- [x] Retain dependency kind, target, alias/package name, optionality, owner, inheritance, and current requirement information.
- [x] Evaluate whether existing requirements accept each proposed dependency version.
- [x] Leave compatible requirements and consumers unchanged.
- [x] Calculate the minimum appropriate update for incompatible requirements.
- [x] Include consumers whose normalized packageable manifest/content changes.
- [x] Semver-check each newly included consumer against its own registry baseline.
- [x] Recompute target versions, dependency requirements, and membership until stable.
- [x] Detect dependency cycles or impossible selection constraints with actionable errors.
- [x] Calculate deterministic dependency-first publication order.
- [x] Explain each transitive package inclusion and requirement change in human and JSON output.

**Exit criteria:** Multi-level version and dependency ripples are complete, minimal, explainable, and reproducible.

### Phase 5 — Stateless prepare and verify

**Goal:** Safely materialize and prove the reconstructed release state.

- [x] Add `clippier release prepare` with package selection and `--dry-run`.
- [x] Use structured TOML editing that preserves repository formatting conventions as much as practical.
- [x] Correctly edit package versions and every supported dependency table/form.
- [x] Apply related manifest changes atomically or fail without leaving a partially prepared workspace.
- [x] Update `Cargo.lock` through Cargo-aware behavior.
- [x] Add dirty-worktree and concurrent-change safeguards appropriate to local and CI usage.
- [x] Make repeated preparation a no-op once local versions and requirements are sufficient.
- [x] Add `clippier release verify` that reconstructs release analysis and rejects under-bumped, stale, inconsistent, incomplete, or over-selected states where correctness requires rejection.
- [x] Validate Cargo metadata, dependency resolution, and package construction.
- [x] Preserve existing low-level `version` behavior for compatibility.

**Exit criteria:** Preparation produces only necessary source-controlled changes, and verification proves the current repository state is publishable without a state file.

### Phase 6 — Stateless publication

**Goal:** Publish exactly the currently pending, verified release closure locally or in CI.

- [x] Add `clippier release publish` using existing publication machinery.
- [x] Reconstruct pending packages from local versions, normalized package contents, dependency requirements, and crates.io state.
- [x] Support selected roots or all pending packages.
- [x] Include mandatory pending dependencies automatically.
- [x] Re-run release verification immediately before publication.
- [x] Preserve dependency ordering, temporary sanitized packaging, category checks, registry waits, retries, dry-run, and already-published skipping.
- [x] Preserve useful per-package status and failure summaries.
- [x] Resume correctly after partial publication by reconstructing registry state.
- [x] Preserve the existing `clippier publish` interface or delegate it compatibly without surprising existing users.

**Exit criteria:** The same repository checkout can be safely published locally or through CI without any generated state file.

### Phase 7 — Clippier GitHub Action integration

**Goal:** Make the release domain easily consumable across Rust repositories.

- [x] Add release status, prepare, verify, and publish commands to the composite Action.
- [x] Add focused inputs for package selection, dry-run, workspace path, and release configuration only where defaults are insufficient.
- [x] Add outputs for ephemeral JSON status, changed/pending package lists, whether work exists, and publish order.
- [x] Extend Clippier source hashing, feature selection, binary caching, diagnostics, and summaries to release commands.
- [x] Ensure outputs are deterministic and suitable for follow-on jobs without making them authoritative persistent state.
- [x] Avoid expanding broad duplicated Action command-condition lists when a centralized command capability check fits existing conventions.
- [x] Ensure external Rust repositories can consume the Action with minimal wrapper YAML.
- [x] Document default behavior, permissions, secrets, and optional configuration.

**Exit criteria:** Other Rust repositories can invoke complete Clippier release functionality through the Action without reimplementing release logic.

### Phase 8 — GitHub workflows

**Goal:** Provide manual push-button preparation and publication.

#### Prepare workflow

- [x] Add a manually dispatched reusable preparation workflow.
- [x] Accept optional package roots; blank means all packages currently requiring release.
- [x] Check out sufficient repository history for optional Git prioritization while retaining full registry-backed correctness.
- [x] Run release status, prepare, and verify.
- [x] Exit successfully with a useful summary when nothing needs release.
- [x] Commit only reconstructed manifest/lockfile changes to a managed release branch.
- [x] Create or update a single managed release PR instead of creating duplicates.
- [x] Generate the PR explanation from a fresh status calculation; do not commit a plan.
- [x] Use existing affected-package workflows to validate changed packages.
- [x] Keep crates.io publication credentials unavailable to this workflow.

#### Publish workflow

- [x] Add a manually dispatched reusable publish workflow intended for the protected default branch.
- [x] Accept optional selected package roots or publish all currently pending packages.
- [x] Re-run status and verification immediately before publishing.
- [x] Add repository-scoped concurrency so two publishers cannot operate simultaneously.
- [x] Use a protected GitHub environment when repository policy requires approval.
- [x] Support crates.io trusted publishing when configured and token-based publishing as the documented fallback.
- [x] Produce per-package and final workflow summaries.
- [x] Replace the current MoosicBox `publish-crates.yml` implementation with a thin consumer of the reusable release workflow.
- [x] Provide boilerplate-minimal examples for other Rust repositories.

**Exit criteria:** Preparing and publishing a correct release are push-button GitHub operations, while local publication remains equivalent.

### Phase 9 — Documentation and compatibility

**Goal:** Make the stateless release system transparent, predictable, and adoptable.

- [x] Document repository/registry source-of-truth behavior and why no release plan is needed.
- [x] Document direct release roots versus mandatory transitive closure.
- [x] Document version calculation and pre-1.0 Cargo compatibility.
- [x] Document the upstream `cargo-semver-checks` default feature heuristic and limitations.
- [x] Document the independent-version migration and its one-time MoosicBox application.
- [x] Document local status, prepare, verify, and publish flows.
- [x] Document GitHub Action and reusable workflow integration.
- [x] Document recovery from stale preparation, registry races, and partial publication.
- [x] Document packages without library targets and any supported overrides.
- [x] Preserve and document compatible behavior for existing Clippier version/publish users.
- [x] Use full dependency versions in documentation examples as required by repository guidelines.

**Exit criteria:** A Rust repository can adopt the pipeline with minimal boilerplate and understand every automatic decision from command/workflow output.

## Product completion verification

- [x] A source-only change to a leaf crate is detected from normalized package contents and receives the smallest sufficient release.
- [x] A compatible public API addition receives the correct Cargo-compatible release.
- [x] An incompatible public API change receives the correct Cargo-compatible release, including pre-1.0 cases.
- [x] A breaking internal dependency release updates incompatible requirements and ripples through all necessary consumers.
- [x] Compatible dependency releases do not cause unnecessary consumer releases.
- [x] Explicit package selection includes mandatory closure but excludes unrelated packages.
- [x] Never-published packages receive sensible initial treatment.
- [x] Packages without library targets receive the documented deterministic treatment.
- [x] Shared-to-independent migration does not make every package appear semantically changed.
- [x] Re-running `release prepare` after successful preparation is a no-op.
- [x] Local and GitHub status produce equivalent deterministic decisions from the same checkout, configuration, lockfile, and registry snapshot.
- [x] `release verify` catches under-bumped packages, incomplete requirement propagation, invalid packaging, and registry races.
- [x] A partial publish rerun safely skips completed versions and publishes only the remainder.
- [x] Existing affected-package build-and-test workflows run for generated release PRs.
- [x] Focused Clippier integration fixtures cover registry/content normalization, semver adapter behavior, dependency propagation, preparation idempotence, and publication reconstruction without creating a separate planner test suite.
- [x] Run repository-required Rust formatting across the workspace.
- [x] Run focused Clippier tests and relevant integration tests.
- [x] Run Clippier build/check with release-related features.
- [x] Run warning-denying Clippy validation for affected targets/features.
- [x] Validate Action and workflow syntax with repository tooling when available.

## Blockers and open questions

- [x] Confirm crates.io trusted publishing availability and configuration for each consuming repository; token publishing remains a required documented fallback.
- [x] During implementation, select and document the exact deterministic default for changed packages without a checkable library API.
- [x] Determine whether normalization can be implemented solely over `cargo package` archives or requires a Clippier-owned canonical package representation; choose based on focused fixtures and Cargo output evidence.
- [x] Confirm whether release verification should reject unnecessary over-bumps or merely report them. It must always reject insufficient bumps.
- [x] Confirm managed release branch naming and whether existing PR-update conventions in target repositories impose additional constraints.

These are implementation/repository questions unless they require a product-policy choice. Research them before asking the user where codebase behavior can resolve them.

## Session handoff notes

- The original request prioritizes correctness, very low friction, robustness, and reuse across Rust repositories.
- “Automatic” means Clippier automatically calculates and applies correct versions after an explicit command/workflow request. It does not mean releases run automatically on push.
- The user explicitly rejected committed release state and broad shared-version releases.
- The current shared MoosicBox version exists to simplify manual publishing; this effort should make independent versions simpler and more correct, enabling its removal for publishable packages.
- The user accepted `cargo-semver-checks`' upstream feature heuristic after clarification because it is upstream deterministic behavior, not a new Clippier invention.
- Do not exact-pin `cargo-semver-checks`. Follow normal root workspace dependency conventions with the latest stable full version at implementation time.
- Do not add a duplicate package test workflow or a standalone planner-unit-test project. Use existing affected-package CI plus focused release integration coverage.
- Do not begin implementation merely because this progress document exists. Resume by researching the current phase and reconciling it against both product closure and architectural integrity.

## Update rules for future sessions

- Reconcile completion against the original product outcome, product closure, and architectural integrity—not checkbox count alone.
- Preserve repository and registry state as the only canonical release sources of truth.
- Never add a committed or required generated release plan as a shortcut.
- Never accept whole-workspace version bumps as the completed solution when independent package versions are required.
- Never defer required dependency propagation, verification, registry-race handling, or publication recovery as optional cleanup.
- Update codebase findings and locked decisions when concrete repository evidence changes.
- Keep `cargo-semver-checks` behind Clippier-owned internal boundaries.
- Preserve narrow Cargo/crates.io product scope while completing every required integration layer.
- Avoid speculative generalized release infrastructure for other ecosystems or registries.
- Let existing affected-package workflows own package build/test coverage, but retain focused integration validation for release-domain correctness.
- Keep this file local unless the user explicitly decides to commit it.
