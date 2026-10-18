# HyperChad bmux renderer

A terminal-native renderer for canonical HyperChad `Container` trees, using
published bmux alpha.4 layout/control/runtime primitives. It supports standalone
terminal ownership and host-driven embedding. It is not a browser engine.

## Use from HyperChad applications

Enable `hyperchad` features `app` and `renderer-bmux`; add
`renderer-bmux-images` for all terminal image protocols. When depending directly
on `hyperchad_app`, the corresponding features are `bmux` and `bmux-images`.
Use full release versions in external manifests; this workspace uses
`workspace = true` dependencies.

An existing `AppBuilder` with its real `Router` can select:

```rust,ignore
// `builder` is the application's configured hyperchad_app::AppBuilder.
let app = builder
    .with_initial_route("/")
    .build_default_bmux()?;
// Or build_bmux_with_config(config) for explicit host/resource policy.
```

`build_default_bmux`/`build_bmux_with_config` attach local router transport and,
with the `assets` feature, explicitly registered static assets (memory contents or
bounded registered file/directory readers).
`build_bmux(renderer)` accepts an already configured renderer; it does not install
that transport on your behalf. Static-output generation is unsupported.

## Actual native-app acceptance (not fixture certification)

The acceptance lane adds independent tests without changing production policy:

```sh
cargo test -p moosicbox_app_native --no-default-features --features bmux --test bmux_actual_app
cargo test -p hyperchad_renderer_bmux --test native_assets
python3 packages/app/native/tests/test_bmux_pty_acceptance.py
python3 packages/app/native/tests/bmux_pty_acceptance.py \
  /absolute/path/to/moosicbox_app_native --protocol kitty --artifacts /tmp/bmux-native-kitty
# Repeat with --protocol text, --protocol sixel, and --protocol iterm2.
# These emulate capabilities and decode artifacts; no physical-terminal claim.
```

The PTY script never builds. Supply a separately built real native executable; it
records its SHA-256. It starts `serve` with fresh HOME/XDG directories, an allowlisted
environment, no inherited app configuration, and an ephemeral loopback-only mock.
It reconstructs terminal cells, navigates the actual Settings link by keyboard,
creates/edits/saves/selects a connection by mouse, navigates Albums, checks mock
artwork fetches and decoded Kitty RGBA quadrants (or Sixel/iTerm2 pixel colors),
resizes, and checks clean shutdown
and terminal-mode restoration. It saves terminal bytes, final cells, mock requests,
artifact metadata, and isolated app logs. The VT recorder is a deliberately limited
acceptance model, not a complete terminal emulator or screenshot comparison.

`bmux_actual_app` uses the production default builder, real home/settings handlers,
in-memory SQLite, local form transport, registered PNG/SVG assets, and the native
artwork policy with a real local TCP resource server. `native_assets` checks all
45 shipped SVGs, nontransparent/white icon pixels, exact PNG fixture pixels, and
clipped image placement/allocation. These tests do not certify audio playback,
provider authentication, arbitrary browser scripting, or a full production server.
Native Settings `hx-*` connection forms and app custom actions are required native
workflows, not browser-only unsupported forms.

Current checked suites pass: full renderer (179 unit tests, 10 integration tests,
1 doctest), native tests including actual-app host integration, 25 focused bridge
checks including runtime ownership/cancellation, and the coherent resource-snapshot
regression. Strict `cargo clippy --all-targets -- -D warnings` passes for the renderer
and for native with `--no-default-features --features bmux`.

The actual-executable PTY audit passes all four emulated protocols (Kitty, text,
Sixel, iTerm2), with unchanged normal 15-second waits, against SHA-256
`31c3cf59ec044baabc5e08185b5a04fe430b8ace08b68376fb539a2476fc39bf`.
This covers home/sidebar glyphs, keyboard Settings navigation, connection creation,
editing/saving/selection, actual Albums HTTP response and artwork, resize, clean
shutdown and terminal restoration. Kitty checks ordered quadrant pixels in the
presenter's resampled/padded RGBA texture and white sidebar icon placements;
Sixel/iTerm2 decode artwork colors and sidebar placements; text checks absence of
graphics. Assertions wait for artwork presentation, not just request receipt or an
earlier icon. Evidence is under
`/tmp/bmux-complete-acceptance-{kitty,text,sixel,iterm2}-app-runtime` on the audit machine.

The real loading Albums tree contains 115 image nodes but only 15 unique sources,
including 100 identical placeholders. Ordered semantic effects now bypass queued
resource work; fragment replacement removes stale placeholder jobs. Local native
route handlers run on their owning app runtime, not the terminal executor. These
fix the previously reproduced image-backlog delay and cross-runtime HTTP hang.
This is focused actual-app acceptance of the named artifact, not physical-terminal,
playback, provider-authentication, production-service or later-source certification.

The actual MoosicBox native application has a `bmux` feature and initial `/` route:

```sh
cargo run -p moosicbox_app_native --no-default-features --features bmux -- serve
```

This selects the real application, not the fixture below. It still needs the
application's normal service/configuration dependencies. Live audio playback,
remote library connectivity and production authentication have **not** been
verified by the renderer's fixture/PTy tests.

The native `bmux` build writes logs to `moosicbox_app_native.log` (set
`MOOSICBOX_LOG_DIR` to choose its directory), not stderr, to avoid corrupting the
full-screen display. Image sources without an authorized host policy retain text
fallbacks. Failed fetches/decodes are not retried on unrelated view refreshes;
change the source or explicitly call `retry_resource` to retry.

## Standalone and embedded examples

```sh
cargo run -p hyperchad_renderer_bmux --example workflows -- dashboard
cargo run -p hyperchad_renderer_bmux --example workflows -- documents
cargo run -p hyperchad_renderer_bmux --example workflows -- music
cargo run -p hyperchad_renderer_bmux --example embedded_host
# No terminal graphics dependencies in the presenter:
cargo run -p hyperchad_renderer_bmux --no-default-features --example workflows -- music
```

The workflow example uses `BmuxRenderer::new(config, request_action, on_resize)`,
the HyperChad `Renderer` trait and `ToRenderRunner`. It has editable fields,
checkbox/select/button, notes, navigation, artwork, a raster chart, rich table
cells and spans. Navigation switches fixtures through the real navigation channel;
periodic partial refreshes preserve local edits. Dashboard metrics, document
content and music rows are deterministic fixtures, **not the MoosicBox database,
player or service integration**. The only authorized resource is the exact
`memory:demo-cover` identity, generated in memory. No external network/file I/O is
performed. `--smoke` requests orderly shutdown after three seconds.

`embedded_host` transfers the sole mutable owner through `into_embedded`, drains
messages, prepares a local size, paints into a translated/clipped `PaintCx`, then
commits its ticket. Commit only after enclosing host output succeeds; discard on
failure/cancellation. Stale tickets are rejected. The host owns input arbitration,
command/subscription scheduling, image compositing from `Frame::images()`, output
and cleanup. The example uses an in-memory surface, not a competing terminal
writer. A live host must execute returned effects; simply dropping updates is only
appropriate for this effect-free example.

## Authorized transport and resources

`BmuxConfig.host.request` is an optional asynchronous `HostCallbacks` function
returning `HostResponse`; absence denies host operations. Requests distinguish
`Resource`, `ChooseFile`, `ExternalLink`, `Http` and `Cookies`. Responses include
bounded bytes, authorized file references, HTTP status/body, views, and `Route`
responses retaining status/headers/optional view/body. `HostFuture` returns
`Result<HostResponse, renderer::Error>`.

Configure callbacks with `build_bmux_with_config` before installing the app bridge.
The bridge handles app-local route requests once through the real Router, retains
request/target/strategy correlation and a bounded app-local cookie jar, and forwards
operations outside its scope to your callback. Local relative request targets are
not permission to fetch arbitrary absolute URLs. Exact registered memory-asset
paths use their registered bytes; query/fragment stripping is supported. Traversal,
encoded separators, double encodings, controls and backslashes fail closed.
Filesystem/directory asset registrations **do not authorize filesystem access**.

A custom callback should match exact application-authorized resources/methods and
reject everything else. For HTTP, authorize initial and every redirect URL, ports,
methods, headers, credentials, request bodies and bounded response sizes. Return a
`Route`/`View` only for trusted renderer-neutral responses, not parsed arbitrary
code. For files, explicitly obtain user consent, enforce roots and byte limits,
and return authorized metadata/bytes rather than implicitly opening a path from a
document. For external links, request application approval before launching anything.
For cookie notifications, choose an explicit persistence policy; a local cookie jar
is not permission to leak cookies to remote origins. Never log secrets or raw
untrusted terminal text to the active terminal.

`GraphicsStore` provides async-free RGBA registration, authorized encoded decode,
canvas creation/update and placements from already resolved cell rectangles/clips.
`SourcePolicy` defaults to deny; memory, canonical filesystem roots and exact HTTPS
origins are separate permissions. It never fetches. Reauthorize redirects and avoid
file-open TOCTOU races in your host. `memory:demo-cover` is a demo callback convention,
not a universal URL scheme resolved automatically by `GraphicsStore`.

Default graphics budgets: 4096-pixel maximum dimension, 16 MiB per RGBA resource,
64 MiB resident RGBA total, 128 entries, 8 MiB encoded input, 1024-byte identities,
4096 canvas actions/update, 64 million pixel visits/update. Zero disables the
corresponding budget. Canvases join LRU eviction; handle `Missing` by recreating.
Outstanding scene payloads consume additional caller-owned memory. Decoder scratch
limits are best-effort for some codecs: hard hostile-decoder process-memory limits
require isolation. PNG/JPEG/GIF/WebP decoding uses the first frame, not animation.

## Graphics policy and keyboard operation

`BmuxConfig.image_protocol` selects Auto, Kitty, Sixel, Iterm2 or Disabled;
`graphics_enabled` is an independent gate. Auto uses environment capability hints,
not interactive terminal probes. `HYPERCHAD_BMUX_IMAGE_PROTOCOL` can override protocol
selection. A bmux pane advertises its boundary through `BMUX_PANE` and
`BMUX_IMAGE_PROTOCOL`; physical-host hints must not leak through that boundary.
Build `image-kitty`, `image-sixel`, `image-iterm2` or `all-protocols` as appropriate.
A compiled protocol is not evidence of terminal support. Text fallback remains the
portable path, including after decode when graphics are disabled/unavailable.

Tab/Shift-Tab traverse prepared enabled controls; typing/paste edit text, Space
toggles checkboxes, Enter activates buttons/links or selects popup choices, arrows
navigate native selects/editors, and multiline editors accept line breaks. Password
buffers edit real values but paint masked projections and do not expose secret
values in semantic change events. **Ctrl-C or Ctrl-Q exits standalone mode**, draining
admitted effects and restoring the terminal. Escape remains available to controls.
Embedded hosts own their quit policy; applications can also call
`BmuxRenderer::shutdown`. Input is routed against successfully committed geometry, not failed
presentation candidates.

## Element support matrix

| Canonical elements                         | Terminal adaptation / boundary                                                                                      |
| ------------------------------------------ | ------------------------------------------------------------------------------------------------------------------- |
| Div, Aside, Main, Header, Footer, Section  | Cell layout, nested clipping, overflow, styles; semantic structure rather than browser boxes                        |
| Text, Raw, Span                            | Unicode/grapheme-aware styled inline fragments, wrapping and source ranges; Raw is text, never executable HTML/ANSI |
| Heading, unordered/ordered lists, ListItem | Canonical layout/inline adaptation; not browser font metrics                                                        |
| Anchor                                     | Inline text with native focus/activation metadata, no button chrome; navigation obeys host policy                   |
| Button                                     | Native button activation; submit/reset flow through form semantics                                                  |
| Input text/password/checkbox               | Native persistent editing/toggle state; password masking                                                            |
| Input hidden/file                          | Hidden form values; file chooser requires explicit host authorization                                               |
| Textarea                                   | Multiline native editor; explicit dimensions recommended                                                            |
| Select/Option                              | Single/multiple native selection, disabled options and focus                                                        |
| Details/Summary                            | Disclosure state feeds authoritative layout                                                                         |
| Form                                       | Successful controls, submitter/reset, request serialization including authorized files; no implicit network         |
| Table/THead/TBody/TR/TH/TD                 | Real table allocation, rich inline cells, row/column spans and structural validation                                |
| Image                                      | Bounded decoded RGBA, clipped scene placement, alt fallback; protocol presentation is host-dependent                |
| Canvas                                     | StrokeSize, StrokeColor, Line, FillRect, Clear, ClearRect; bounded raster work and atomic update validation         |

Layout resolves `Number` lengths/percentages, terminal cell metrics, row/column
flow, alignment, border insets/backgrounds, scrolling/clipping and rich inline styles.
Border dimensions affect layout; the current text presenter does not draw general
CSS border strokes, rounded corners or shadows. List containers preserve content
and layout, but browser list-marker/counter styling is not implemented.
Browser typography, arbitrary CSS and subcell hit geometry are not reproduced.
Contain/stretch/centered-cover exist in graphics placement; cover needs actual cell
pixel metrics. Check the app/program's selected fitting policy separately rather
than assuming every image style maps to cover. Hidden/clipped geometry is not an
excuse to omit canonical content or corrupt neighboring rich spans.

## Action support matrix

| Action family                                                                              | Support / explicit boundary                                                                                                                                                                                    |
| ------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Click, click-outside, mouse-down, key-down, hover, change, resize, immediate, named events | Structured event dispatch with source identity/generation and bounded work                                                                                                                                     |
| NoOp, Let, Parameterized, Multi, MultiEffect, Logic                                        | Structured evaluator/action composition, scoped values and resource limits                                                                                                                                     |
| Style                                                                                      | Visibility, display, background and focus; selector validation, retained overrides and scheduling                                                                                                              |
| Input Select, form reset/focus                                                             | Native editor/control effects, not DOM calls                                                                                                                                                                   |
| Navigate                                                                                   | Local router bridge or explicit external-link host policy                                                                                                                                                      |
| Custom                                                                                     | Application action channel; application defines behavior                                                                                                                                                       |
| Log                                                                                        | Structured logging; host must protect active-terminal output and secrets                                                                                                                                       |
| Requests/HTTP lifecycle                                                                    | Authorized host transport; status/view/headers correlation, delayed/debounced actions and lifecycle dispatch                                                                                                   |
| Typed expressions                                                                          | Bounded supported operators, fields, functions/methods and element queries; unsupported constructs return explicit errors                                                                                      |
| RawRust/closures/browser-only executable expressions                                       | Not compiled/evaluated by this renderer; explicit Unsupported errors. A host must deliberately translate such application capabilities into supported Custom/HTTP actions; no implicit arbitrary-code callback |

There is no JavaScript engine, browser DOM, shell execution or automatic audio
playback. Expressions are not a general Rust interpreter. Source admission rejects
unsupported/malformed action structures; lack of an error in a fixture does not
prove arbitrary application expressions are supported.

## Validation and limitations

```sh
cargo test -p hyperchad_renderer_bmux
cargo test -p hyperchad_renderer_bmux --no-default-features
cargo test -p hyperchad_renderer_bmux --test stress -- --nocapture
python3 packages/hyperchad/renderer/bmux/tests/pty_smoke.py /absolute/path/to/workflows --protocol text
# Repeat with kitty, sixel, iterm2 against a matching explicitly built artifact.
```

Focused default/no-default tests have exercised all three fixtures, native keyboard
editing across partial refresh/resize, rich cell/span content, image/canvas metadata,
commit/discard, control-escape sanitization and password non-disclosure. PTY output
checks have passed all three workflows under emulated text/Kitty/Sixel/iTerm2
capabilities, including hostile paste, resize, alternate-screen/terminal-attribute
restoration and Kitty delete commands. Sixel/iTerm2 have no retained Kitty-style
image-ID delete. These results describe built artifacts, not physical terminal
visual verification or proof of a later source revision. Strict lint validation is
separate; no claim of package-wide clean Clippy is made here.

`tests/stress.rs` adds deterministic 1055/4905-node mixed-Unicode tables, 64 rapid
partial replacements and resize. It prints measured layout/update times without
fragile timing thresholds; the new stress case must be run before claiming measured
performance. Table stress is not virtualized-list scaling or a throughput SLA.

Terminal graphics, color, selection/clipboard conventions and cell pixel metrics
vary by emulator/embedding host. Accessibility/semantic metadata is not a browser
accessibility tree. Hosts remain responsible for scheduling, focus arbitration,
transport authorization, output success and cleanup. Large source trees are bounded
but not universally virtualized; deployment-specific limits should be configured
and tested with real application routes.
