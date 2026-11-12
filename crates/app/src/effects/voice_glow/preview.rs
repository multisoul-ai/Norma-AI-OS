//! 设置页预览的时间轴与模拟能量：无逐帧状态，任意时刻 `t` 都能直接算出，暂停、重播、悬停只需记一个起点。

use super::scene::{color, lerp, GlowFrame, GlowPrim, Rect};
use super::styles::{halo, ribbon, stardust::StardustPool};
use super::{VoiceGlowPhase, VoiceGlowStyle};

/// 预览按这个虚拟整窗尺寸出图，再整体缩放进缩略图。
pub(crate) const VIRTUAL_W: f32 = 1200.0;
pub(crate) const VIRTUAL_H: f32 = 700.0;
const DOCK: Rect = Rect {
    x: 300.0,
    y: 626.0,
    w: 600.0,
    h: 52.0,
};
/// 自动轮播：聆听 → 思考 → 回复 → 静默。
const AUTO_CYCLE: [(VoiceGlowPhase, f32); 4] = [
    (VoiceGlowPhase::Listening, 2600.0),
    (VoiceGlowPhase::Thinking, 1800.0),
    (VoiceGlowPhase::Speaking, 4800.0),
    (VoiceGlowPhase::Idle, 900.0),
];
const FADE_IN_MS: f32 = 120.0;
const FADE_OUT_MS: f32 = 220.0;
const INTERRUPT_AT_MS: f32 = 1400.0;
const INTERRUPT_FADE_MS: f32 = 80.0;
const INTERRUPT_LOOP_MS: f32 = 2080.0;
/// 星尘按时间回放：只重放最近这段，足够填满粒子又不至于每帧上百步。
const STARDUST_REPLAY_MS: f32 = 2600.0;
const STEP_MS: f32 = 33.0;
/// 网格定格帧取自动轮播里「回复」段中段。
pub(crate) const STILL_MS: f32 = 2600.0 + 1800.0 + 2000.0;
/// 固定状态时每隔这么久重新计时，让回放保持有界。
pub(crate) const PINNED_LOOP_MS: f32 = 24_000.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PreviewPin {
    Phase(VoiceGlowPhase),
    Interrupt,
}

pub(crate) fn dock() -> Rect {
    DOCK
}

pub(crate) fn auto_cycle_ms() -> f32 {
    AUTO_CYCLE.iter().map(|(_, ms)| ms).sum()
}

/// 自动轮播里 `t` 所处的段序号与段内已过时长。
pub(crate) fn auto_position(t_ms: f32) -> (usize, f32) {
    let mut t = t_ms.rem_euclid(auto_cycle_ms());
    for (index, (_, ms)) in AUTO_CYCLE.iter().enumerate() {
        if t < *ms {
            return (index, t);
        }
        t -= ms;
    }
    (AUTO_CYCLE.len() - 1, 0.0)
}

pub(crate) fn auto_phase(t_ms: f32) -> VoiceGlowPhase {
    AUTO_CYCLE[auto_position(t_ms).0].0
}

fn hash(seed: f32) -> f32 {
    let v = (seed * 12.9898).sin() * 43758.547;
    v - v.floor()
}

/// 确定的「音节」包络：约 130ms 一个重音，每 2.2s 句尾停顿，音节之间平滑过渡。
pub(crate) fn sim_energy(t_ms: f32) -> f32 {
    if (t_ms.rem_euclid(2200.0)) > 1900.0 {
        return 0.02;
    }
    let slot = (t_ms / 130.0).floor();
    let frac = (t_ms / 130.0).fract();
    let level = |s: f32| 0.3 + 0.7 * hash(s);
    let smooth = frac * frac * (3.0 - 2.0 * frac);
    lerp(level(slot - 1.0), level(slot), smooth).clamp(0.0, 1.0)
}

/// `t` 时刻显示的相位与强度（淡入 120ms、淡出 220ms、打断 80ms）。
pub(crate) fn timeline(pin: Option<PreviewPin>, t_ms: f32) -> (VoiceGlowPhase, f32) {
    match pin {
        None => {
            let (index, elapsed) = auto_position(t_ms);
            let phase = AUTO_CYCLE[index].0;
            if phase == VoiceGlowPhase::Idle {
                (
                    VoiceGlowPhase::Speaking,
                    (1.0 - elapsed / FADE_OUT_MS).max(0.0),
                )
            } else {
                (phase, (elapsed / FADE_IN_MS).min(1.0))
            }
        }
        Some(PreviewPin::Phase(phase)) => (phase, (t_ms / FADE_IN_MS).clamp(0.0, 1.0)),
        Some(PreviewPin::Interrupt) => {
            let t = t_ms.rem_euclid(INTERRUPT_LOOP_MS);
            let intensity = if t < INTERRUPT_AT_MS {
                (t / FADE_IN_MS).min(1.0)
            } else {
                (1.0 - (t - INTERRUPT_AT_MS) / INTERRUPT_FADE_MS).max(0.0)
            };
            (VoiceGlowPhase::Speaking, intensity)
        }
    }
}

fn frame_at(pin: Option<PreviewPin>, t_ms: f32, reduce: bool, gain: f32) -> GlowFrame {
    let (phase, intensity) = timeline(pin, t_ms);
    let energy = match phase {
        VoiceGlowPhase::Speaking => sim_energy(t_ms),
        VoiceGlowPhase::Listening => 0.75 * sim_energy(t_ms),
        _ => 0.0,
    };
    GlowFrame {
        width: VIRTUAL_W,
        height: VIRTUAL_H,
        dock: DOCK,
        energy,
        intensity,
        phase,
        t_ms,
        reduce_motion: reduce,
        gain,
    }
}

/// 经典风格的预览近似：四边白色柔光（真实渲染用分区渐变，见 `app::voice_window_inset::classic`）。
fn classic(frame: &GlowFrame) -> Vec<GlowPrim> {
    if frame.phase != VoiceGlowPhase::Speaking || frame.intensity < 0.004 {
        return Vec::new();
    }
    let alpha = lerp(0.12, 0.55, frame.energy) * frame.intensity;
    let inset = lerp(30.0, 38.0, frame.energy) * frame.intensity;
    let rgba = color(0xFFFFFF, alpha);
    let (w, h) = (frame.width, frame.height);
    let edge = |rect: Rect| GlowPrim::Glow {
        rect,
        radius: 0.0,
        blur: inset * 1.2,
        rgba,
    };
    vec![
        edge(Rect {
            x: 0.0,
            y: -inset,
            w,
            h: inset + 1.0,
        }),
        edge(Rect {
            x: 0.0,
            y: h - 1.0,
            w,
            h: inset + 1.0,
        }),
        edge(Rect {
            x: -inset,
            y: 0.0,
            w: inset + 1.0,
            h,
        }),
        edge(Rect {
            x: w - 1.0,
            y: 0.0,
            w: inset + 1.0,
            h,
        }),
    ]
}

/// `t` 时刻整窗坐标系下的图元；`gain` 是强度档位倍率（经典不受影响）。
pub(crate) fn prims(
    style: VoiceGlowStyle,
    pin: Option<PreviewPin>,
    t_ms: f32,
    reduce: bool,
    gain: f32,
) -> Vec<GlowPrim> {
    match style {
        VoiceGlowStyle::Off => Vec::new(),
        VoiceGlowStyle::Classic => classic(&frame_at(pin, t_ms, reduce, 1.0)),
        VoiceGlowStyle::Halo => halo::build(&frame_at(pin, t_ms, reduce, gain)),
        VoiceGlowStyle::Ribbon => ribbon::build(&frame_at(pin, t_ms, reduce, gain)),
        VoiceGlowStyle::Stardust => {
            let mut pool = StardustPool::default();
            let mut t = (t_ms - STARDUST_REPLAY_MS).max(0.0);
            while t < t_ms {
                pool.step(STEP_MS, &frame_at(pin, t, reduce, gain));
                t += STEP_MS;
            }
            pool.build(&frame_at(pin, t_ms, reduce, gain))
        }
    }
}
