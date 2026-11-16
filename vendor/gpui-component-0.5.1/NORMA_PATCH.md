# Norma patch on gpui-component 0.5.1

Adds `text::LinkClickHandler` (GPUI global). When set, `TextView` link clicks call it with the raw
URL instead of `App::open_url`. Touches `src/text/text_view.rs` and `src/text/inline.rs` only.
Drop this vendor copy once upstream exposes a link-click callback.
