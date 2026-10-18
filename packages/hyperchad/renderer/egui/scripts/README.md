# Isolated native screenshot

Enter `nix develop .#egui-visual-test`, then run:

```sh
python3 packages/hyperchad/renderer/egui/scripts/native-screenshot.py \
  --kind hyperchad --executable /absolute/path/to/built-hyperchad-example \
  --required-id feedback-submit --required-id feedback-name \
  --required-id feedback-enabled --required-id feedback-next \
  --artifacts /tmp/hyperchad-native-proof

python3 packages/hyperchad/renderer/egui/scripts/native-screenshot.py \
  --kind moosicbox --executable /absolute/path/to/built-moosicbox-app-native \
  --required-id root --artifacts /tmp/moosicbox-native-proof
```

Executables must already be built with the production egui screenshot seam.
This script does not build, substitute a demonstration renderer, or attach to an
existing desktop. `--kind` labels caller intent; it does not authenticate the
executable as that product. Inspect the PNG and app log before accepting proof.
The two captures are separate acceptance requirements.

The feedback example's comparator is enabled with `--feedback-example`; this
never adds legacy capture aliases. Only the agreed production screenshot/ready
names are forwarded. Use `--variant alternate` for a genuine same-scale fixture
variation (rather than editing captured pixels).
Repeat `--required-id` for visible semantic IDs, for example `feedback-submit`,
`feedback-name`, `feedback-enabled`, `feedback-next`. The app's existing `root`
ID encloses its fresh no-connection home page; its presence alone does not prove
interaction correctness or connection state. `ready` is a geometry/assets marker,
not semantic application readiness. Capture success requires this marker and no
`screenshot.error.txt`; failure artifacts are retained and included in metadata.
Pass `--feedback-example --baseline /absolute/path/to/prior-native.png` to invoke
the example's strict comparison and retain actual/expected/diff artifacts.
A mismatch normally returns nonzero even though a native PNG was captured.
For an intentional negative check, add `--expect-mismatch --variant alternate`
against a prior unmodified fixture capture: the launcher requires a nonzero application exit,
explicit comparator mismatch, and readable expected/actual/diff PNGs. It verifies
the expected pixels against the baseline and checks the baseline hash is unchanged.
The launcher rejects baseline paths inside its fresh artifacts and never creates,
updates, or automatically blesses a baseline. This variation changes actual native
fixture background at unchanged scale/dimensions; it does not edit image pixels. The baseline must come from a real
captured frame, not manufactured pixels.

## Launcher contract validation

```sh
nix develop .#egui-visual-test --command python3 -B -m unittest discover \
  -s packages/hyperchad/renderer/egui/scripts -p 'test_*.py' -v
black --check packages/hyperchad/renderer/egui/scripts/*.py
```

These checks cover invalid arguments without side effects, repeatable semantic
IDs/variant parsing, invalid/tiny/uniform image rejection, hash preservation and
owned process cleanup. Generated pixels in these unit checks are **not** native
renderer acceptance or a baseline. Native acceptance still requires inspected
actual rendered frames and deliberate matching/mismatching comparisons.

## Opt-in bounded interaction consumer (v1)

The Rust producer must implement the agreed interaction protocol; a one-shot-only
binary fails closed rather than receiving guessed input. This mode does not use
READY as evidence and never supplies screenshot/baseline aliases.

```sh
python3 -B packages/hyperchad/renderer/egui/scripts/native-screenshot.py \
  --kind moosicbox --executable /absolute/frozen/moosicbox_app_native \
  --artifacts /tmp/fresh-interaction \
  --interaction-plan /absolute/reviewed-plan.json \
  --x11-library /nix/store/explicit-libx11/lib/libX11.so.6 \
  --xtst-library /nix/store/explicit-libxtst/lib/libXtst.so.6 -- serve
```

Library paths must be supplied explicitly; there is no discovery/install fallback.
The launcher checks XTEST, host nonce/PID/window identity and native client size,
then targets only the newly owned private display/window. Input uses XTEST physical
pointer/buttons/keycodes, not XSendEvent or host semantic mutations. X focus is
verified. Printable ASCII text uses the actual X keymap (unsupported levels fail).
No Return/submit key is supported. Save/New/Connect/Select/Play clicks and navigation
outside `/` and `/settings` are refused. Plans remain trusted, reviewed task input:
these guards are not a general policy engine or application/network sandbox.

Plan shape (max 32 phases; this example edits without submitting):

```json
{"version":1,"phases":[
  {"op":"capture"},
  {"op":"click","selector":{"href":"/settings"}},
  {"op":"assert","expect":{"route":"/settings"}},
  {"op":"click","selector":{"tag":"input","name":"name"}},
  {"op":"assert","expect":{"selector":{"tag":"input","name":"name"},"focused":true}},
  {"op":"key","key":"Ctrl+A"},
  {"op":"text","text":"temporary-unsaved"},
  {"op":"assert","expect":{"selector":{"tag":"input","name":"name"},"value":"temporary-unsaved"}},
  {"op":"capture"}
]}
```

Other operations: `key` (Tab, Shift+Tab, Ctrl+A, BackSpace, Escape, Left, Right),
`wheel` with semantic `selector` and signed nonzero `ticks` (max 20). Assertions
support `route`, node `visible`, `focused`, `value`, `scroll_y_min` and
`scroll_max_y_min`. Selectors use exact id/tag/name/href/text/parent matches and
must resolve uniquely, including hidden nodes. Missing/ambiguous selectors fail;
input requires a visible/enabled clipped rectangle. Hidden rectangles may be null.
No-overflow wheel input fails instead of claiming sticky coverage. Sticky-header
proof additionally requires comparing captured header/content geometry and actual
scroll offsets; a wheel event alone is not that assertion. Close/reopen/reset
scenarios need explicit assertions and screenshots, not implicit success.

Wire protocol: env `HYPERCHAD_EGUI_INTERACTION_DIR` and
`HYPERCHAD_EGUI_INTERACTION_NONCE` select fresh `interaction/` under artifacts.
Host atomically publishes `state.json` containing version=1, nonce, pid, window_id,
revision, frame, route, ready, diagnostics, pending routes/resources, client_pixels,
focused_key and nodes. Each node has parent, id/tag/name/href/text, visible/enabled,
rect_pixels, value, scroll_pixels and scroll_max_pixels. Secret/password/file values
must be null/redacted at the producer; consumer never requires those values.
Node keys are generation-qualified; rects are clipped client-relative physical pixels.

Consumer atomically replaces `request.json` with
`{version,nonce,seq,op,after_frame,expected_revision}`. Sequence starts at 1 and
strictly increases. `capture` requires a later settled frame in the same revision;
producer publishes `capture-0001.png` **before** `reply-0001.json` containing
`{version,nonce,seq,status,error,state}`. State is from the exact captured frame.
Errors/stale revisions are explicit; no silent rebase or arbitrary output path.
`finish` uses the same envelope/reply and acknowledges before orderly exit.
Producer must ignore already-consumed requests and repaint/poll while waiting.
Limits: 1 MiB regular non-symlink JSON, 4096 nodes, 64 MiB PNG, ten seconds per
phase bounded by the overall 1..300-second deadline. Duplicate keys/NaN, regressed
frames/revisions, missing semantics, diagnostic states, stale replies and incorrect
capture dimensions fail closed. Per-phase evidence is retained in fixed
`phase-0001.json` files with before/after states; failed evidence is not acceptance.

Producer integration and actual native interaction execution remain required;
mock wire tests prove only the consumer contract. Production-template modal
fixtures with synthetic overflowing data must be labeled fixtures, not real
connected-App acceptance. No test silently creates a backend/server connection.

## Production-template fixture consumer extensions

`production-template.plan.json` is **synthetic local fixture coverage, not connected
MoosicBox acceptance**. It has 29 phases/eight captures and uses the actual native
example/modal template. It does not change `settings-disconnected.plan.json`.
Do not launch until an explicit fresh executable hash/grant is supplied.

Narrow font options (both weights required, only interaction mode):

```sh
--font-regular /nix/store/0lhf2h2fnjqvpzmk92s5axr2ybvs981s-dejavu-fonts-2.37/share/fonts/truetype/DejaVuSans.ttf \
--font-regular-sha256 58568c88a01b80dfc056daca3eeafd434a0380df1a48dda53f0de0d716109bbd \
--font-bold /nix/store/0lhf2h2fnjqvpzmk92s5axr2ybvs981s-dejavu-fonts-2.37/share/fonts/truetype/DejaVuSans-Bold.ttf \
--font-bold-sha256 7c7aef63328a765586cda41bf9d2c227ff1ad82816326dae4d7fe2fd1b7f613a
```

Hashes are checked before artifact creation/launch. Paths and hashes are recorded
in `metadata.json.fixture_fonts`; only `HYPERCHAD_ACCEPTANCE_FONT_REGULAR` and
`HYPERCHAD_ACCEPTANCE_FONT_BOLD` are exported. No arbitrary environment forwarding,
font search/download or dependency install. Weight 300 remains the fixture's bundled
Ubuntu Light; OS/2 weight provenance of the supplied 400/700 files is separate from
visual proof of their actual rendering.

New plan-only operations, unchanged v1 producer wire protocol:

* `{"op":"scale","logical_client":[1000,700]}` checks actual physical client
  dimensions against requested `--scale` (one-pixel rounding tolerance), recording
  independent effective X/Y ratios. Run scale 1 and 1.25 separately on new private
  displays and artifact directories. Host/native geometry and screenshot dimensions
  must still agree. This proves client-size ratio, not every internal font metric.
* Structural selectors `{"scroll_ancestor":{"id":"modal-first-row"}}` and
  `{"button_within":{"id":"acceptance-modal"}}` resolve exactly once from
  the current generation-qualified tree. A unique ancestor must actually have
  positive vertical overflow. No fallback IDs/paint-order indices or fixed points.
* `{"op":"sticky","before":5,"after":6,"header":{"id":"modal-heading"},
  "content":{"id":"modal-first-row"}}` compares exact prior capture states.
  The same-generation header stays visible/stationary within one physical pixel;
  actual ancestor offset increases by more than one pixel; content moves upward or
  becomes clipped. Zero overflow, stale identities/native dimensions, unchanged
  content and moved header all reject. Evidence records actual scroll delta.
* `{"op":"outside","selector":{"id":"acceptance-modal"},
  "content":{"id":"modal-first-row"}}` chooses the center of a positive free
  strip inside the current overlay and outside its actual scroll panel. Containment
  is verified before XTEST click; no available outside area fails. Template-button
  close and outside close are each followed by hidden-state assertions; reopen is
  separately asserted/captured. The selector never mutates host semantics.
* `{"op":"pixels","before":2,"after":4,"selector":{"id":"acceptance-canvas"},
  "equal":true}` decodes both fixed numbered captures to RGBA and compares only
  the same-generation canvas's clipped physical rectangle, inward-rounded to whole
  pixels. Bounds/geometry/frame/native identity must agree. Draw equals replay,
  clear differs from draw, initial equals clear. Control focus/hover is outside the
  crop. Evidence records crop and RGBA hashes. This is a comparison of immutable
  run captures, **not** a baseline update/approval. Wrong equality assertions fail;
  tests also deliberately perturb an inside pixel to verify rejection while an
  outside focus-like pixel does not affect the comparison.

Wheel completion now requires the selected same-generation scroll target to move
in the requested direction by more than one physical pixel, followed by **two
 distinct settled frames** with identical revision/window/client/tree state. Re-reading
one frame does not count; readiness loss or changed offset restarts the two-frame
check. The fixture wheel declares `"settle_frames":2`; no effect, saturation,
identity change or deadline exhaustion fails closed. Capture rejection remains
terminal: there is no stale-request retry/rebase. This prevents a later but still
pre-wheel ready snapshot from authorizing capture (C22's frames 61→62 had offset 0,
while frame 63 first exposed offset 192 at a new revision).

The fixture label, modal header/content geometry, font glyphs/weights, canvas draw,
clear, replay and close/reopen images still require real visual review. Passing
mock unit contracts never substitutes for native captures. Pending async producer
work must satisfy the existing readiness gates; the consumer does not sleep-and-
assume success or weaken diagnostics to make fixture comparisons pass.

## Production contract

* `HYPERCHAD_EGUI_SCREENSHOT` is the absolute destination PNG.
* `HYPERCHAD_EGUI_SCREENSHOT_EXIT=1` requests clean exit after successful capture.
* Request `egui::ViewportCommand::Screenshot` only after production routed
  content/layout has been painted. Handle `egui::Event::Screenshot` on a later
  update, save the returned pixels (prefer atomic publication), and surface errors.
* A window existing or a fixed number of blank frames is not content readiness.
  The launcher rejects missing, invalid, tiny and uniform PNGs, but cannot prove
  semantic readiness from pixels alone.
* Adjacent `screenshot.json` is recommended for renderer/backend, content-ready
  generation, painted frame count, dimensions and scale. It complements, rather
  than replaces, the launcher's `metadata.json`, GL probe and executable hash.
* The hidden eframe `__screenshot`/`EFRAME_SCREENSHOT_TO` facility is not used:
  eframe 0.33.3 only implements it for Glow, not WGPU.

## Isolation and artifacts

Each invocation refuses an existing artifact directory and creates a fresh
HOME, XDG config/data/state/cache/runtime/temp and `MOOSICBOX_CONFIG_DIR`. The app
must honor that config-root override before logging/state initialization.
Inherited app credentials, DISPLAY, Wayland, D-Bus and audio endpoints are not
forwarded. Arguments are explicit and must not point at real persisted state.
The application runs from its fresh state directory, not the source tree.

Xvfb uses a randomly selected high display, an authentication cookie, disabled
TCP, and its readiness pipe; an occupied display fails closed. Neither `:0` nor
`wayland-1` is used. Mesa must identify as llvmpipe. Fontconfig lists only the
shell's pinned DejaVu fonts (egui's embedded fonts remain controlled by the app).
The display and application have separate process groups; success, failure,
timeout, SIGINT and SIGTERM clean up owned groups. State/logs remain for audit.

This is configuration/display isolation, **not an OS filesystem/network sandbox**.
Explicit executable/arguments remain trusted. Do not use bundled-server or
discovery-enabled builds without auditing their network/port/audio side effects;
HOME/XDG isolation does not block those. A persisted production connection must
never be copied into the test root.

Artifacts: `screenshot.png` on success, `app.log`, `xvfb.log`, `x11.txt`,
`graphics.txt`, `fonts.txt`, `metadata.json`, and `state/`. Overall capture deadline
is 45 seconds by default (`--timeout 1..300`); setup subprocesses have individual
5-second limits and process termination has bounded grace periods. Exit code is
nonzero on failed capture. A generated PNG alone is not product acceptance.

Wayland/Weston coverage is intentionally separate and not yet provided. Never
point this X11 launcher at the user's compositor to simulate Wayland coverage.
