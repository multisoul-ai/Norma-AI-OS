//! 播放：速度三档，派发始终播放选定的完整特效。

use super::page::EffectsPageData;
use super::state::PlaybackSpeed;
use crate::app::MultisoulApp;
use crate::settings::typography::{control_label, SettingsText, TextRole};
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::component::{
    button::{Button, ButtonGroup},
    Selectable,
};
use ui::Theme;

pub(super) fn playback_section(
    theme: Theme,
    data: &EffectsPageData,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let state = &data.state;
    let speed = |id: &'static str, label: &'static str, value: PlaybackSpeed| {
        Button::new(id)
            .w(px(72.0))
            .rounded(px(16.0))
            .child(control_label(label))
            .selected(state.speed == value)
            .on_click(cx.listener(move |app, _, _, cx| app.set_effect_speed(value, cx)))
    };
    let speed_group = ButtonGroup::new("effects-speed").outline().children([
        speed(
            "effects-speed-relaxed",
            ui::i18n::text("effects.speed_relaxed"),
            PlaybackSpeed::Relaxed,
        ),
        speed(
            "effects-speed-standard",
            ui::i18n::text("effects.speed_standard"),
            PlaybackSpeed::Standard,
        ),
        speed(
            "effects-speed-brisk",
            ui::i18n::text("effects.speed_brisk"),
            PlaybackSpeed::Brisk,
        ),
    ]);
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .settings_font(TextRole::Headline)
                .child(ui::i18n::text("effects.playback")),
        )
        .child(
            div()
                .rounded(px(16.0))
                .border_1()
                .border_color(rgba((theme.node_stroke & 0xffffff00) | 0x60))
                .bg(rgba(theme.panel_high))
                .child(row(
                    theme,
                    ui::i18n::text("effects.speed"),
                    ui::i18n::text("effects.speed_hint"),
                    speed_group,
                )),
        )
}

fn row(theme: Theme, title: &'static str, hint: &'static str, control: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(16.0))
        .min_h(px(64.0))
        .px(px(16.0))
        .py(px(10.0))
        .child(
            div()
                .flex_1()
                .min_w(px(180.0))
                .flex()
                .flex_col()
                .child(div().settings_font(TextRole::RowTitle).child(title))
                .child(div().settings_text(TextRole::Helper, theme).child(hint)),
        )
        .child(control)
}
