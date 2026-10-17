# Egui replacement: orchestration and acceptance plan

## Goal and current status

Replace v1 and v2 with one backend, not a third experimental renderer. Public
renderer/navigation contracts remain compatible; version-selection features and
legacy source are removed only after the replacement passes the gates below.

This is a working implementation plan, **not a claim of comprehensive support**.
The coordinator resumed continuous implementation/review/validation after the user
rejected stopping at the foundation checkpoint. The C7 basic native fixture and
157-library/10-feedback-test checkpoint are historical foundation evidence only;
strict lint, full runtime integration, shared layout support, expanded native
scenarios, actual MoosicBox acceptance and legacy removal remain open.

The corrected text/negative-fixture checkpoint has 157 library tests and ten production-frame feedback
tests passing with `feedback,glow,x11`. Both legacy exports remain selected. The
replacement native example, actual MoosicBox application, other graphics/platform
combinations, and the remaining support gaps still need acceptance. Re-run tests
after subsequent changes; counts describe the checkpoint, not future snapshots.

## Lessons from the old implementations

- Persistent effects cannot live in a newly constructed handler per action.
  Throttle keys must distinguish bindings, and recursive actions must not throttle
  the same admitted effect a second time.
- Layout, painting and hit testing must share geometry. Egui child layout cannot
  independently reposition HyperChad's calculated children.
- Authored numeric/string IDs are selectors, not runtime identity. Duplicate IDs,
  subtree replacement and asynchronous completion require separate opaque keys
  and generation stamps.
- Native editing state belongs to persistent controls. Recreating it each frame
  loses selection/focus and incorrectly resets user edits on unrelated updates.
- A successful empty action branch or missing renderer surface is not support.
  Unsupported behavior must be explicit, and remains a release blocker when in
  the agreed support surface.
- Screenshot existence alone proves neither route readiness nor correct behavior.
  Readiness, expected semantic IDs, immutable baselines, interaction assertions,
  and deliberate negative comparisons are separate requirements.

## Architectural boundaries

```text
Renderer clones -> bounded ordered mailbox -> UI-owned Document
Document + action style projection + widget layout policy
    -> cloned calculated roots -> HyperChad Calculator
    -> revisioned GeometrySnapshot
    -> native painting / accessibility / event targeting
Native input + widget responses -> EventRouter -> persistent ActionRuntime
Async route/image/file work -> stamped completion -> UI owner
```

The UI thread alone mutates document, widget, texture and geometry state. Producers
may enqueue before initialization and request repaint after attachment. There is
one take-once UI bootstrap across facade clones. Queue admission is bounded and
reports overflow; frame draining is bounded separately. Canvas and document
commands share admission ordering rather than racing independent queues.

Geometry owns box projection, scroll offsets/ranges, clipping, containing blocks,
paint order and hit order. Forest paths map calculated nodes to stable keys even
when authored IDs are absent or duplicated. Logical text measurement uses shaped
advances and line boxes, not glyph ink bounds. Shared calculator limitations must
be corrected there or explicitly scoped, never hidden by a second layout engine.

Actions reuse the shared evaluator, with persistent per-binding execution state,
precise receipts for undo, deterministic deadlines and explicit host effects.
Structured event payloads must not prevent actions which do not inspect them.
Native editing/activation is authoritative; raw routing must not duplicate widget
clicks. Gesture ownership spans press/release frames and outside-listener snapshots.

## Lanes and exclusive ownership

| Lane                   | Ownership                                                       | Acceptance evidence                                                                                   |
| ---------------------- | --------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| A: adversarial review  | Read-only cross-component review                                | Concrete repros independently checked after fixes                                                     |
| B: frame/integration   | `backend.rs`, `lib.rs`, feedback/example/tests, egui manifest   | Real Renderer/runner, all adapters wired, same frame used by tests/native app                         |
| C: native environment  | `flake.nix`, egui `scripts/`                                    | Isolated X11/Mesa capture, hashes, readiness, cleanup, matching and failing comparisons               |
| D: document/transport  | `document.rs`, `runtime.rs`                                     | All swaps/fragments/deletes, identity, stale tickets, bounded FIFO and ordered canvas tests           |
| E: geometry/typography | `geometry.rs`, `layout.rs`, `font_metrics.rs`                   | Projection/clipping/scroll/DPI/logical-text regressions; exact upstream gap proposals                 |
| G: event routing       | `events.rs`                                                     | Hover/outside/focus/capture/keyboard/lifecycle/gesture adversarial tests                              |
| H: action execution    | `action_runtime.rs`, shared actions `handler.rs`                | Shared-consumer tests, binding timing, recursive actions, exact undo and payload tests                |
| I: structure/painting  | `widgets.rs`, widget modules except forms                       | Styling/text/semantics, no child reflow, no duplicate labels, disclosure policy                       |
| J: forms/files         | `widgets/forms.rs`, `widgets/forms/`                            | Durable editing, lossless submissions, multi-select/files, native service lifecycle                   |
| K: image resources     | `resources.rs`                                                  | Bounded lifecycle, stale rejection, UI texture ownership, raster/SVG fixtures                         |
| L: canvas              | `canvas.rs`                                                     | Every command, ordered partial clears, alpha, replay/resize and native visual fixture                 |
| R: routing             | `routing.rs`                                                    | Real Router fixtures, methods/body/form encoding, cancellation, ticket completion and metadata policy |
| Coordinator            | App feature cutover, root workspace dependencies, release gates | Reconciled evidence, final feature matrix, no orphan components or outstanding workers                |

Shared calculator/model edits are deliberately not assigned concurrently: E first
identifies exact required changes, then the coordinator assigns a narrow follow-up
lane with shared-consumer validation. No worker edits another lane's files merely
to make compilation pass.

### Scheduling rules

1. Interface consumers read actual published source, not provisional field guesses.
2. Source-ready, compiled, tested, integrated, visually checked and accepted are
   different states. An idle worker is not accepted work.
3. Publish compile-coherent snapshots. Freeze shared APIs during validation; do not
   let a struct migration invalidate another lane's build halfway through.
4. One Cargo slot for this checkout, normally `-j 2`; report command, outcome and
   process settlement before handoff. Independent reads/edits continue in parallel.
5. Retain focused build/test logs under ignored `target/feedback-artifacts/` and
   native artifacts in fresh external directories. Do not generate source-tree
   screenshots/errors or automatically overwrite baselines.
6. Send compile/lint failures to the owning lane, preserving other work. Reviewer
   findings need confirmation against the settled source, not blind application.

## Dependency order and gates

### Gate 0: feedback infrastructure — established, replacement replay pending

Use a private authenticated Xvfb with Mesa software rendering, deterministic fonts,
fresh config/data/cache roots, bounded startup and teardown, and executable hashes.
The launcher must reject stale outputs and missing baselines before app startup.
Require expected semantic IDs and stable geometry/assets; a READY marker does not
mean capture succeeded. Preserve terminal errors and all mismatch artifacts.

Legacy native fixture matching and genuine alternate-render mismatch have been
exercised. They validate the infrastructure, **not the replacement implementation**.
Replacement captures use a new reviewed baseline and repeat the same negative gate.
The first replacement candidate was **rejected**: only the native input and checkbox
were visible; heading/labels/button/link text were missing despite passing semantic
tests. Failed artifact: `/tmp/hyperchad-egui-c5-replacement-candidate-20261017/screenshot.png`.
No replacement visual baseline has been accepted for release. The corrected C6
candidate restored text and matched exactly on repeat capture, but its alternate
background produced identical pixels. Investigation showed a negative-fixture bug:
the alternate changed a wrapper correctly covered by the unchanged opaque child.
Targeting the visible authored root fixed the negative fixture without changing
production stacking. C7 passed: normal matched the reviewed C6 image exactly;
a genuine same-scale red background differed in 298,049 pixels, exited 1, and
preserved expected/actual/diff artifacts and the immutable baseline SHA. Artifacts:
`/tmp/hyperchad-egui-c7-replacement-{match,alternate}-20261017/`.
This accepts only the basic-fixture feedback foundation, not full app support.

### Gate 1: coherent vertical slice — first checkpoint passed, expanded coverage pending

Wire document + geometry + real WidgetPainter + events + actions into the native
host and feedback seam. Test click/held/idle frames, editing, checkbox, navigation,
queues before startup, generations and teardown. The current seven interaction/
comparator tests pass on the replacement path. Keep strict required IDs.

Do not treat minified HTML or explicitly sized fixture controls as a production
whitespace/intrinsic-size policy. Add natural indented HTML and unsized-control
regressions, artwork, canvas, route fragments, nested scroll and overlapping hit
regions before calling this gate comprehensive.

### Gate 2: complete runtime integration — in progress

- Connect route loads/form submit/reset/completions to the real orchestrator and
  stamped document transactions, not a second App navigation consumer.
- Consume all action effects: focus/blur, input selection, navigation, application
  actions, logs, styles and errors; verify actual native memory and teardown.
- Route raw pointer/keyboard/window cancellation with native gesture ownership.
  Physical key codes/IME limitations must be documented and tested where available.
- Run image fetch/decode off paint with time/byte limits, cancellation and bounded
  completion delivery; test local HTTP fixtures rather than external services.
- Poll/dispatch the native file service outside traversal; validate stale results,
  cancellation, read failures and budgets. Interactive portal behavior remains a
  manual platform check, distinct from injectable service tests.
- Apply canvas through the same ordered mailbox at the exact document revision.
- Preserve response metadata or explicitly reject unsupported effects; never
  silently pretend native cookies/title/history behavior exists.

### Gate 3: visual and interaction acceptance — pending

Capture and inspect both a deterministic component gallery and actual isolated
MoosicBox pages. Required scenarios: disconnected startup/settings; navigation and
search; text editing/focus; popovers/outside-click; volume/seek/queue controls;
artwork loading/error; scrolling/resize/DPI; canvas visualization; route replacement
while an old async response is outstanding. Use local deterministic server data
for connected scenarios; do not depend on user credentials or mutate live state.

Each scenario records executable hash, feature set, dimensions/DPI, fonts/software
renderer, semantic geometry, interaction expectations and screenshot artifacts.
Compare twice for repeatability, then use a genuine rendered variant to prove the
comparator fails and baseline bytes remain unchanged. Review intentional visual
changes explicitly. Pixels supplement semantic/event assertions, not replace them.

### Gate 4: single-backend cutover and removal — pending

After Gates 1–3 and review, select the replacement through HyperChad App/native
features, remove both v1/v2 implementations and version flags, update examples and
documentation, and remove legacy-only dependencies. Avoid a permanent compatibility
switch that leaves two maintained rendering engines. Test relevant consumers and
both glow/wgpu plus X11/Wayland configurations; run available platform checks and
state unavailable native platforms honestly.

Run workspace Rust formatting (`cargo fmt`), focused/full practical tests and strict
Clippy. Existing unrelated failures must be separated from regressions. Final
acceptance includes no silent unsupported branches, no source-only orphan modules,
no unacknowledged temporary dependency overrides, and all workers/processes settled.

## Current remaining architectural/support decisions

- Shared typography needs family/weight-aware measurement, and weighted faces need
  registration; nominal support is not enough if painting and measurement disagree.
- Fixed right/bottom resolution, sticky positioning and tables require a shared
  calculator audit and targeted tests; explicit current errors are not completion.
  E's proposed patch split (not implemented):
  - Audit `fixed_positioned_elements[_mut]` in `transformer/src/lib.rs` to select
    `Position::Fixed` rather than `is_fixed()` (which includes Sticky). Existing
    tests explicitly expect Fixed plus Sticky, so review all consumers and update
    the contract/tests deliberately rather than silently changing the helper.
  - In `transformer/src/layout/calc.rs`, resolve fixed flex remaining size and
    positioning width/height against the viewport; retain relative-containing-block
    dimensions for absolute nodes. Test nested relative containers with fixed
    100% dimensions and percentage right/bottom insets.
  - Implement sticky as normal-flow anchor plus nearest-scrollport/inset and
    containing-bound constraints. Keep geometry's explicit rejection until that
    complete pipeline exists; removing the rejection alone is not support.
  - Implement shared table column/row/span sizing with the currently ignored
    regressions; do not substitute an egui grid.
- Text rounding parity is corrected: measurement and painting both disable GUI
  output rounding, with an actual painted-galley regression across fractional
  widths/DPI. Full typography parity still needs the shared family/weight/
  whitespace measurement contract; rich-style galleys are not universally
  reused between measurement and paint.
- Native `Raw` nodes also carry ordinary parser text. The current native policy
  renders their content literally through the text path, without reparsing markup
  or decoding entities again. Embedded HTML interpretation is not supported; keep
  that distinction explicit rather than rejecting all production text.
- Router convenience parsers use maps; repeated form/query values must retain raw
  ordered data or require a shared parser API change. Do not silently collapse them.
- Queue count limits do not bound payload bytes, retained document history or
  transaction-clone cost. Measure representative app updates and enforce budgets.
- Native keyboard physical/IME information may exceed egui's available event data;
  distinguish faithful supported metadata from synthesized guesses.

## Reproducible validation entry points

```sh
cargo test -p hyperchad_renderer_egui --no-default-features \
  --features feedback,glow,x11 --lib --test feedback -j 2
cargo build -p hyperchad_renderer_egui --no-default-features \
  --features feedback,glow,x11 --example feedback -j 2
cargo clippy -p hyperchad_renderer_egui --no-default-features \
  --features feedback,glow,x11 --all-targets -j 2 -- -D warnings
nix develop .#egui-visual-test
python3 packages/hyperchad/renderer/egui/scripts/native-screenshot.py --help
```

See `scripts/README.md` for current launcher flags and capture requirements. A
passing historical log or fixture screenshot is not evidence for a later changed
snapshot; validation and hashes must be refreshed at each acceptance boundary.
