//! 语音光晕分区：当前大预览（自动轮播 / 固定状态）→ 风格网格 → 选项。全局偏好，所有项目共用。

use super::page::EffectsPageData;
use super::voice_glow_scene::{glow_scene, GlowScene};
use crate::app::voice_glow_actions::{style_line, style_name};
use crate::app::MultisoulApp;
use crate::effects::voice_glow::preview::{self, PreviewPin, PINNED_LOOP_MS, STILL_MS};
use crate::effects::voice_glow::{VoiceGlowPhase, VoiceGlowStyle};
use crate::settings::typography::{control_label, SettingsText, TextRole};
use crate::settings::wallpaper::layout::WallpaperLayout;
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, KeyDownEvent, Stateful};
use std::sync::OnceLock;
use std::time::Instant;
use ui::component::{
    button::{Button, ButtonVariants},
    Disableable, Icon, IconName, Selectable, Sizable,
};
use ui::Theme;

/// 预览状态标签：(固定值, id, 文案键, 色点)。
const CHIPS: [(PreviewPin, &str, &str, u32); 4] = [
    (
        PreviewPin::Phase(VoiceGlowPhase::Listening),
        "glow-state-listening",
        "effects.glow_phase_listening",
        0x4fb3e8ff,
    ),
    (
        PreviewPin::Phase(VoiceGlowPhase::Thinking),
        "glow-state-thinking",
        "effects.glow_phase_thinking",
        0x9a86f0ff,
    ),
    (
        PreviewPin::Phase(VoiceGlowPhase::Speaking),
        "glow-state-speaking",
        "effects.glow_phase_speaking",
        0xd77ad9ff,
    ),
    (
        PreviewPin::Interrupt,
        "glow-state-interrupt",
        "effects.glow_phase_interrupt",
        0x8e8e93ff,
    ),
];

fn ms_since(start: Instant, now: Instant) -> f32 {
    now.saturating_duration_since(start).as_secs_f32() * 1000.0
}

pub(super) fn voice_glow_section(
    theme: Theme,
    data: &EffectsPageData,
    layout: &WallpaperLayout,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let current = data.state.voice_glow.style;
    div()
        .id("voice-glow-section")
        .debug_selector(|| "voice-glow-section".into())
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .settings_font(TextRole::Headline)
                        .child(ui::i18n::text("effects.glow_title")),
                )
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(px(8.0))
                        .bg(rgba((theme.node_stroke & 0xffffff00) | 0x30))
                        .settings_text(TextRole::Caption, theme)
                        .child(ui::i18n::text("effects.glow_scope")),
                ),
        )
        .child(
            div()
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text("effects.glow_hint")),
        )
        .child(hero(theme, data, layout, cx))
        .child(
            div()
                .settings_text(TextRole::Eyebrow, theme)
                .child(ui::i18n::text("effects.run_styles")),
        )
        .child(
            div()
                .id("voice-glow-grid")
                .debug_selector(|| "voice-glow-grid".into())
                .w(px(layout.columns as f32
                    * (layout.cell_width + WallpaperLayout::GAP)
                    - WallpaperLayout::GAP))
                .flex()
                .flex_wrap()
                .gap(px(WallpaperLayout::GAP))
                .children(VoiceGlowStyle::ALL.into_iter().map(|style| {
                    tile(theme, data, style, style == current, layout.cell_width, cx)
                })),
        )
        .child(super::voice_glow_options::options(theme, data, cx))
}

fn hero(
    theme: Theme,
    data: &EffectsPageData,
    layout: &WallpaperLayout,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    static PAGE_START: OnceLock<Instant> = OnceLock::new();
    let start = data
        .glow
        .since
        .unwrap_or_else(|| *PAGE_START.get_or_init(Instant::now));
    let raw = ms_since(start, Instant::now());
    let prefs = data.state.voice_glow;
    let off = prefs.style == VoiceGlowStyle::Off;
    let reduce = data.reduce_motion;
    let pin = data.glow.pin;
    let t = match (pin, reduce) {
        (None, true) => STILL_MS,
        (None, false) => raw,
        (Some(_), true) => 2000.0,
        (Some(_), false) => raw % PINNED_LOOP_MS,
    };
    let stacked = layout.width < 480.0;
    let width = if stacked {
        layout.width - 34.0
    } else {
        (layout.width * 0.46).clamp(240.0, 360.0)
    };
    let caption = match pin {
        None if !reduce => format!(
            "{} · {}",
            ui::i18n::text("effects.run_auto"),
            phase_label(preview::auto_phase(t))
        ),
        None => ui::i18n::text("effects.run_auto").to_owned(),
        Some(PreviewPin::Phase(phase)) => phase_label(phase).to_owned(),
        Some(PreviewPin::Interrupt) => ui::i18n::text("effects.glow_phase_interrupt").to_owned(),
    };
    let status = data
        .state
        .glow_status
        .clone()
        .unwrap_or_else(|| ui::i18n::text("settings.auto_save").to_owned());
    let failed = status == ui::i18n::text("effects.save_failed");
    let chip = |id: &'static str, label: &'static str, dot: Option<u32>, selected: bool| {
        Button::new(id)
            .small()
            .rounded(px(8.0))
            .selected(selected)
            .disabled(off)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .when_some(dot, |row, dot| {
                        row.child(div().size(px(8.0)).rounded_full().bg(rgba(dot)))
                    })
                    .child(control_label(label)),
            )
    };
    let chips = div()
        .id("voice-glow-states")
        .debug_selector(|| "voice-glow-states".into())
        .flex()
        .flex_wrap()
        .gap(px(6.0))
        .child(
            chip(
                "glow-state-auto",
                ui::i18n::text("effects.run_auto"),
                None,
                pin.is_none(),
            )
            .on_click(cx.listener(|app, _, _, cx| app.set_glow_preview_pin(None, cx))),
        )
        .children(CHIPS.into_iter().map(|(value, id, key, dot)| {
            chip(id, ui::i18n::text(key), Some(dot), pin == Some(value)).on_click(
                cx.listener(move |app, _, _, cx| app.set_glow_preview_pin(Some(value), cx)),
            )
        }));
    div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .p(px(16.0))
        .rounded(px(16.0))
        .border_1()
        .border_color(rgba((theme.node_stroke & 0xffffff00) | 0x60))
        .bg(rgba(theme.panel_high))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(20.0))
                .when(stacked, |row| row.flex_col().items_start())
                .child(
                    div()
                        .id("voice-glow-hero")
                        .debug_selector(|| "voice-glow-hero".into())
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(glow_scene(GlowScene {
                            theme,
                            style: prefs.style,
                            pin,
                            t_ms: t,
                            gain: prefs.gain.multiplier(),
                            width,
                            playing: !reduce && !off,
                            reduce_motion: reduce,
                        }))
                        .child(div().settings_text(TextRole::Caption, theme).child(caption)),
                )
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
                                .child(ui::i18n::text("effects.glow_current")),
                        )
                        .child(
                            div()
                                .id("voice-glow-current-name")
                                .debug_selector(|| "voice-glow-current-name".into())
                                .settings_font(TextRole::Headline)
                                .truncate()
                                .child(style_name(prefs.style)),
                        )
                        .child(
                            div()
                                .settings_text(TextRole::Helper, theme)
                                .child(style_line(prefs.style)),
                        )
                        .child(actions(theme, data, off, status, failed, cx)),
                ),
        )
        .child(chips)
}

fn phase_label(phase: VoiceGlowPhase) -> &'static str {
    ui::i18n::text(match phase {
        VoiceGlowPhase::Listening => "effects.glow_phase_listening",
        VoiceGlowPhase::Thinking => "effects.glow_phase_thinking",
        VoiceGlowPhase::Speaking | VoiceGlowPhase::Idle => "effects.glow_phase_speaking",
    })
}

fn actions(
    theme: Theme,
    data: &EffectsPageData,
    off: bool,
    status: String,
    failed: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(8.0))
        .child(
            Button::new("voice-glow-replay")
                .primary()
                .rounded(px(18.0))
                .disabled(off)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(Icon::new(IconName::Redo2).size(px(14.0)))
                        .child(control_label(ui::i18n::text("effects.replay"))),
                )
                .on_click(cx.listener(|app, _, _, cx| app.replay_glow_preview(cx))),
        )
        .when(data.state.glow_undo.is_some(), |row| {
            row.child(
                Button::new("voice-glow-undo")
                    .rounded(px(18.0))
                    .child(control_label(ui::i18n::text("effects.undo")))
                    .on_click(cx.listener(|app, _, _, cx| app.undo_voice_glow(cx))),
            )
        })
        .child(
            div()
                .id("voice-glow-save-feedback")
                .debug_selector(|| "voice-glow-save-feedback".into())
                .flex()
                .items_center()
                .gap(px(5.0))
                .settings_text(TextRole::Caption, theme)
                .child(
                    Icon::new(if failed {
                        IconName::CircleX
                    } else {
                        IconName::CircleCheck
                    })
                    .size(px(14.0))
                    .text_color(rgba(if failed {
                        theme.destructive
                    } else {
                        theme.accent_blue
                    })),
                )
                .child(status),
        )
}

fn tile(
    theme: Theme,
    data: &EffectsPageData,
    style: VoiceGlowStyle,
    selected: bool,
    width: f32,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let hover = data
        .glow
        .hover
        .filter(|(hovered, _)| *hovered == style && !data.reduce_motion);
    let gain = data.state.voice_glow.gain.multiplier();
    let (pin, t_ms) = match hover {
        Some((_, since)) => (
            Some(PreviewPin::Phase(VoiceGlowPhase::Speaking)),
            ms_since(since, Instant::now()),
        ),
        None => (None, STILL_MS),
    };
    let border = if selected {
        rgba(theme.accent_blue)
    } else {
        rgba((theme.edge_highlight & 0xffffff00) | 0x40)
    };
    let id = style.id();
    let pick = move |app: &mut MultisoulApp, cx: &mut Context<MultisoulApp>| {
        app.pick_voice_glow_style(style, cx)
    };
    div()
        .id(("voice-glow-cell", style as usize))
        .debug_selector(move || format!("voice-glow-cell-{id}"))
        .w(px(width))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .cursor_pointer()
        .tab_index(0)
        .rounded(px(8.0))
        .p(px(4.0))
        .border_1()
        .border_color(rgba(0x00000000))
        .hover(move |s| s.bg(rgba(theme.panel_high)))
        .active(move |s| s.bg(rgba(theme.node_stroke)))
        .focus(move |s| {
            s.bg(rgba(theme.panel_high))
                .border_1()
                .border_color(rgba(theme.accent_blue))
        })
        .on_hover(cx.listener(move |app, entered: &bool, _, cx| {
            app.set_glow_preview_hover(style, *entered, cx)
        }))
        .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                pick(app, cx);
                cx.stop_propagation();
            }
        }))
        .on_click(cx.listener(move |app, _: &ClickEvent, _, cx| pick(app, cx)))
        .child(
            div()
                .relative()
                .rounded(px(10.0))
                .border_1()
                .border_color(border)
                .child(glow_scene(GlowScene {
                    theme,
                    style,
                    pin,
                    t_ms,
                    gain,
                    width: width - 10.0,
                    playing: hover.is_some(),
                    reduce_motion: data.reduce_motion,
                }))
                .when(style == VoiceGlowStyle::Off, |frame| {
                    frame.child(
                        div()
                            .absolute()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(IconName::EyeOff)
                                    .size(px(22.0))
                                    .text_color(rgba(theme.text_secondary)),
                            ),
                    )
                })
                .when(selected, |frame| {
                    frame.child(
                        div()
                            .absolute()
                            .right(px(7.0))
                            .top(px(7.0))
                            .size(px(22.0))
                            .rounded(px(11.0))
                            .bg(rgba(theme.accent_blue))
                            .text_color(rgba(0xffffffff))
                            .text_size(px(13.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child("✓"),
                    )
                }),
        )
        .child(
            div()
                .px(px(4.0))
                .flex()
                .items_baseline()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .settings_font(TextRole::Body)
                        .text_color(rgba(if selected {
                            theme.accent_blue
                        } else {
                            theme.text_primary
                        }))
                        .truncate()
                        .child(style_name(style)),
                )
                .when(style == VoiceGlowStyle::Classic, |row| {
                    row.child(
                        div()
                            .settings_text(TextRole::Caption, theme)
                            .child(ui::i18n::text("effects.run_default")),
                    )
                }),
        )
}
