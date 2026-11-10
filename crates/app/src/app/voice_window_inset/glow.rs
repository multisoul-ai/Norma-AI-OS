//! 非经典风格的驱动：把「回复」（TTS 状态机）与「聆听/思考」（外部相位）合成一帧图元。

use std::sync::Arc;
use std::time::Duration;

use crate::effects::voice_glow::scene::{GlowFrame, GlowPrim, Rect};
use crate::effects::voice_glow::styles::{halo, ribbon, stardust::StardustPool};
use crate::effects::voice_glow::{VoiceGlowPhase, VoiceGlowPrefs, VoiceGlowStyle};

const EPSILON: f32 = 0.0001;
const FADE_IN_MS: f32 = 120.0;
const FADE_OUT_MS: f32 = 220.0;
/// 拿不到麦克风帧时聆听使用的轻微呼吸能量。
const LISTENING_BASE: f32 = 0.35;

pub(super) struct GlowDriver {
    ext_phase: VoiceGlowPhase,
    shown_phase: VoiceGlowPhase,
    ext_intensity: f32,
    t_ms: f32,
    reduce_motion: bool,
    pool: StardustPool,
    prims: Arc<Vec<GlowPrim>>,
}

impl Default for GlowDriver {
    fn default() -> Self {
        Self {
            ext_phase: VoiceGlowPhase::Idle,
            shown_phase: VoiceGlowPhase::Speaking,
            ext_intensity: 0.0,
            t_ms: 0.0,
            reduce_motion: false,
            pool: StardustPool::default(),
            prims: Arc::new(Vec::new()),
        }
    }
}

impl GlowDriver {
    pub(super) fn set_phase(&mut self, phase: VoiceGlowPhase) {
        if phase != self.ext_phase {
            self.ext_phase = phase;
            self.reduce_motion = crate::motion::system_reduce_motion();
        }
    }

    pub(super) fn reset(&mut self) {
        *self = Self::default();
    }

    pub(super) fn is_active(&self) -> bool {
        self.ext_phase != VoiceGlowPhase::Idle
            || self.ext_intensity > EPSILON
            || !self.pool.is_empty()
    }

    pub(super) fn prims(&self) -> Arc<Vec<GlowPrim>> {
        self.prims.clone()
    }

    pub(super) fn tick(
        &mut self,
        delta: Duration,
        speaking: Option<(f32, f32)>,
        prefs: VoiceGlowPrefs,
        viewport: (f32, f32),
        dock: Rect,
    ) {
        let dt_ms = delta.as_secs_f32() * 1000.0;
        self.t_ms += dt_ms;
        // 系统「减少动态效果」可能在回复中途改变：每 0.5s 重读一次，开销很小。
        if (self.t_ms / 500.0).floor() != ((self.t_ms - dt_ms) / 500.0).floor() {
            self.reduce_motion = crate::motion::system_reduce_motion();
        }
        let target = if self.ext_phase == VoiceGlowPhase::Idle {
            0.0
        } else {
            1.0
        };
        let rate = dt_ms
            / if target > self.ext_intensity {
                FADE_IN_MS
            } else {
                FADE_OUT_MS
            };
        self.ext_intensity += (target - self.ext_intensity).clamp(-rate, rate);

        let speaking_intensity = speaking.map_or(0.0, |(intensity, _)| intensity);
        let phase = if speaking_intensity > EPSILON {
            VoiceGlowPhase::Speaking
        } else if self.ext_phase != VoiceGlowPhase::Idle {
            self.shown_phase = self.ext_phase;
            self.ext_phase
        } else {
            self.shown_phase
        };
        let energy = match phase {
            VoiceGlowPhase::Speaking => speaking.map_or(0.0, |(_, energy)| energy),
            VoiceGlowPhase::Listening => LISTENING_BASE + 0.15 * (self.t_ms / 260.0).sin(),
            _ => 0.0,
        };
        let frame = GlowFrame {
            width: viewport.0,
            height: viewport.1,
            dock,
            energy,
            intensity: speaking_intensity.max(self.ext_intensity),
            phase,
            t_ms: self.t_ms,
            reduce_motion: self.reduce_motion,
            gain: prefs.gain.multiplier(),
        };
        let prims = match prefs.style {
            VoiceGlowStyle::Halo => halo::build(&frame),
            VoiceGlowStyle::Ribbon => ribbon::build(&frame),
            VoiceGlowStyle::Stardust => {
                self.pool.step(dt_ms, &frame);
                self.pool.build(&frame)
            }
            VoiceGlowStyle::Off | VoiceGlowStyle::Classic => Vec::new(),
        };
        self.prims = Arc::new(prims);
    }
}
