# Norma OS GPUI 0.2.2 patch

## Intrinsic text measurement after wrapping (2026-11-13)

Text layout caching now requires an exact `wrap_width` match. Previously a
MaxContent pass (`None`) could reuse the size of an earlier zero-width wrapped
pass (`Some(0)`), collapsing Markdown list text and its clip to zero width.
The text and color remained intact, so parser or height-only tests missed it.
Nowrap measurement and the existing truncation-width check stay unchanged.

Regression: `cargo test -p app --lib staff_markdown_text_recovers_intrinsic_size`
measures the same production `StyledText` at natural, zero, then natural width.
Before the fix, its final size was 0×466 instead of the initial 173×26.
Native Markdown rendering is also checked by `staff-workflow --visual` in the
desktop E2E module; unit tests alone do not establish native text visibility.

## Shared display clock lifecycle (2026-11-06)

`CVDisplayLinkStop` does not join CoreVideo's IO thread. Releasing the native
link or its callback context immediately can race a final callback. The old
`mem::forget` workaround also retained native objects and unreleased GCD sources
on each window occlusion/screen change.

The compatibility patch follows [Zed's shared display registry](https://github.com/zed-industries/zed/blob/main/crates/gpui_macos/src/display_link.rs):

- One process-owned native link per encountered display ID; never release a
  started link without a CoreVideo join guarantee. Window count does not grow
  this registry. This does retain a clock for a disconnected display ID.
- Window sources subscribe under a mutex. The native callback receives only
  the integer display ID and merges into current subscribers under that lock.
- Lifecycle operations assert the macOS main thread. All CoreVideo calls occur
  outside the registry lock to avoid a callback lock-order deadlock.
- Start/stop are idempotent; failed starts remove their subscription. Last
  subscriber stops the clock. Other windows on that display keep receiving frames.
- Drop removes the subscriber before cancelling, balancing the GCD suspension,
  and releasing the window's dispatch source. A late callback cannot reach it.
- Occlusion and synchronous paints pause/resume the same window source. Screen
  changes move it to the new display. Generation checks prevent a nested paint
  from undoing a newer visibility/screen transition.

No GPUI version or minimum macOS version changes. Native-object retention is
bounded by encountered display IDs, not by window operations. This does not
claim that all application heap/GPU allocations are leak-free.

Validation: `python3 scripts/test-display-link-lifecycle.py` compiles this exact
production module into a native main-thread probe, checks two simultaneous
subscriptions (including dropping running windows), then uses `heap` before/after 64 lifecycle cycles plus a final
pair of windows. The original implementation grows CVDisplayLink 2→68,
GCD sources 11→143 and Mach ports 2→68. The patched implementation remains at
1 native link, 8 GCD sources (including system sources) and 1 Mach port.
Source rules and type-aware lint: `python3 scripts/check-resource-lifecycle.py`
and `cargo lint-resources`.

## Fixed-layout scene presentation (2026-09-30, feasibility work)

`scene(id, logical_size, child)` fills a parent viewport and presents its child
with positive uniform scale and translation. It keeps the child's layout and
content mask in local coordinates. Cached views retain original paint commands;
replay transforms GPU primitives and clipping without raster readback. This
initial path transforms primitive geometry on the CPU before normal GPU drawing;
its frame cost still needs the unified-scene plan's Release measurements.

A stable transform reference shared by paint commands, hitboxes, debug selectors
and mouse listeners keeps cached children aligned when their parent moves.
Pixel scroll deltas are inverted with the scale; line deltas keep their semantic
units. Deferred drawing retains its source scene and local clip. Normal elements
outside a scene retain their existing coordinate and culling behavior.

Focused verification (pure unit tests, not native UI acceptance):

```sh
cargo test --manifest-path vendor/gpui-0.2.2/Cargo.toml --locked --lib scene_ -- --nocapture
```

The vendored crate is excluded from the app workspace so this explicit command
uses its own lockfile without adding upstream tests to app-wide test runs.
Four unit tests check retained paint commands, parent camera movement, nested
clips, submitted solid quads, mouse inverse mapping and re-entry after culling.
Platform input handlers retain the same live transform: IME candidate bounds and
selected bounds project to window coordinates, while character lookup uses the
inverse mapping and rejects points beyond the clip. The input tests register
through Window::handle_input, replay a cached child under a moving camera, and
verify unchanged behavior for ordinary inputs. They use TestAppContext and do
not establish real macOS IME behavior.
The old `crates/e2e` suite is disabled at the user's request; its results are
historical only. End-to-end acceptance uses Computer Use on an isolated real App.
Deferred drawing and IME still need real-App evidence. Opacity (including video),
native Browser composition, all primitive kinds, text sharpness, real card
integration and performance are still required before adopting this path in the
workbench. This patch is not a claim that G1 has passed.

## Native view scene presentation (2026-10-01, pending real-App acceptance)

`Window::on_scene_presentation` registers native content during paint. After the
frame finishes, its callback receives the latest accumulated scene transform,
clip, opacity and unchanged logical viewport. Cached paint replay retains these
registrations; removing a registration delivers `None` once to hide its view.
The App's Wry/CEF adapters use an AppKit clipping host to scale native content
without resizing the web document on every camera frame. Desktop detachment
preserves the webview and releases only its own native input responder.

`scene_native` unit coverage checks parent camera updates without child repaint,
clipping, opacity and removal notification. App tests exercise production scene
registration and preserved Browser sessions. These tests do not establish native
AppKit/Metal ordering, live web content scaling, menu occlusion or real input;
those remain G1/G4 acceptance work.

## Bounded Metal frame diagnostics (2026-10-01)

Set `GPUI_FRAME_TIMINGS=1` and enable the existing logger's `gpui_frame` info
target. Disabled by default; capture ends after 300 seconds or 36,000 submissions.
No pixel readback, trace buffer, file writer, retained drawable, or extra GPU wait
is added. CPU-only benchmark runs must leave this option disabled.

Records are joined by process ID and submission ID. `submit` reports GPUI
`Window::draw` wall time plus Metal encoding/submission wall time, pixel size and
backing scale. The content build is consumed once: a replay of an unchanged
scene reports `build_ms=None`, not the previous frame's work again. This CPU
scope excludes earlier application update callbacks and is not input latency.
`gpu` reads `GPUStartTime`/`GPUEndTime` from the completion handler; missing or
invalid timestamps remain `None`. `present` uses the drawable's actual
`presentedTime`, not the time the callback arrives. Zero/missing presentations
must remain unverified. Callback ordering is unspecified.

Do not count idle gaps as missed frames. Evaluate presentation intervals only
over independently identified continuous animation intervals and keep capture
and observer overhead visible in the report. The records alone do not establish
application-attributable stalls, input latency, or completion of a performance gate.

Focused arithmetic validation:

```sh
cargo test --manifest-path vendor/gpui-0.2.2/Cargo.toml --locked --lib frame_timing::tests -- --nocapture
```

## Public `DispatchEventResult` (2026-10-19)

Upstream exposes `Window::dispatch_event` as `pub` but returns the
`pub(crate)` `DispatchEventResult`, so no downstream crate can call it. The
struct is now `pub` with its fields unchanged. Only the debug-build desktop E2E
control channel (`crates/app/src/e2e_control/`) uses it to inject mouse and
scroll input into its own window; release builds do not compile that caller.

## `Window::set_active_for_e2e` (2026-10-21)

Adds a public method that marks the window active (and runs activation observers
on the first change) without activating the application or changing the AppKit
key window, then refreshes. The focus path is only established for active
windows, so injected keystrokes would otherwise miss focused inputs while the
isolated E2E window stays in the background. Only the debug-build desktop E2E
control channel (`crates/app/src/e2e_control/`) calls it; a later real
key-status change still overwrites the flag.

## `Window::set_inactive_for_e2e` (2026-10-24)

The inverse of `set_active_for_e2e`: marks the window inactive (running the
activation observers on the first change) and refreshes, so the desktop E2E
control channel can simulate the user leaving and coming back without touching
the OS foreground. Only the debug-build control channel
(`crates/app/src/e2e_control/`) calls it.
