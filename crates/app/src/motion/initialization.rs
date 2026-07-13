//! 新建 Terminal / Agent 卡片的短暂初始化动画状态机。
//!
//! 状态完全属于 app 动效层：它只决定覆盖层帧号与透明度，不参与 runtime、PTY
//! 或持久化。正常路径为 32 帧 × 31ms，最后 186ms 交叉淡入真实内容；用户输入可
//! 将任意活动帧切换为 80ms 的快速退出。

use std::time::Duration;

use canvas::AgentProfile;

pub(crate) const INITIALIZATION_FRAME_COUNT: usize = 32;
pub(crate) const INITIALIZATION_FRAME_INTERVAL: Duration = Duration::from_millis(31);
pub(crate) const INITIALIZATION_CROSSFADE_START: Duration = Duration::from_millis(806);
pub(crate) const INITIALIZATION_CROSSFADE_DURATION: Duration = Duration::from_millis(186);
pub(crate) const INITIALIZATION_TOTAL_DURATION: Duration = Duration::from_millis(992);
pub(crate) const INITIALIZATION_INTERRUPT_DURATION: Duration = Duration::from_millis(80);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InitializationAnimationKind {
    Terminal,
    Codex,
    Claude,
    OpenCode,
}

impl InitializationAnimationKind {
    pub(crate) fn for_agent_profile(profile: AgentProfile) -> Self {
        match profile {
            AgentProfile::Codex => Self::Codex,
            AgentProfile::Claude => Self::Claude,
            AgentProfile::OpenCode => Self::OpenCode,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InitializationPresentation {
    pub(crate) kind: InitializationAnimationKind,
    pub(crate) frame_index: usize,
    pub(crate) overlay_opacity: f32,
    pub(crate) content_opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum InitializationPhase {
    Playing {
        elapsed: Duration,
    },
    Dismissing {
        elapsed: Duration,
        overlay_start: f32,
        content_start: f32,
        frame_index: usize,
    },
    Finished,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct InitializationAnimation {
    kind: InitializationAnimationKind,
    phase: InitializationPhase,
}

impl InitializationAnimation {
    pub(crate) fn new(kind: InitializationAnimationKind) -> Self {
        Self {
            kind,
            phase: InitializationPhase::Playing {
                elapsed: Duration::ZERO,
            },
        }
    }

    pub(crate) fn presentation(self) -> Option<InitializationPresentation> {
        match self.phase {
            InitializationPhase::Playing { elapsed } => playing_presentation(self.kind, elapsed),
            InitializationPhase::Dismissing {
                elapsed,
                overlay_start,
                content_start,
                frame_index,
            } => {
                let progress = duration_progress(elapsed, INITIALIZATION_INTERRUPT_DURATION);
                Some(InitializationPresentation {
                    kind: self.kind,
                    frame_index,
                    overlay_opacity: overlay_start * (1.0 - progress),
                    content_opacity: content_start + (1.0 - content_start) * progress,
                })
            }
            InitializationPhase::Finished => None,
        }
    }

    pub(crate) fn tick(&mut self, delta: Duration) -> bool {
        let before = self.presentation();
        match &mut self.phase {
            InitializationPhase::Playing { elapsed } => {
                *elapsed = elapsed.saturating_add(delta);
                if *elapsed >= INITIALIZATION_TOTAL_DURATION {
                    self.phase = InitializationPhase::Finished;
                }
            }
            InitializationPhase::Dismissing { elapsed, .. } => {
                *elapsed = elapsed.saturating_add(delta);
                if *elapsed >= INITIALIZATION_INTERRUPT_DURATION {
                    self.phase = InitializationPhase::Finished;
                }
            }
            InitializationPhase::Finished => return false,
        }
        before != self.presentation()
    }

    pub(crate) fn interrupt(&mut self) -> bool {
        let Some(presentation) = self.presentation() else {
            return false;
        };
        if !matches!(self.phase, InitializationPhase::Playing { .. }) {
            return false;
        }
        self.phase = InitializationPhase::Dismissing {
            elapsed: Duration::ZERO,
            overlay_start: presentation.overlay_opacity,
            content_start: presentation.content_opacity,
            frame_index: presentation.frame_index,
        };
        true
    }

    /// 返回到下一次可见时间线边界的逻辑时长。
    ///
    /// 常规动画仍以 12ms 刷新，但帧切换、交叉淡入起点和结束点不能被该间隔
    /// 量化。因此帧循环会在常规 tick 前选择这个更短的值。
    pub(crate) fn next_visual_boundary_after(self) -> Option<Duration> {
        match self.phase {
            InitializationPhase::Playing { elapsed } => {
                let completed_frames =
                    (elapsed.as_millis() / INITIALIZATION_FRAME_INTERVAL.as_millis()) as usize;
                let next_frame = INITIALIZATION_FRAME_INTERVAL
                    * (completed_frames + 1).min(INITIALIZATION_FRAME_COUNT) as u32;
                Some(next_frame.saturating_sub(elapsed))
            }
            InitializationPhase::Dismissing { elapsed, .. } => {
                Some(INITIALIZATION_INTERRUPT_DURATION.saturating_sub(elapsed))
            }
            InitializationPhase::Finished => None,
        }
    }
}

fn playing_presentation(
    kind: InitializationAnimationKind,
    elapsed: Duration,
) -> Option<InitializationPresentation> {
    if elapsed >= INITIALIZATION_TOTAL_DURATION {
        return None;
    }
    let frame_index = ((elapsed.as_millis() / INITIALIZATION_FRAME_INTERVAL.as_millis()) as usize)
        .min(INITIALIZATION_FRAME_COUNT - 1);
    let crossfade = elapsed
        .checked_sub(INITIALIZATION_CROSSFADE_START)
        .map(|elapsed| duration_progress(elapsed, INITIALIZATION_CROSSFADE_DURATION))
        .unwrap_or(0.0);
    Some(InitializationPresentation {
        kind,
        frame_index,
        overlay_opacity: 1.0 - crossfade,
        content_opacity: crossfade,
    })
}

fn duration_progress(elapsed: Duration, duration: Duration) -> f32 {
    if duration.is_zero() {
        return 1.0;
    }
    (elapsed.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0)
}
