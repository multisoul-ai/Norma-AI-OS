# Norma OS GPUI 0.2.2 patch

Norma OS pins `gpui = 0.2.2`. On macOS, that version intentionally calls
`mem::forget` for a stopped `CVDisplayLink` because releasing it immediately can
race its CoreVideo callback thread.

The upstream window implementation also dropped and recreated that DisplayLink
around every synchronous `displayLayer:` paint. The combination retained one
native DisplayLink, Mach port, and run-loop source per repaint cycle.

This vendored patch preserves the upstream Drop behavior and its crash-safety
trade-off. It changes only the two per-paint paths to pause and resume the same
DisplayLink. Real lifecycle transitions such as window occlusion, screen changes,
and window destruction retain the upstream stop/recreate behavior.

`DisplayLink::start` and `stop` track their running state so nested AppKit paints
cannot unbalance `dispatch_resume`/`dispatch_suspend`. The window also records a
DisplayLink generation: after the paint callback releases the window lock, it
resumes only the same instance it paused. A callback that replaced or removed the
link therefore cannot accidentally resume a different native object.

Patched call sites:

- Immediate frame when a window becomes key.
- AppKit `displayLayer:` synchronous paint.

Verification belongs in the Norma OS L3 memory regression run: continuous
foreground repaint must not cause the `CVDisplayLink` object count to grow.

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
