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
