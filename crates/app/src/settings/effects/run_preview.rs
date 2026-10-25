//! 运行特效的迷你预览：一张缩小的 Agent 卡片，叠上与画布相同的特效绘制。
//!
//! 预览没有自己的逐帧状态：每次渲染从“准备”出发，按时间轴把平滑过程重放到当前时刻
//! （33ms 一步，十几秒的轮播约几百步，微秒级），所以暂停、重播、悬停都只需记一个起点。

use crate::cards::border_glow::render_glow_ring;
use crate::cards::run_effect::paint_prims;
use crate::cards::snake::render_working_snake_border;
use crate::motion::breathing::{breathing_color_for_phase, BreathingState};
use crate::motion::run_style::{
    self, with_alpha_f, CardFrame, RunInput, RunMotion, RunPalette, RunState, RunStyle,
};
use crate::motion::visual::BorderGlow;
use crate::motion::working::resolve_working_snake;
use canvas::CanvasRect;
use gpui::{canvas, div, prelude::*, px, rgba, Div};
use ui::Theme;

/// 大预览的自动轮播（与原型的 `DUR` 一致）。
pub(super) const HERO_CYCLE: [(RunState, f32); 6] = [
    (RunState::Ready, 1200.0),
    (RunState::Thinking, 3000.0),
    (RunState::Tool, 3800.0),
    (RunState::Approval, 3000.0),
    (RunState::Done, 2800.0),
    (RunState::Failed, 2600.0),
];
/// 网格悬停时更快地走一遍（原型的 0.6 倍），跳过准备。
pub(super) const TILE_CYCLE: [(RunState, f32); 5] = [
    (RunState::Thinking, 1800.0),
    (RunState::Tool, 2300.0),
    (RunState::Approval, 1800.0),
    (RunState::Done, 1700.0),
    (RunState::Failed, 1600.0),
];
/// 网格静止时的定格：思考进入 1.5s 后。
pub(super) const STILL: (RunState, f32) = (RunState::Thinking, 1500.0);
const STEP_MS: f32 = 33.0;
const TOOL_SWITCH_MS: f32 = 1800.0;
/// 固定某个状态时，每 24s 从头再来，单帧回放最多约 730 步。
const PINNED_LOOP_MS: f32 = 24_000.0;

/// 时间轴上 `t` 所处的段序号与段内已过时长。
pub(super) fn position(schedule: &[(RunState, f32)], t: f32) -> (usize, f32) {
    let mut start = 0.0;
    for (index, (_, duration)) in schedule.iter().enumerate() {
        if t < start + duration {
            return (index, t - start);
        }
        start += duration;
    }
    let last = schedule.len().saturating_sub(1);
    (last, t - (start - schedule.get(last).map_or(0.0, |s| s.1)))
}

/// 按时间轴从准备回放到 `t`（循环播放时取本轮）；`viewed_at` 之后的完成视为已查看。
pub(super) fn simulate(
    style: RunStyle,
    palette: &RunPalette,
    schedule: &[(RunState, f32)],
    t: f32,
    viewed_at: Option<f32>,
    reduce_motion: bool,
) -> RunMotion {
    let total: f32 = schedule.iter().map(|(_, d)| d).sum();
    let period = if total.is_finite() {
        total
    } else {
        PINNED_LOOP_MS
    };
    let round_start = (t / period).floor() * period;
    let local = t - round_start;
    let viewed_from = viewed_at
        .filter(|v| *v >= round_start)
        .map(|v| v - round_start);
    let mut motion = RunMotion::new(style, RunInput::default(), palette);
    let mut s = 0.0;
    loop {
        let (index, into) = position(schedule, s);
        let state = schedule[index].0;
        let changed = motion.set_input(RunInput {
            state,
            viewed: state == RunState::Done && viewed_from.is_some_and(|v| s >= v),
            // 只有工具调用才换工具；其他状态指纹为 0，与画布的 `run_input` 一致。
            tool_key: if state == RunState::Tool {
                (into / TOOL_SWITCH_MS) as u64 + 1
            } else {
                0
            },
        });
        if s >= local {
            return motion;
        }
        let dt = STEP_MS.min(local - s);
        // 与画布一致：定格后不再推进（画布不出帧），直到状态再变化。
        if changed || motion.wants_frames(reduce_motion) {
            motion.tick(dt, style, palette, reduce_motion);
        }
        s += dt;
    }
}

pub(super) struct RunScene {
    pub theme: Theme,
    pub style: RunStyle,
    pub motion: RunMotion,
    pub width: f32,
    /// 播放中：每帧绘制后请求下一帧。
    pub playing: bool,
    pub reduce_motion: bool,
}

/// 16:9 场景，卡片占 72% × 66%；特效尺寸按 340px 宽的原型卡片缩放。
pub(super) fn run_scene(scene: RunScene) -> Div {
    let RunScene {
        theme,
        style,
        motion,
        width,
        playing,
        reduce_motion,
    } = scene;
    let height = width * 9.0 / 16.0;
    let card = CanvasRect::new(width * 0.14, height * 0.17, width * 0.72, height * 0.66);
    let zoom = card.size.x / 340.0;
    let radius = (14.0 * zoom).clamp(4.0, 14.0);
    let title_h = card.size.y * 0.18;
    let palette = RunPalette::from_theme(theme);
    let state = motion.input.state;
    let status = with_alpha_f(palette.state_color(state), 1.0);
    let t = motion.time;
    let classic = style == RunStyle::Classic;
    let done_unviewed = state == RunState::Done && !motion.input.viewed;
    let border = match state {
        RunState::Approval if classic => {
            breathing_color_for_phase((t / 1200.0).fract(), BreathingState::Warm, theme)
        }
        RunState::Done if classic && done_unviewed => {
            breathing_color_for_phase((t / 2500.0).fract(), BreathingState::Cool, theme)
        }
        _ => (theme.node_stroke & 0xffffff00) | 0x90,
    };
    let placeholder = |w: f32| {
        div()
            .h(px((3.0 * zoom).max(2.0)))
            .w(gpui::relative(w))
            .rounded(px(2.0))
            .bg(rgba((theme.text_tertiary & 0xffffff00) | 0x50))
    };
    let mut root = div()
        .relative()
        .flex_none()
        .w(px(width))
        .h(px(height))
        .overflow_hidden()
        .rounded(px(10.0))
        .bg(rgba(theme.canvas_to));
    if classic && done_unviewed && !reduce_motion {
        let glow = BorderGlow {
            intensity: 0.75 + 0.25 * ((t / 2500.0) * std::f32::consts::TAU).sin(),
            flash: 0.0,
        };
        root = root.child(render_glow_ring(card, radius, zoom, border, glow, 1.0));
    }
    let card_el = div()
        .absolute()
        .left(px(card.origin.x))
        .top(px(card.origin.y))
        .w(px(card.size.x))
        .h(px(card.size.y))
        .rounded(px(radius))
        .border_1()
        .border_color(rgba(border))
        .bg(rgba(theme.panel_high))
        .overflow_hidden()
        .child(
            div()
                .h(px(title_h))
                .px(px(8.0 * zoom))
                .flex()
                .items_center()
                .gap(px(5.0 * zoom))
                .border_b_1()
                .border_color(rgba((theme.edge_highlight & 0xffffff00) | 0x70))
                .child(
                    div()
                        .size(px((6.0 * zoom).max(3.0)))
                        .rounded_full()
                        .bg(rgba(status)),
                )
                .child(placeholder(0.32)),
        )
        .child(
            div()
                .p(px(8.0 * zoom))
                .flex()
                .flex_col()
                .gap(px(5.0 * zoom))
                .child(placeholder(0.7))
                .child(placeholder(0.5))
                .child(placeholder(0.6)),
        );
    if classic {
        let snake = matches!(state, RunState::Thinking | RunState::Tool)
            .then(|| resolve_working_snake(theme, 1.0, (t / 1200.0).fract()))
            .flatten()
            .filter(|_| !reduce_motion);
        root = root.child(card_el).when_some(snake, |root, snake| {
            root.child(
                div()
                    .absolute()
                    .left(px(card.origin.x))
                    .top(px(card.origin.y))
                    .w(px(card.size.x))
                    .h(px(card.size.y))
                    .child(render_working_snake_border(0, snake, zoom, radius)),
            )
        });
        return root.child(frame_driver(playing && !reduce_motion));
    }
    let margin = style.margin() * zoom;
    let frame = CardFrame {
        x: margin,
        y: margin,
        w: card.size.x,
        h: card.size.y,
        radius,
        zoom,
        title_h,
    };
    let prims = run_style::build(style, &motion, &frame, reduce_motion);
    let keep_going = playing && !reduce_motion;
    let effect = canvas(
        move |_, _, _| prims,
        move |bounds, prims, window, _| {
            paint_prims(&prims, bounds.origin, window);
            if keep_going {
                window.request_animation_frame();
            }
        },
    )
    .absolute()
    .left(px(card.origin.x - margin))
    .top(px(card.origin.y - margin))
    .w(px(card.size.x + margin * 2.0))
    .h(px(card.size.y + margin * 2.0));
    // 与画布一致：灵光、磁流体画在卡片之下，一道光画在卡片之上。
    if style.draws_under_cards() {
        root.child(effect).child(card_el)
    } else {
        root.child(card_el).child(effect)
    }
}

/// 经典预览没有自己的 canvas，用一个空 canvas 在播放时持续请求帧。
fn frame_driver(playing: bool) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            if playing {
                window.request_animation_frame();
            }
        },
    )
    .absolute()
    .size_0()
}

/// 状态芯片用的语义色。
pub(super) fn state_color(theme: Theme, state: RunState) -> u32 {
    with_alpha_f(RunPalette::from_theme(theme).state_color(state), 1.0)
}

#[cfg(test)]
#[path = "../../../tests/unit/run_preview_tests.rs"]
mod tests;
