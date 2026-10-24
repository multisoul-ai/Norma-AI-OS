//! 迷你场景：背景用普通元素，特效用与画布相同的绘制代码叠在上面。
//! 静止时画款式的定格帧；播放时循环，并在每次绘制后请求下一帧。

use crate::effects::dispatch_receipt::{
    paint_preview_plan, preview_plan, preview_still_ms, preview_total_ms,
};
use crate::motion::DispatchEffectStyle;
use gpui::{canvas, div, prelude::*, px, relative, rgba, Div, Window};
use std::time::Instant;
use ui::Theme;

/// 播放结束后停顿一会儿再重来，避免循环太急。
const LOOP_PAUSE_MS: f32 = 500.0;

#[derive(Clone, Copy)]
pub(super) enum PreviewTime {
    Still,
    /// 循环播放，每轮之间停顿一下。
    Playing {
        started: Instant,
    },
    /// 只播一遍然后停在最后一帧（减少动态效果下的“重播”）。
    Once {
        started: Instant,
    },
}

pub(super) struct Preview {
    pub theme: Theme,
    /// `None` 表示“随机”：每轮循环换一款。
    pub style: Option<DispatchEffectStyle>,
    pub time: PreviewTime,
    pub width: f32,
    pub radius: f32,
    pub speed: f32,
}

#[derive(Debug, PartialEq)]
pub(super) struct Resolved {
    pub style: DispatchEffectStyle,
    pub t_ms: f32,
    /// 第几轮循环；用来让烟花、闪电每轮形状不同。
    pub round: u32,
    /// 这一帧之后是否还需要继续请求动画帧。
    pub more_frames: bool,
}

/// 当前这一刻应画哪一款、在时间轴的哪一毫秒。纯函数，便于测试。
pub(super) fn resolve_time(
    style: Option<DispatchEffectStyle>,
    time: PreviewTime,
    speed: f32,
    now: Instant,
) -> Option<Resolved> {
    let elapsed = |started: Instant| {
        now.saturating_duration_since(started).as_secs_f32() * 1000.0 / speed.max(0.1)
    };
    let pick = |round: u32| {
        style.unwrap_or_else(|| DispatchEffectStyle::random_for(&format!("preview-{round}")))
    };
    match (style, time) {
        (Some(style), PreviewTime::Still) => Some(Resolved {
            style,
            t_ms: preview_still_ms(style),
            round: 0,
            more_frames: false,
        }),
        (None, PreviewTime::Still) => None,
        (_, PreviewTime::Once { started }) => {
            let style = pick(0);
            let total = preview_total_ms(style);
            let t = elapsed(started);
            Some(Resolved {
                style,
                t_ms: t.min(total),
                round: 0,
                more_frames: t < total,
            })
        }
        (_, PreviewTime::Playing { started }) => {
            let t = elapsed(started);
            // 随机款每轮长度不同，逐轮累加才能找到当前轮。
            let (mut round, mut offset) = (0u32, 0.0f32);
            loop {
                let cycle = preview_total_ms(pick(round)) + LOOP_PAUSE_MS;
                if style.is_some() {
                    round = (t / cycle) as u32;
                    offset = round as f32 * cycle;
                    break;
                }
                if t < offset + cycle {
                    break;
                }
                offset += cycle;
                round += 1;
            }
            Some(Resolved {
                style: pick(round),
                t_ms: t - offset,
                round,
                more_frames: true,
            })
        }
    }
}

pub(super) fn preview_scene(preview: Preview) -> Div {
    let Preview {
        theme,
        style,
        time,
        width,
        radius,
        speed,
    } = preview;
    let height = width * 9.0 / 16.0;
    div()
        .relative()
        .flex_none()
        .w(px(width))
        .h(px(height))
        .overflow_hidden()
        .rounded(px(radius))
        .bg(rgba(theme.canvas_to))
        .child(backdrop(theme, width))
        .child(
            canvas(
                move |bounds, _, _| {
                    resolve_time(style, time, speed, Instant::now()).map(|resolved| {
                        let seed =
                            0x5eed_u64.wrapping_add(u64::from(resolved.round).wrapping_mul(7919));
                        let more = resolved.more_frames;
                        (
                            preview_plan(resolved.style, resolved.t_ms, bounds, theme, seed),
                            more,
                        )
                    })
                },
                move |_, prepared, window: &mut Window, _| {
                    if let Some((plan, more)) = prepared {
                        paint_preview_plan(plan, window);
                        if more {
                            window.request_animation_frame();
                        }
                    }
                },
            )
            .absolute()
            .size_full(),
        )
}

/// 一张目标卡片、一张陪衬卡片和底部的指令栏，位置与 `preview_plan` 的几何一致；
/// 卡片有彩色标题点和深色终端体，指令栏左侧是蓝色录音圆，与原型一致。
fn backdrop(theme: Theme, width: f32) -> Div {
    let card = |left: f32, top: f32, width: f32, height: f32, dot: u32, faded: bool| {
        div()
            .absolute()
            .left(relative(left))
            .top(relative(top))
            .w(relative(width))
            .h(relative(height))
            .rounded(px(7.0))
            .border_1()
            .border_color(rgba((theme.node_stroke & 0xffffff00) | 0x60))
            .bg(rgba(
                (theme.panel_high & 0xffffff00) | if faded { 0xa0 } else { 0xf0 },
            ))
            .child(
                div()
                    .absolute()
                    .left(relative(0.08))
                    .top(relative(0.12))
                    .w(relative(0.09))
                    .h(relative(0.14))
                    .rounded(px(1.5))
                    .bg(rgba(dot)),
            )
            .child(
                div()
                    .absolute()
                    .left(relative(0.08))
                    .right(relative(0.08))
                    .top(relative(0.34))
                    .bottom(relative(0.1))
                    .rounded(px(4.0))
                    .bg(rgba(0x0e151de6)),
            )
    };
    div()
        .absolute()
        .size_full()
        .child(card(0.0667, 0.1704, 0.3125, 0.311, 0x3fae8cff, true))
        .child(card(0.5583, 0.1259, 0.375, 0.3704, 0xd97757ff, false))
        .child(
            div()
                .absolute()
                .left(relative(0.2917))
                .top(relative(0.8185))
                .w(relative(0.4167))
                .h(relative(0.1259))
                .rounded(px(99.0))
                .bg(rgba(theme.dock))
                .child(
                    div()
                        .absolute()
                        .left(px(width * 0.4167 * 0.025))
                        .top(relative(0.15))
                        .size(px(width * 0.0496))
                        .rounded(px(99.0))
                        .bg(rgba(theme.accent_blue)),
                ),
        )
}
