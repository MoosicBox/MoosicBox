# HyperChad egui: single-backend delivery status

## Current checkpoint — single-backend cutover validated

The requested outcome is one production native renderer, reached through the
normal HyperChad App and MoosicBox native entry points, with HyperChad layout
authority, UI-owned mutable state and nonblocking asynchronous commands. It is
not a third experimental engine or a demonstration-only rendering path.

The source cutover is applied. Crate-root `EguiRenderer` and `EguiRenderRunner`
re-export the types in `src/backend.rs`. Both legacy engine files and all renderer
version-selection features/forwarding are removed; no version facade or no-op
version feature remains. `layout::EguiCalc` and the App's calculator-specialized
renderer alias remain. App construction and the native acceptance example now use
the canonical root renderer. A root/backend type-identity regression was added.

**Post-cutover checks passed at C26**, including the physical-key/IME fix. The
coordinator ran 231 renderer unit tests, 11 feedback tests and one doctest;
all-target strict clippy for native Glow and Wgpu consumers; all five renderer/
platform compile selections; workspace formatting checks; and 19 Python consumer
tests in `nix develop .#egui-visual-test`. Bare system Python lacks Pillow; the
Nix environment is required. Three small lint-only fixes followed the event patch.

Fresh C26 native runs passed the actual App Settings scenario on both Glow and
Wgpu (24 phases/six captures each), and the production-template fixture at DPI
1 and 1.25 (29 phases/eight captures each). No baseline changes or readiness
relaxation were used. Artifacts are retained in:

* `/tmp/hyperchad-egui-c26-glow-settings/`
* `/tmp/hyperchad-egui-c26-wgpu-settings/`
* `/tmp/hyperchad-egui-c26-fixture-dpi1-verified/`
* `/tmp/hyperchad-egui-c26-fixture-dpi125/`

Binaries and validation logs are under `target/feedback-artifacts/c26-*`.
Glow App SHA-256: `8a5550166c8666cb9ac544be7578b98498cf6e93a23cabc2bd161dfd6d4cc66f`;
fixture: `ac266cd533ddb30b9aa5b343874ac49f1aac3770849991baf4e37adb122bbf71`;
Wgpu App: `913b3bce1623a08f61590b91b535d0c88e168eb6cbdbb1e250618760735b46be`.
The Wgpu startup diagnostic confirms renderer Wgpu, llvmpipe CPU adapter and
Wgpu's Gl backend; this is not a hardware-GPU or Vulkan acceptance claim.
Earlier checkpoints below remain historical, not substitutes for these runs.

## Production ownership and completion path

```text
MoosicBox native / HyperChad App builder
  -> canonical EguiRenderer and native runner
  -> bounded ordered document command transport
  -> UI-owned Document + persistent action/event/widget state
  -> HyperChad calculator and shared text measurement
  -> one geometry/clip/scroll projection
  -> native widgets, images and retained canvas painting
  -> native input acknowledgements and asynchronous route/resource completions
```

* `src/runtime.rs` owns command admission/FIFO draining; producers do not perform
  filesystem work or await network completion in the native frame.
* `src/document.rs` reconciles complete views, fragments, deletion selectors and
  swaps transactionally. Element identities/generations and request provenance
  reject stale work rather than applying it to a reused numeric ID.
* `src/action_runtime.rs` and `src/events.rs` retain action execution/event state.
  Native editing and widget activation are authoritative; raw input must not
  duplicate clicks or overwrite widget-owned text editing.
* `src/backend.rs` constructs the production host. The context-setup hook runs
  before font/layout initialization, allowing the same installed font registry to
  serve measurement and native painting. `src/feedback.rs` uses production frame
  integration, not an independent evaluator or alternate layout engine.
* `src/routing.rs` and `src/resources.rs` own asynchronous request/resource
  admission, cancellation/provenance and completion delivery. Document mutation
  remains on the UI side.

## Implemented contracts and source evidence

These are implemented paths, not guarantees that every possible input or platform
has been accepted. Their tests must be rerun after cutover.

| Contract           | Current implementation / regression evidence                                                                                                                                                                                                                                                                                              |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Fixed positioning  | Shared `transformer/src/layout/calc.rs` resolves viewport-relative fixed dimensions/insets; `nested_fixed_percentages_use_viewport_absolute_uses_relative_container` distinguishes absolute containing blocks. egui geometry tests cover fixed children escaping ancestor scroll/clip.                                                    |
| Sticky positioning | `src/geometry.rs` projects shared `StickyAxis`/`sticky_offset` constraints against nearest scrollports and containing bounds, with inherited translations and paint ordering. Tests cover parsed modal headers, nested scrollports, both axes/end insets, clipping and invalid geometry.                                                  |
| Tables             | Shared `transformer/src/layout/calc.rs` table preflight, grid/span sizing, measurement reset and geometry validation are present. Regressions include row-span slot/height allocation, section-boundary rejection and measurement errors. No egui Grid substitutes for shared layout.                                                     |
| Typography         | `src/text_layout.rs` registers family/weight/fallback faces and supplies shared layout jobs/whitespace policy. `src/widgets/text.rs` tests fractional-DPI measurement against actual painted galleys and relative weights; `src/widgets/forms/typography_tests.rs` tests genuine registered faces, inherited styles and native selection. |
| Forms and widgets  | `src/widgets/forms.rs` and its allocation/typography tests implement native editing/state with geometry-constrained allocation. Select/file controls must not be assumed to have universally exact platform chrome metrics.                                                                                                               |
| Images and canvas  | `src/resources.rs` handles image resources; `src/canvas.rs` retains canvas commands for painting. Native fixture acceptance below includes actual draw/clear/replay, not just command storage.                                                                                                                                            |
| Native observation | `src/interaction.rs` provides bounded worker I/O, atomic v1 state/replies, redacted values and correlated screenshot markers. PNG publication precedes its exact-frame reply. The host must issue the screenshot command in the snapshot frame; delivery-frame state cannot substitute for it.                                            |

## Acceptance evidence — pre-cutover only

Coordinator-reported accepted checkpoints at the cutover gate:

* **C19 actual App / Glow:** disconnected Settings scenario passed. The 250-test
  strict checkpoint is pre-cutover evidence; it is not the current test count.
* **C23 DPI 1.0 and C24 DPI 1.25 production-template fixture:** each passed 29
  phases/eight captures. Coverage includes canvas draw/clear/replay, genuine font
  weights 300/400/700, modal overflow and sticky controls, outside close and reopen.
  This fixture runs through App integration but is distinct from ordinary Settings
  acceptance; neither proves connected-service operation.
* **C24 actual App / Wgpu artifact:** passed 24 phases/six captures. Its feature
  tree contained Wgpu and no Glow. **Adapter identity was not logged**: the feature
  tree does not establish a GPU adapter, hardware/software status or graphics API.
* **Five compile configurations:** Glow/X11, Wgpu/X11, Glow/Wayland,
  Wgpu/Wayland and combined features passed before cutover. This is compile
  coverage and renderer isolation, not single-window-system dependency isolation.

Acceptance artifacts and launcher details belong with their immutable binary
identity and fresh output directory. See `scripts/README.md`, the existing
Settings/production-template plans, and `target/feedback-artifacts/matrix-*` logs.
Do not replace baselines or reuse an artifact as proof for a changed binary.

## Graphics and platform boundary

The renderer chooses Wgpu when its feature is enabled, otherwise Glow. Both
features deterministically select Wgpu. Defaults include Wgpu, X11 and Wayland;
explicit eframe ancillary features retain fonts/accessibility without its broad
renderer defaults. Optional workspace `egui-wgpu 0.33.3` intentionally matches
`eframe 0.33.3` and restores the already-used Wgpu backend defaults. The user
approved this compatibility exception after verifying crates.io latest stable
`egui-wgpu 0.37.0`; it is not authorization to upgrade the graphics stack.

Accessibility is retained. Upstream `egui-winit 0.33.3` enables
`accesskit_winit 0.29.2` defaults, which enable **both winit X11 and Wayland**.
Cargo feature unification cannot subtract these defaults with a direct dependency.
Do not pursue platform purity through unapproved patches, overrides or accessibility
removal. rfd/portal Wayland dependencies are separately not proof of winit platform
selection.

Native acceptance is Linux/X11. **Wayland native smoke has not run.** Weston
14.0.1 was found in the local Nix store and its help/version inspected, but no
compositor was launched for this work. A separately authorized Wayland smoke can
select a private Wayland socket while unsetting X11 display inputs and verify the
actual native handle. The current observation/input protocol requires an actual
X11 XID and XTEST; it must not claim Wayland interaction support.

## Remaining work and explicit limitations

### Post-cutover gate — completed at C26

Lock reconciliation removed obsolete direct edges without version churn. The
canonical root regression, full renderer suite, strict native-consumer checks,
five compile selections and fresh-binary native scenarios all passed as recorded
at the current checkpoint above. Historical feature trees explain the upstream
platform coupling; these checks do not establish native Wayland support.

### Confirmed boundaries requiring separate product decisions/work

* **Keyboard metadata:** the post-cutover event fix snapshots input before widget
  painting, preserves unambiguous egui physical-key identities and tracks
  composition chronologically across frames (Enabled/Preedit set it;
  Commit/Disabled/focus loss clear it). Native widgets remain the sole text/IME
  editors. New adapter regressions and the router composition guard passed in
  C26's full renderer test run. egui-collapsed numpad/main-row
  identities remain unavailable rather than guessed, and native IME interaction
  acceptance is not established by the existing XTEST scenarios.
* **Document payload budgets:** `runtime.rs` explicitly bounds queued command
  count, not bytes. `document.rs` stages cloned document state for transactional
  updates. Large tree/payload byte/depth admission and worst-case clone cost are
  not proven bounded by queue capacity. Route/image/interaction limits do not
  establish a general document-ingestion budget. Measure/enforce at that boundary
  rather than claiming all payloads are bounded.
* **Repeated query values:** `routing.rs::route_request` explicitly returns
  `RepeatedQueryKey` because Router's map API cannot represent duplicates. Query
  pairs and body bytes remain in `RequestContext`; no silent overwrite is allowed.
  Supporting duplicate GET keys needs a shared Router contract change. This is
  not a blanket claim that repeated POST/form body values are discarded.
* **Raw text policy:** `widgets/dispatch.rs` handles `Raw` and `Text` literally
  because parser ordinary text also uses Raw nodes. It does not reparse embedded
  markup or decode entities again. Embedded HTML execution is not a native feature.
* **Broader native acceptance:** connected services, interactive file portals,
  exhaustive control chrome metrics and other OS platforms are not established by
  the listed X11 scenarios. Do not infer universal support from element dispatch
  coverage or compile success.

No new code, dependency changes, compositor launches or baseline updates are
implicitly authorized by these remaining items.

## Reproducible targeted validation entry points

Run only in the assigned Cargo slot/environment; these are handoff commands, not
results from this documentation update. After deliberate lock reconciliation:

```sh
cargo test --locked --offline -p hyperchad_renderer_egui --no-default-features \
  --features feedback,glow,x11 --lib --test feedback -j 2
cargo test --locked --offline -p hyperchad_renderer_egui --no-default-features \
  --features feedback,glow,x11 --doc -j 2
cargo clippy --locked --offline -p hyperchad_renderer_egui --no-default-features \
  --features feedback,glow,x11 --all-targets -j 2 -- -D warnings
cargo check --locked --offline -p hyperchad_renderer_egui --no-default-features \
  --all-targets --features feedback,wgpu,wayland
cargo check --locked --offline -p hyperchad_app --lib --no-default-features \
  --features egui-glow,x11
cargo check --locked --offline -p moosicbox_app_native --bin moosicbox_app_native \
  --no-default-features --features dev,egui-glow,x11
```

Repeat renderer checks with `feedback,glow,x11`, `feedback,wgpu,x11`,
`feedback,glow,wayland`, `feedback,wgpu,wayland` and
`feedback,glow,wgpu,x11,wayland`; adapt consumer graphics/platform selections.
The native acceptance example is gated on Glow/X11 and is expected to be skipped
in Wgpu-only all-targets checks. Use the documented Nix visual-test environment and
launcher instructions for native runs; compile checks do not start a display.

## Historical summary — 2026-10-17 (not current assignments or gates)

Early C7/157-library/ten-feedback-test checkpoints established the initial frame
harness, not comprehensive support. C13 first rendered disconnected App Settings
at 1000×600 using binary SHA-256
`3bda3b58108f8871a2dee5927487be40462b696c48dbdf1a4715a43642572b25`,
with artifact `/tmp/moosicbox-egui-c13-initial-20261017/screenshot.png`;
that image alone did not prove subsequent interactions or layout changes.

Later shared typography, fixed/sticky and table implementations superseded the
earlier proposed patches and explicit unsupported-layout errors. Production
modal/canvas/font and real Settings acceptance then supplied the pre-cutover
coverage listed above. The coordinator authorized the final source removal only
after those gates. Legacy diffs contained only exact-hit-testing setup already
present/tested in the surviving backend; no new legacy-only tests were lost.

Obsolete lane assignments, pending foundation gates and unimplemented-layout
proposals have been retired from this active document. They do not describe the
current code. The only current delivery gate is the post-cutover validation above;
separately listed support boundaries are not silently declared complete.
