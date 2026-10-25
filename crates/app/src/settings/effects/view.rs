//! 特效页：范围 → 当前特效 → 特效库 → 按来源 → 播放（与壁纸页同构），之后是卡片运行特效。

use super::page::EffectsPageData;
use crate::app::MultisoulApp;
use crate::settings::wallpaper::layout::WallpaperLayout;
use gpui::{div, prelude::*, px, rgba, Context, Div, Window};
use ui::Theme;

pub(crate) fn effects_settings(
    theme: Theme,
    data: &EffectsPageData,
    available_width: f32,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let layout = WallpaperLayout::for_width(available_width);
    div()
        .w(px(layout.width))
        .flex()
        .flex_col()
        .gap(px(24.0))
        .text_color(rgba(theme.text_primary))
        .child(super::scope::scope_section(
            theme,
            data,
            layout.stacked_preview,
            cx,
        ))
        .child(super::current::current_section(theme, data, &layout, cx))
        .child(super::grid::grid_section(theme, data, &layout, cx))
        .child(super::sources::sources_section(theme, data, window, cx))
        .child(super::playback::playback_section(theme, data, cx))
        .child(super::run_section::run_section(theme, data, &layout, cx))
}
