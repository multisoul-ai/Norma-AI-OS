# Apple Glass Theme Implementation Plan

Source spec: `docs/product-specs/apple-glass-theme.md`

## Summary

Implement the Apple glass theme as a scoped shell-layer change. Keep domain crates free of GPUI dependencies, preserve terminal readability, and make the first shipped default the light glass theme while retaining a dark glass fallback API.

## Steps

1. Add TDD coverage in `crates/ui/src/theme.rs`.
   - Assert `glass_light()` and `glass_dark()` exist.
   - Assert `Theme::default()` uses `glass_light()`.
   - Assert terminal background alpha is higher than glass node fill alpha.
   - Assert focus color no longer uses the old green accent.

2. Implement glass theme tokens.
   - Replace the old single `canvas_bg` pure-color token with `canvas_from` and `canvas_to`.
   - Store colors as `0xRRGGBBAA`.
   - Add title bar, edge highlight, terminal exit color, and shadow tokens.
   - Keep `Theme::dark()` as a compatibility alias to `glass_dark()`.

3. Apply window-level material.
   - Set `WindowBackgroundAppearance::Blurred`.
   - Use transparent titlebar options.
   - Leave runtime theme switching out of scope.

4. Apply canvas and node material.
   - Render the canvas with a subtle frosted gradient.
   - Render nodes with translucent fill, fine border, top highlight, rounded corners, and soft shadow.
   - Use Apple blue family for selected and focused states.

5. Preserve terminal readability.
   - Use RGBA theme tokens in terminal rendering.
   - Keep terminal content dark and highly opaque.
   - Keep exited process placeholder high contrast.

## Verification

- `cargo test -p ui`
- `cargo check -p app`
- `rustfmt --edition 2021 --check crates/ui/src/theme.rs crates/app/src/app.rs crates/app/src/canvas_view.rs crates/app/src/main.rs crates/app/src/terminal_view.rs`

## Notes

- Full `cargo fmt --check` currently reports unrelated formatting diffs outside this feature area, so the implementation uses targeted formatting checks for the files touched by this plan.
- Existing uncommitted app/layout changes were preserved and treated as the current working baseline.
