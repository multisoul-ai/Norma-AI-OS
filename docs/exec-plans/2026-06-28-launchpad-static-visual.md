# Launchpad Static Visual Implementation Plan

> **Superseded for background behavior (2026-07-15):** Launchpad now uses the dedicated `wallpapers/home.mp4` video rather than the Canvas/app wallpaper. See [`2026-07-15-launchpad-dedicated-video.md`](../design-docs/2026-07-15-launchpad-dedicated-video.md). The module split and static-content steps remain historical context only.

## Source Of Truth

- Product spec: `docs/product-specs/2026-06-28-SPEC-launchpad-static-visual.md`
- Prototype: `docs/design-docs/launchpad-ui-options/index.html?variant=1`
- Prototype id: `launchpad_option_1_orbit_entry`

## Implementation Steps

1. Add focused tests for the pure Launchpad layout model and Recent Projects source behavior.
2. Move `crates/app/src/workspace/launchpad.rs` into `crates/app/src/workspace/launchpad/mod.rs` and split responsibilities into `layout`, `backdrop`, `actions`, `recent`, and `preview`.
3. Add a narrow app-level wallpaper image getter so Launchpad can use the same wallpaper source as Canvas.
4. Rebuild the Launchpad render tree around the selected static visual prototype:
   - app-level wallpaper backdrop
   - graphite shade and cyan/orange rings
   - static top toolbar preview
   - left brand/actions/recent content
   - right static preview cards
5. Verify behavior and constraints.

## Verification

Run after implementation:

```bash
cargo test -p app launchpad
cargo check -p app
bash scripts/check-max-file-lines.sh
bash scripts/check-no-allow.sh
```

## Rollback Points

- If GPUI rendering APIs block the visual design, keep the module split and tests, then reduce the visual to the existing cards over the new wallpaper backdrop.
- If line-count pressure remains high after splitting, move only decorative static preview rendering into `preview.rs` and keep behavior wiring in `mod.rs`.
