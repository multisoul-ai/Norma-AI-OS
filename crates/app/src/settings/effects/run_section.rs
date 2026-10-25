//! 卡片运行特效分区：当前大预览（自动轮播 / 固定状态）→ 风格网格。全局偏好，所有项目共用。

use super::page::EffectsPageData;
use super::run_preview::{
    position, run_scene, simulate, state_color, RunScene, HERO_CYCLE, STILL, TILE_CYCLE,
};
use crate::app::run_effects_actions::{run_style_line, run_style_name};
use crate::app::MultisoulApp;
use crate::motion::run_style::{RunPalette, RunState, RunStyle};
use crate::settings::typography::{control_label, SettingsText, TextRole};
use crate::settings::wallpaper::layout::WallpaperLayout;
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, KeyDownEvent, Stateful};
use std::sync::OnceLock;
use std::time::Instant;
use ui::component::{
    button::{Button, ButtonVariants},
    Icon, IconName, Selectable, Sizable,
};
use ui::Theme;

const STATES: [RunState; 6] = [
    RunState::Ready,
    RunState::Thinking,
    RunState::Tool,
    RunState::Approval,
    RunState::Done,
    RunState::Failed,
];

pub(crate) fn state_label(state: RunState) -> &'static str {
    ui::i18n::text(match state {
        RunState::Ready => "effects.run_state_ready",
        RunState::Thinking => "effects.run_state_thinking",
        RunState::Tool => "effects.run_state_tool",
        RunState::Approval => "effects.run_state_approval",
        RunState::Done => "effects.run_state_done",
        RunState::Failed => "effects.run_state_failed",
    })
}

fn ms_since(start: Instant, now: Instant) -> f32 {
    now.saturating_duration_since(start).as_secs_f32() * 1000.0
}

pub(super) fn run_section(
    theme: Theme,
    data: &EffectsPageData,
    layout: &WallpaperLayout,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let current = data.state.run_style;
    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .settings_font(TextRole::Headline)
                .child(ui::i18n::text("effects.run_title")),
        )
        .child(
            div()
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text("effects.run_hint")),
        )
        .child(hero(theme, data, layout, cx))
        .child(
            div()
                .settings_text(TextRole::Eyebrow, theme)
                .child(ui::i18n::text("effects.run_styles")),
        )
        .child(
            div()
                .id("run-effects-grid")
                .debug_selector(|| "run-effects-grid".into())
                .w(px(layout.columns as f32
                    * (layout.cell_width + WallpaperLayout::GAP)
                    - WallpaperLayout::GAP))
                .flex()
                .flex_wrap()
                .gap(px(WallpaperLayout::GAP))
                .children(RunStyle::ALL.into_iter().map(|style| {
                    tile(theme, data, style, style == current, layout.cell_width, cx)
                })),
        )
}

fn hero(
    theme: Theme,
    data: &EffectsPageData,
    layout: &WallpaperLayout,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    static PAGE_START: OnceLock<Instant> = OnceLock::new();
    let now = Instant::now();
    let start = data
        .run
        .since
        .unwrap_or_else(|| *PAGE_START.get_or_init(Instant::now));
    let t = ms_since(start, now);
    let viewed_at = data
        .run
        .viewed_at
        .filter(|viewed| *viewed >= start)
        .map(|viewed| ms_since(start, viewed));
    let style = data.state.run_style;
    let palette = RunPalette::from_theme(theme);
    let reduce = data.reduce_motion;
    let pinned_schedule;
    let (schedule, t): (&[(RunState, f32)], f32) = match (data.run.pinned, reduce) {
        (Some(state), _) => {
            pinned_schedule = [(state, f32::INFINITY)];
            (&pinned_schedule, if reduce { STILL.1 } else { t })
        }
        (None, true) => {
            pinned_schedule = [(STILL.0, f32::INFINITY)];
            (&pinned_schedule, STILL.1)
        }
        (None, false) => (&HERO_CYCLE, t),
    };
    let motion = simulate(style, &palette, schedule, t, viewed_at, reduce);
    let playing_state = motion.input.state;
    // 自动轮播要推进时间轴，始终出帧；固定状态定格后停帧。
    let playing = !reduce && (data.run.pinned.is_none() || motion.wants_frames(false));
    let stacked = layout.width < 480.0;
    let width = if stacked {
        layout.width - 34.0
    } else {
        (layout.width * 0.42).clamp(220.0, 300.0)
    };
    let auto = data.run.pinned.is_none();
    let caption = if auto && !reduce {
        let total: f32 = HERO_CYCLE.iter().map(|(_, d)| d).sum();
        let (index, _) = position(&HERO_CYCLE, t % total);
        format!(
            "{} · {}",
            ui::i18n::text("effects.run_auto"),
            state_label(HERO_CYCLE[index].0)
        )
    } else {
        state_label(playing_state).to_owned()
    };
    let status = data
        .state
        .run_status
        .clone()
        .unwrap_or_else(|| ui::i18n::text("settings.auto_save").to_owned());
    let failed = status == ui::i18n::text("effects.save_failed");
    let chip = |id: &'static str, label: &'static str, dot: Option<u32>, selected: bool| {
        Button::new(id)
            .small()
            .rounded(px(8.0))
            .selected(selected)
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
        .id("run-effects-states")
        .debug_selector(|| "run-effects-states".into())
        .flex()
        .flex_wrap()
        .gap(px(6.0))
        .child(
            chip(
                "run-state-auto",
                ui::i18n::text("effects.run_auto"),
                None,
                auto,
            )
            .on_click(cx.listener(|app, _, _, cx| app.set_run_preview_state(None, cx))),
        )
        .children(STATES.into_iter().map(|state| {
            chip(
                state_id(state),
                state_label(state),
                Some(state_color(theme, state)),
                data.run.pinned == Some(state),
            )
            .on_click(cx.listener(move |app, _, _, cx| app.set_run_preview_state(Some(state), cx)))
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
                        .id("run-effects-hero")
                        .debug_selector(|| "run-effects-hero".into())
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .cursor_pointer()
                        .child(run_scene(RunScene {
                            theme,
                            style,
                            motion,
                            width,
                            playing,
                            reduce_motion: reduce,
                        }))
                        .child(div().settings_text(TextRole::Caption, theme).child(caption))
                        .on_click(cx.listener(|app, _, _, cx| app.mark_run_preview_viewed(cx))),
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
                                .child(ui::i18n::text("effects.run_current")),
                        )
                        .child(
                            div()
                                .id("run-effects-current-name")
                                .debug_selector(|| "run-effects-current-name".into())
                                .settings_font(TextRole::Headline)
                                .truncate()
                                .child(run_style_name(style)),
                        )
                        .child(
                            div()
                                .settings_text(TextRole::Helper, theme)
                                .child(run_style_line(style)),
                        )
                        .child(
                            div()
                                .settings_text(TextRole::Caption, theme)
                                .child(ui::i18n::text("effects.run_preview_hint")),
                        )
                        .child(actions(theme, data, status, failed, cx)),
                ),
        )
        .child(chips)
}

fn actions(
    theme: Theme,
    data: &EffectsPageData,
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
            Button::new("run-effects-replay")
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
                .on_click(cx.listener(|app, _, _, cx| app.replay_run_preview(cx))),
        )
        .when(data.state.run_undo.is_some(), |row| {
            row.child(
                Button::new("run-effects-undo")
                    .rounded(px(18.0))
                    .child(control_label(ui::i18n::text("effects.undo")))
                    .on_click(cx.listener(|app, _, _, cx| app.undo_run_style(cx))),
            )
        })
        .child(
            div()
                .id("run-effects-save-feedback")
                .debug_selector(|| "run-effects-save-feedback".into())
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

fn state_id(state: RunState) -> &'static str {
    match state {
        RunState::Ready => "run-state-ready",
        RunState::Thinking => "run-state-thinking",
        RunState::Tool => "run-state-tool",
        RunState::Approval => "run-state-approval",
        RunState::Done => "run-state-done",
        RunState::Failed => "run-state-failed",
    }
}

fn tile(
    theme: Theme,
    data: &EffectsPageData,
    style: RunStyle,
    selected: bool,
    width: f32,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let palette = RunPalette::from_theme(theme);
    let hover = data
        .run
        .hover
        .filter(|(hovered, _)| *hovered == style && !data.reduce_motion);
    let motion = match hover {
        Some((_, since)) => simulate(
            style,
            &palette,
            &TILE_CYCLE,
            ms_since(since, Instant::now()),
            None,
            false,
        ),
        None => simulate(
            style,
            &palette,
            &[(STILL.0, f32::INFINITY)],
            STILL.1,
            None,
            data.reduce_motion,
        ),
    };
    let border = if selected {
        rgba(theme.accent_blue)
    } else {
        rgba((theme.edge_highlight & 0xffffff00) | 0x40)
    };
    let id = style.id();
    let pick =
        move |app: &mut MultisoulApp, cx: &mut Context<MultisoulApp>| app.pick_run_style(style, cx);
    div()
        .id(("run-effects-cell", style as usize))
        .debug_selector(move || format!("run-effects-cell-{id}"))
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
            app.set_run_preview_hover(style, *entered, cx)
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
                .child(run_scene(RunScene {
                    theme,
                    style,
                    motion,
                    width: width - 10.0,
                    playing: hover.is_some(),
                    reduce_motion: data.reduce_motion,
                }))
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
                        .child(run_style_name(style)),
                )
                .when(style == RunStyle::Classic, |row| {
                    row.child(
                        div()
                            .settings_text(TextRole::Caption, theme)
                            .child(ui::i18n::text("effects.run_default")),
                    )
                }),
        )
}
