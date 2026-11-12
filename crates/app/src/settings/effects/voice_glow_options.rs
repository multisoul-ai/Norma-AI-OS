//! 语音光晕选项：强度档位、聆听与思考时是否显示。关闭风格时整组禁用。

use super::page::EffectsPageData;
use crate::app::MultisoulApp;
use crate::effects::voice_glow::{Gain, VoiceGlowStyle};
use crate::settings::typography::{control_label, SettingsText, TextRole};
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful};
use ui::component::{button::Button, switch::Switch, Disableable, Selectable, Sizable};
use ui::Theme;

fn gain_label(gain: Gain) -> &'static str {
    ui::i18n::text(match gain {
        Gain::Soft => "effects.glow_gain_soft",
        Gain::Standard => "effects.glow_gain_standard",
        Gain::Vivid => "effects.glow_gain_vivid",
    })
}

fn row(theme: Theme, title: &'static str, hint: &'static str, control: impl IntoElement) -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(16.0))
        .px(px(16.0))
        .py(px(12.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(div().settings_font(TextRole::RowTitle).child(title))
                .child(div().settings_text(TextRole::Helper, theme).child(hint)),
        )
        .child(div().flex_none().child(control))
}

pub(super) fn options(
    theme: Theme,
    data: &EffectsPageData,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let prefs = data.state.voice_glow;
    let off = prefs.style == VoiceGlowStyle::Off;
    // 经典只在回复时出现，「聆听与思考时也显示」对它无效。
    let classic = prefs.style == VoiceGlowStyle::Classic;
    let phases_hint = if classic {
        ui::i18n::text("effects.glow_phases_classic_hint")
    } else {
        ui::i18n::text("effects.glow_phases_hint")
    };
    let gain = div()
        .id("voice-glow-gain")
        .flex()
        .gap(px(4.0))
        .children(Gain::ALL.into_iter().map(|value| {
            Button::new(("voice-glow-gain", value as usize))
                .small()
                .rounded(px(8.0))
                .selected(prefs.gain == value)
                .disabled(off)
                .child(control_label(gain_label(value)))
                .on_click(cx.listener(move |app, _, _, cx| app.set_voice_glow_gain(value, cx)))
        }));
    let phases = Switch::new("voice-glow-phases")
        .small()
        .checked(prefs.show_listening_thinking && !classic)
        .disabled(off || classic)
        .on_click(cx.listener(|app, _, _, cx| app.toggle_voice_glow_phases(cx)));
    div()
        .id("voice-glow-options")
        .debug_selector(|| "voice-glow-options".into())
        .flex()
        .flex_col()
        .rounded(px(14.0))
        .border_1()
        .border_color(rgba((theme.node_stroke & 0xffffff00) | 0x60))
        .when(off, |group| group.opacity(0.5))
        .child(row(
            theme,
            ui::i18n::text("effects.glow_gain_title"),
            ui::i18n::text("effects.glow_gain_hint"),
            gain,
        ))
        .child(
            div()
                .h(px(1.0))
                .bg(rgba((theme.node_stroke & 0xffffff00) | 0x40)),
        )
        .child(row(
            theme,
            ui::i18n::text("effects.glow_phases_title"),
            phases_hint,
            phases,
        ))
}
