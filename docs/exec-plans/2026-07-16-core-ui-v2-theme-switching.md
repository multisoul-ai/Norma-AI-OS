# Core UI V2 theme switching implementation plan

## Scope

Implement the six reviewed color themes from `docs/design-docs/core-ui-v2/README.md` in the
native GPUI app. Keep canvas card title structure, typography, spacing, and interaction unchanged;
only shared color/material tokens and the Settings theme picker may change.

## Acceptance evidence

1. `ui::ThemeId::ALL` exposes exactly Paper Blue, Linen Sage, Silver Violet, Sapphire Night,
   Midnight Violet, and Forest Mint; `Theme::for_id` maps the documented base/text/accent/signal
   literals and keeps Terminal as a dark content well.
2. Settings exposes a real Themes route and six stable click targets. Selecting each target updates
   the app's active theme immediately without closing Settings.
3. Headless GPUI e2e clicks prove the visible Settings route and theme selection state.
4. `cargo test -p ui`, `cargo check -p app`, the focused theme e2e, and repository guard scripts pass.
5. A real built app is operated with Computer Use; screenshots for all six themes are saved under
   `artifacts/core-ui-v2-themes/` and summarized in a verification report.

## TDD slices

1. Theme catalog seam: add a failing public-API test for stable ids and reviewed literals, then add
   `ThemeId` and the six token sets.
2. Settings interaction seam: add a failing rendered-tree e2e for Themes navigation and selection,
   then wire the picker to `MultisoulApp::set_theme`.
3. Visual material seam: make canvas overlay and command Dock consume theme tokens, preserving the
   existing card-title element tree.
4. Verify and capture the real app; document any visual deviations instead of claiming them away.
