//! 当前特效卡：大预览 + 名称、说明、重播与保存状态；项目范围多一个“跟随工作空间”开关。

use super::page::EffectsPageData;
use super::preview::{preview_scene, Preview, PreviewTime};
use super::state::EffectsTarget;
use crate::app::effects_actions::{style_chips, style_line, style_name};
use crate::app::MultisoulApp;
use crate::motion::DispatchEffectStyle;
use crate::settings::typography::{control_label, SettingsText, TextRole};
use crate::settings::wallpaper::layout::WallpaperLayout;
use gpui::{div, prelude::*, px, rgba, Context, Div};
use std::sync::OnceLock;
use std::time::Instant;
use ui::component::{
    button::{Button, ButtonVariants},
    switch::Switch,
    Icon, IconName,
};
use ui::Theme;

pub(super) fn style_of(id: &str) -> Option<DispatchEffectStyle> {
    DispatchEffectStyle::from_id(id)
}

/// 大预览的播放方式：平时循环；减少动态效果时只在点“重播”后播一遍。
fn hero_time(data: &EffectsPageData) -> PreviewTime {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = *START.get_or_init(Instant::now);
    match (data.reduce_motion, data.replay_started) {
        (false, Some(started)) => PreviewTime::Playing { started },
        (false, None) => PreviewTime::Playing { started: start },
        (true, Some(started)) => PreviewTime::Once { started },
        (true, None) => PreviewTime::Still,
    }
}

pub(super) fn current_section(
    theme: Theme,
    data: &EffectsPageData,
    layout: &WallpaperLayout,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let Some(target) = data.target.clone() else {
        return div();
    };
    let selected = data.state.selected_style(&target).to_owned();
    let project = matches!(target, EffectsTarget::Project { .. });
    let follows = data.state.follows_workspace(&target);
    // 内容区够宽就左右并排，预览随容器在 200–272px 之间收缩；很窄时才上下堆叠并占满宽。
    let stacked = layout.width < 480.0;
    let width = if stacked {
        layout.width - 34.0
    } else {
        (layout.width * 0.42).clamp(200.0, 272.0)
    };
    let note = match (project, follows) {
        (true, true) => ui::i18n::text("effects.note_following"),
        (true, false) => ui::i18n::text("effects.note_project"),
        (false, _) => ui::i18n::text("effects.note_workspace"),
    };
    let style = style_of(&selected);
    let seconds = style.map(|style| {
        let timing = style.timing(data.state.speed.multiplier());
        format!(" · {:.2}s", (timing.flight + timing.impact).as_secs_f32())
    });
    let undo = data.state.undo.is_some();
    let chips = style_chips(&selected);
    let accent = |alpha: u32| rgba((theme.accent_blue & 0xffffff00) | alpha);
    let status = data
        .state
        .status
        .clone()
        .unwrap_or_else(|| ui::i18n::text("settings.auto_save").to_owned());
    div()
        .flex()
        .items_center()
        .gap(px(20.0))
        .p(px(16.0))
        .when(stacked, |card| card.flex_col().items_start())
        .rounded(px(16.0))
        .border_1()
        .border_color(rgba((theme.node_stroke & 0xffffff00) | 0x60))
        .bg(rgba(theme.panel_high))
        .child(preview_scene(Preview {
            theme,
            style,
            time: hero_time(data),
            width,
            radius: 12.0,
            speed: data.state.speed.multiplier(),
        }))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .when(stacked, |info| info.w_full())
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .settings_text(TextRole::Eyebrow, theme)
                        .child(ui::i18n::text("effects.current")),
                )
                .child(
                    div()
                        .id("effects-current-name")
                        .debug_selector(|| "effects-current-name".into())
                        .settings_font(TextRole::Headline)
                        .truncate()
                        .child(style_name(&selected)),
                )
                .child(div().settings_text(TextRole::Helper, theme).child(format!(
                    "{}{}",
                    style_line(&selected),
                    seconds.unwrap_or_default()
                )))
                .child(
                    div().flex().flex_wrap().gap(px(6.0)).children(
                        chips
                            .into_iter()
                            .filter(|chip| !chip.is_empty())
                            .map(|chip| {
                                div()
                                    .h(px(24.0))
                                    .px(px(10.0))
                                    .flex()
                                    .items_center()
                                    .rounded(px(8.0))
                                    .border_1()
                                    .border_color(rgba((theme.node_stroke & 0xffffff00) | 0x70))
                                    .settings_text(TextRole::Caption, theme)
                                    .child(chip)
                            }),
                    ),
                )
                .when(project, |info| info.child(follow_row(theme, follows, cx)))
                .child(div().settings_text(TextRole::Helper, theme).child(note))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            Button::new("effects-replay")
                                .primary()
                                .rounded(px(18.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .child(Icon::new(IconName::Redo2).size(px(14.0)))
                                        .child(control_label(ui::i18n::text("effects.replay"))),
                                )
                                .on_click(
                                    cx.listener(|app, _, _, cx| app.replay_effect_preview(cx)),
                                ),
                        )
                        .when(undo, |row| {
                            row.child(
                                Button::new("effects-undo")
                                    .rounded(px(18.0))
                                    .child(control_label(ui::i18n::text("effects.undo")))
                                    .on_click(
                                        cx.listener(|app, _, _, cx| app.undo_effect_change(cx)),
                                    ),
                            )
                        })
                        .child(
                            div()
                                .id("effects-save-feedback")
                                .debug_selector(|| "effects-save-feedback".into())
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .settings_text(TextRole::Caption, theme)
                                .child(
                                    Icon::new(IconName::CircleCheck)
                                        .size(px(14.0))
                                        .text_color(accent(0xff)),
                                )
                                .child(status),
                        ),
                ),
        )
}

fn follow_row(theme: Theme, follows: bool, cx: &mut Context<MultisoulApp>) -> gpui::Stateful<Div> {
    div()
        .id("effects-follow-workspace")
        .w_full()
        .h(px(36.0))
        .px(px(8.0))
        .rounded(px(6.0))
        .border_1()
        .border_color(rgba(0x00000000))
        .tab_index(0)
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(theme.panel)))
        .active(move |style| style.bg(rgba(theme.node_stroke)))
        .focus(move |style| style.border_color(rgba(theme.accent_blue)))
        .child(
            div()
                .settings_font(TextRole::Body)
                .child(ui::i18n::text("effects.follow_workspace")),
        )
        .child(Switch::new("effects-follow-workspace-switch").checked(follows))
        .on_click(cx.listener(move |app, _, _, cx| app.set_effects_follow_workspace(!follows, cx)))
        .on_key_down(cx.listener(move |app, event: &gpui::KeyDownEvent, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                app.set_effects_follow_workspace(!follows, cx);
                cx.stop_propagation();
            }
        }))
}
