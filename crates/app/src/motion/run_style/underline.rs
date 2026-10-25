//! 一道光：只用标题栏下边线。思考时光段来回游走；工具调用时光段滑到工具名下方，
//! 每次切换轻闪；等你时整条线随心跳呼吸；完成时自左向右铺满；失败整条红线。

use super::geometry::{ease_out_cubic, heartbeat, CardFrame};
use super::palette::with_alpha_f;
use super::state::Targets;
use super::{RunMotion, RunPrim, RunState};

const TX: usize = 0;
const PRESENCE: usize = 1;
/// [光段位置（0..1 沿线）, 可见度, -, -]
pub(super) const INITIAL: [f32; 4] = [0.5, 0.0, 0.0, 0.0];
const THINK_PERIOD_MS: f32 = 2400.0;
/// 工具调用时光段停在靠右的工具名下方。
const TOOL_POSITION: f32 = 0.88;
const FILL_MS: f32 = 600.0;
const PING_MS: f32 = 400.0;

fn thinking_position(time: f32) -> f32 {
    0.5 - 0.5 * (time / THINK_PERIOD_MS * std::f32::consts::TAU).cos()
}

pub(super) fn targets(motion: &RunMotion) -> Targets {
    let state = motion.input.state;
    let visible =
        !matches!(state, RunState::Ready) && !(state == RunState::Done && motion.input.viewed);
    let (tx, tx_tau) = if state == RunState::Tool {
        (TOOL_POSITION, 260.0)
    } else {
        // 非工具状态时紧跟思考光段，切到工具时就从当前位置滑过去。
        (thinking_position(motion.time), 1.0)
    };
    Targets {
        values: [tx, if visible { 1.0 } else { 0.0 }, 0.0, 0.0],
        taus: [tx_tau, 160.0, 1.0, 1.0],
        phase_speed: 0.0,
    }
}

pub(super) fn build(motion: &RunMotion, frame: &CardFrame, reduce_motion: bool) -> Vec<RunPrim> {
    let presence = motion.values[PRESENCE].clamp(0.0, 1.0);
    if presence < 0.01 {
        return Vec::new();
    }
    let x0 = frame.x + 1.0;
    let x1 = frame.x + frame.w - 1.0;
    let length = (x1 - x0).max(0.0);
    let y = frame.y + frame.title_h;
    let thickness = (1.6 * frame.zoom).clamp(1.0, 2.5);
    let color = motion.color;
    let line = |to: f32, alpha: f32| RunPrim::Bar {
        x0,
        x1: to,
        y: y - thickness * 0.5,
        height: thickness,
        left: with_alpha_f(color, alpha * presence),
        right: with_alpha_f(color, alpha * presence),
    };
    let glint = |center: f32, width: f32, alpha: f32| {
        let half = width * 0.5;
        let left = (center - half).max(x0);
        let right = (center + half).min(x1);
        let peak = with_alpha_f(color, alpha * presence);
        let clear = with_alpha_f(color, 0.0);
        vec![
            RunPrim::Bar {
                x0: left,
                x1: center,
                y: y - thickness * 0.5,
                height: thickness,
                left: clear,
                right: peak,
            },
            RunPrim::Bar {
                x0: center,
                x1: right,
                y: y - thickness * 0.5,
                height: thickness,
                left: peak,
                right: clear,
            },
        ]
    };
    let ping = (1.0 - motion.since_ping / PING_MS).max(0.0);
    match motion.input.state {
        RunState::Ready => Vec::new(),
        RunState::Thinking if reduce_motion => vec![line(x1, 0.5)],
        RunState::Thinking => glint(
            x0 + length * thinking_position(motion.time),
            70.0 * frame.zoom,
            0.95,
        ),
        RunState::Tool if reduce_motion => {
            glint(x0 + length * TOOL_POSITION, 60.0 * frame.zoom, 0.9)
        }
        RunState::Tool => glint(
            x0 + length * motion.values[TX],
            (60.0 + 30.0 * ping) * frame.zoom,
            0.7 + 0.3 * ping,
        ),
        RunState::Approval if reduce_motion => vec![line(x1, 0.8)],
        RunState::Approval => vec![line(x1, 0.3 + 0.6 * heartbeat(motion.time))],
        RunState::Done => {
            let progress = if reduce_motion {
                1.0
            } else {
                ease_out_cubic(motion.since_change / FILL_MS)
            };
            vec![line(x0 + length * progress, 0.85)]
        }
        RunState::Failed => vec![line(x1, 0.85)],
    }
}
