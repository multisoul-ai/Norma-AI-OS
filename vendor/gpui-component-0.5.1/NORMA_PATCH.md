# Norma patch on gpui-component 0.5.1

Adds `text::LinkClickHandler` (GPUI global). When set, `TextView` link clicks call it with the raw
URL instead of `App::open_url`. That change touches `src/text/text_view.rs` and `src/text/inline.rs`.
Drop this vendor copy once upstream exposes a link-click callback.

Table cells (`src/text/node.rs`, `render_table`) no longer `truncate()`. The cell is `min_w_0` +
`overflow_hidden`, and an inner `flex_1` + `min_w_0` div gives the text a definite width so it wraps.
Long paths in a Markdown table stay readable instead of being clipped at the right edge.
