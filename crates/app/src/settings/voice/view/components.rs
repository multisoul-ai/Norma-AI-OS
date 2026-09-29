use super::with_alpha;
use gpui::{div, prelude::*, px, rgba, Div, FontWeight, SharedString};
use ui::Theme;

/// Settings actions keep a GPUI `Div` because callers attach stable debug selectors after
/// construction; gpui-component 0.5.1's `Button` does not expose that test seam or this compact
/// accent variant. Keep this exception local instead of introducing another general button type.
pub(crate) fn action_button(_theme: Theme, label: impl Into<SharedString>, accent: u32) -> Div {
    div()
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(accent, 0x70)))
        .bg(rgba(with_alpha(accent, 0x18)))
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(accent))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(accent, 0x28))))
        .child(label.into())
}
