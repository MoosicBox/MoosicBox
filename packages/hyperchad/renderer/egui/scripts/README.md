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
