//! 运行状态映射与逐帧平滑：新状态从当前值出发（可中断），工具切换反馈有节流。

use super::palette::{mix_rgb, Rgb};
use super::{aura, ferrofluid, underline, RunPalette, RunStyle};
use runtime::{AgentTaskState, CardAttentionKind, CardAttentionState};

/// 同一张卡片两次工具切换反馈的最小间隔。
pub(crate) const PING_THROTTLE_MS: f32 = 600.0;
/// 完成、失败进入后保持动画的时长，之后画面定格，不再请求帧。
pub(crate) const SETTLE_AFTER_MS: f32 = 1600.0;
const LEVEL_TAU_MS: f32 = 140.0;
const PALETTE_TAU_MS: f32 = 220.0;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum RunState {
    #[default]
    Ready,
    Thinking,
    Tool,
    Approval,
    Done,
    Failed,
}

impl RunState {
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Ready => 0,
            Self::Thinking => 1,
            Self::Tool => 2,
            Self::Approval => 3,
            Self::Done => 4,
            Self::Failed => 5,
        }
    }
}

/// 一张卡片当前的业务输入。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RunInput {
    pub state: RunState,
    /// 完成提示是否已查看；只对 `Done` 有意义。
    pub viewed: bool,
    /// 当前工具名的指纹，变化即一次工具切换。
    pub tool_key: u64,
}

pub(crate) fn run_input(
    state: Option<&AgentTaskState>,
    attention: Option<CardAttentionState>,
) -> RunInput {
    let viewed = !matches!(
        attention,
        Some(CardAttentionState {
            kind: CardAttentionKind::AgentTaskDone,
            viewed: false,
        })
    );
    let (state, tool_key) = match state {
        Some(AgentTaskState::Thinking) => (RunState::Thinking, 0),
        Some(AgentTaskState::RunningTool { tool_name }) => (
            RunState::Tool,
            fingerprint(tool_name.as_deref().unwrap_or_default()),
        ),
        Some(AgentTaskState::WaitingApproval { .. } | AgentTaskState::WaitingQuestion) => {
            (RunState::Approval, 0)
        }
        Some(AgentTaskState::Done) => (RunState::Done, 0),
        Some(AgentTaskState::Failed { .. }) => (RunState::Failed, 0),
        Some(AgentTaskState::Ready | AgentTaskState::Ended) | None => (RunState::Ready, 0),
    };
    RunInput {
        state,
        viewed,
        tool_key,
    }
}

fn fingerprint(text: &str) -> u64 {
    // FNV-1a：只用来判断工具名是否变化，不需要抗碰撞。
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// 各风格每帧的平滑目标：最多四个标量（含义由风格自定）和一个相位速度。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Targets {
    pub values: [f32; 4],
    pub taus: [f32; 4],
    /// 每毫秒前进的周长比例。
    pub phase_speed: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunMotion {
    pub input: RunInput,
    /// 自创建起的毫秒数，周期性起伏都以它为时钟；不出帧时冻结。
    pub time: f32,
    pub since_change: f32,
    pub since_ping: f32,
    /// 总体强度（原型的 `lvl`）。
    pub level: f32,
    pub color: Rgb,
    pub spectrum: [Rgb; 4],
    pub values: [f32; 4],
    pub phase: f32,
    settled: bool,
}

impl RunMotion {
    pub(crate) fn new(style: RunStyle, input: RunInput, palette: &RunPalette) -> Self {
        let ready = palette.state_color(RunState::Ready);
        Self {
            input,
            time: 0.0,
            since_change: 0.0,
            since_ping: if input.state == RunState::Tool {
                0.0
            } else {
                f32::MAX
            },
            level: 0.0,
            color: ready,
            spectrum: palette.spectrum(RunState::Ready),
            values: initial_values(style),
            phase: 0.0,
            settled: false,
        }
    }

    /// 返回是否有变化。进入工具调用或切换工具时触发 ping（受节流）。
    pub(crate) fn set_input(&mut self, input: RunInput) -> bool {
        if input == self.input {
            return false;
        }
        let previous = self.input;
        self.input = input;
        if input.state != previous.state {
            self.since_change = 0.0;
        }
        let tool_switched = input.state == RunState::Tool
            && (previous.state != RunState::Tool || previous.tool_key != input.tool_key);
        if tool_switched && self.since_ping >= PING_THROTTLE_MS {
            self.since_ping = 0.0;
        }
        self.settled = false;
        true
    }

    pub(crate) fn tick(
        &mut self,
        dt_ms: f32,
        style: RunStyle,
        palette: &RunPalette,
        reduce_motion: bool,
    ) {
        let dt = dt_ms.max(0.0);
        self.time += dt;
        self.since_change += dt;
        self.since_ping = (self.since_ping + dt).min(f32::MAX);
        let snap = reduce_motion;
        let ease = |tau: f32| {
            if snap {
                1.0
            } else {
                1.0 - (-dt / tau.max(1.0)).exp()
            }
        };
        let state = self.input.state;
        let visible_state = if state == RunState::Done && self.input.viewed {
            RunState::Ready
        } else {
            state
        };
        let level_target = self.level_target();
        self.level += (level_target - self.level) * ease(LEVEL_TAU_MS);
        let color_target = palette.state_color(state);
        self.color = mix_rgb(self.color, color_target, ease(LEVEL_TAU_MS));
        let spectrum_target = palette.spectrum(visible_state);
        let k = ease(PALETTE_TAU_MS);
        for (current, target) in self.spectrum.iter_mut().zip(spectrum_target) {
            *current = mix_rgb(*current, target, k);
        }
        let targets = targets(style, self);
        for i in 0..4 {
            self.values[i] += (targets.values[i] - self.values[i]) * ease(targets.taus[i]);
        }
        if !snap {
            self.phase = (self.phase + dt * targets.phase_speed).rem_euclid(1.0);
        }
        let near = |a: f32, b: f32, eps: f32| (a - b).abs() < eps;
        self.settled = near(self.level, level_target, 0.002)
            && (0..3).all(|i| near(self.color[i], color_target[i], 0.6))
            && self
                .spectrum
                .iter()
                .zip(spectrum_target)
                .all(|(a, b)| (0..3).all(|i| near(a[i], b[i], 0.6)))
            && (0..4).all(|i| near(self.values[i], targets.values[i], 0.002));
    }

    fn level_target(&self) -> f32 {
        match self.input.state {
            RunState::Ready => 0.0,
            RunState::Thinking | RunState::Approval => 1.0,
            RunState::Tool => 1.2,
            RunState::Done if self.input.viewed => 0.0,
            RunState::Done => 0.5,
            RunState::Failed if self.since_change > 900.0 => 0.6,
            RunState::Failed => 1.0,
        }
    }

    /// 是否还需要下一帧。减少动态效果时只在状态变化时重绘，不出帧。
    pub(crate) fn wants_frames(&self, reduce_motion: bool) -> bool {
        if reduce_motion {
            return false;
        }
        match self.input.state {
            RunState::Thinking | RunState::Tool | RunState::Approval => true,
            RunState::Done if !self.input.viewed => {
                self.since_change < SETTLE_AFTER_MS || !self.settled
            }
            RunState::Failed => self.since_change < SETTLE_AFTER_MS || !self.settled,
            RunState::Done | RunState::Ready => !self.settled,
        }
    }
}

fn initial_values(style: RunStyle) -> [f32; 4] {
    match style {
        RunStyle::Classic => [0.0; 4],
        RunStyle::Underline => underline::INITIAL,
        RunStyle::Aura => aura::INITIAL,
        RunStyle::Ferrofluid => ferrofluid::INITIAL,
    }
}

fn targets(style: RunStyle, motion: &RunMotion) -> Targets {
    match style {
        RunStyle::Classic => Targets {
            values: [0.0; 4],
            taus: [1.0; 4],
            phase_speed: 0.0,
        },
        RunStyle::Underline => underline::targets(motion),
        RunStyle::Aura => aura::targets(motion),
        RunStyle::Ferrofluid => ferrofluid::targets(motion),
    }
}
