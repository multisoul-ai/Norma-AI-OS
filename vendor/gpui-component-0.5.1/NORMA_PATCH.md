# Norma patch on gpui-component 0.5.1

Adds `text::LinkClickHandler` (GPUI global). When set, `TextView` link clicks call it with the raw
URL instead of `App::open_url`. That change touches `src/text/text_view.rs` and `src/text/inline.rs`.
Drop this vendor copy once upstream exposes a link-click callback.

Table cells (`src/text/node.rs`, `render_table`) no longer `truncate()`. The cell is `min_w_0` +
`overflow_hidden`, and an inner `flex_1` + `min_w_0` div gives the text a definite width so it wraps.
Long paths in a Markdown table stay readable instead of being clipped at the right edge.

Non-scrolling `TextView` (`src/text/text_view.rs`) sizes to its content (`w_full` + `h_auto`) instead of
`size_full()`. A percentage height inside an auto-height parent does not contribute to that parent, so
the next sibling overlapped the painted markdown. `scrollable(true)` still fills its parent.

Selectable TextView can render literal, wrapping text without Markdown parsing. Selection uses the
two original drag endpoints and GPUI's nearest byte index for each text block. The selectable
surface takes focus on mouse-down and removes the synthetic final block separator when copying.

Markdown updates coalesce within a fixed interval and preserve selection coordinates during streaming.
