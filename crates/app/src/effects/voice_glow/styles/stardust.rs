//! 星尘：发光颗粒从 Dock 升起、摇曳、消散（呼应壁纸里的发丝光与星点）。
//! 聆听时颗粒被吸向 Dock；思考时少量颗粒绕 Dock 公转。固定容量粒子池，帧内不分配。

use crate::effects::voice_glow::scene::{color, GlowFrame, GlowPrim, Rect};
use crate::effects::voice_glow::VoiceGlowPhase;

const CAPACITY: usize = 220;
const PALETTE: [u32; 4] = [0xF4EEFF, 0xC5B5FF, 0x9FD8FF, 0xF4EEFF];
const LAVENDER: u32 = 0xC5B5FF;

#[derive(Clone, Copy, Debug)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    from: (f32, f32),
    to: (f32, f32),
    age: f32,
    life: f32,
    size: f32,
    color: usize,
    phase: f32,
    mode: VoiceGlowPhase,
}

#[derive(Debug)]
pub(crate) struct StardustPool {
    items: Vec<Particle>,
    spawn_acc: f32,
    rng: u32,
}

impl Default for StardustPool {
    fn default() -> Self {
        Self {
            items: Vec::with_capacity(CAPACITY),
            spawn_acc: 0.0,
            rng: 0x9E37_79B9,
        }
    }
}

impl StardustPool {
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn capacity(&self) -> usize {
        self.items.capacity()
    }

    fn random(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng >> 8) as f32 / (1u32 << 24) as f32
    }

    pub(crate) fn step(&mut self, dt_ms: f32, frame: &GlowFrame) {
        for particle in &mut self.items {
            particle.age += dt_ms;
            advance(particle, dt_ms, frame.t_ms);
        }
        self.items.retain(|particle| particle.age < particle.life);
        if frame.intensity < 0.01 {
            self.items.clear();
        }
        let energy = frame.energy.clamp(0.0, 1.0);
        let rate = match (frame.reduce_motion, frame.phase) {
            (true, _) | (_, VoiceGlowPhase::Idle) => 0.0,
            (_, VoiceGlowPhase::Speaking) => 200.0 * energy,
            (_, VoiceGlowPhase::Listening) => 150.0 * energy,
            (_, VoiceGlowPhase::Thinking) => 12.0,
        };
        self.spawn_acc = (self.spawn_acc + rate * dt_ms / 1000.0).min(1.0 + rate / 60.0);
        while self.spawn_acc >= 1.0 && self.items.len() < CAPACITY {
            self.spawn_acc -= 1.0;
            let particle = self.spawn(frame);
            self.items.push(particle);
        }
        if self.items.len() >= CAPACITY {
            self.spawn_acc = 0.0;
        }
    }

    fn spawn(&mut self, frame: &GlowFrame) -> Particle {
        let dock = frame.dock;
        let (r1, r2, r3, r4) = (self.random(), self.random(), self.random(), self.random());
        let mut particle = Particle {
            x: dock.x + 16.0 + r1 * (dock.w - 32.0).max(1.0),
            y: dock.y,
            vx: 0.0,
            vy: 0.0,
            from: (0.0, 0.0),
            to: (0.0, 0.0),
            age: 0.0,
            life: 1400.0 + r2 * 1600.0,
            size: 0.6 + r3 * 1.8,
            color: (r4 * 4.0) as usize % PALETTE.len(),
            phase: r1 * std::f32::consts::TAU,
            mode: frame.phase,
        };
        let cx = dock.x + dock.w / 2.0;
        match frame.phase {
            VoiceGlowPhase::Speaking => {
                particle.vx = (particle.x - cx) / dock.w * 0.03;
                particle.vy = -(0.03 + r2 * 0.06) * (0.6 + frame.energy);
            }
            VoiceGlowPhase::Listening => {
                let angle = std::f32::consts::PI * (1.05 + r1 * 0.9);
                let radius = dock.w * (0.5 + r3 * 0.5);
                particle.from = (
                    cx + angle.cos() * radius,
                    dock.y + dock.h / 2.0 + angle.sin() * radius * 0.55,
                );
                particle.to = (cx + (r2 - 0.5) * dock.w * 0.6, dock.y + dock.h / 2.0);
                particle.life = 900.0 + r2 * 600.0;
            }
            _ => particle.life = 2600.0,
        }
        particle
    }

    pub(crate) fn build(&self, frame: &GlowFrame) -> Vec<GlowPrim> {
        let intensity = frame.intensity.clamp(0.0, 1.0);
        if intensity < 0.004 && self.items.is_empty() {
            return Vec::new();
        }
        let dock = frame.dock;
        let energy = frame.energy.clamp(0.0, 1.0);
        let mut prims = Vec::with_capacity(self.items.len() * 2 + 1);
        prims.push(GlowPrim::Glow {
            rect: Rect {
                x: dock.x - 20.0,
                y: dock.y,
                w: dock.w + 40.0,
                h: dock.h,
            },
            radius: 26.0,
            blur: 60.0,
            rgba: color(
                LAVENDER,
                0.5 * intensity * frame.gain * (0.3 + 0.7 * energy),
            ),
        });
        for particle in &self.items {
            let (x, y) = position(particle, frame);
            let alpha = (std::f32::consts::PI * particle.age / particle.life)
                .sin()
                .max(0.0)
                * intensity
                * frame.gain;
            let rgb = PALETTE[particle.color];
            let halo = particle.size * 2.6;
            prims.push(GlowPrim::Glow {
                rect: Rect {
                    x: x - halo,
                    y: y - halo,
                    w: halo * 2.0,
                    h: halo * 2.0,
                },
                radius: halo,
                blur: 3.0,
                rgba: color(rgb, 0.8 * alpha),
            });
            prims.push(GlowPrim::Glow {
                rect: Rect {
                    x: x - particle.size,
                    y: y - particle.size,
                    w: particle.size * 2.0,
                    h: particle.size * 2.0,
                },
                radius: particle.size,
                blur: 0.5,
                rgba: color(0xFFFFFF, alpha),
            });
        }
        prims
    }
}

fn advance(particle: &mut Particle, dt_ms: f32, t_ms: f32) {
    if particle.mode == VoiceGlowPhase::Speaking {
        particle.x +=
            particle.vx * dt_ms + (t_ms * 0.002 + particle.phase).sin() * 0.12 * dt_ms / 16.0;
        particle.y += particle.vy * dt_ms;
    }
}

fn position(particle: &Particle, frame: &GlowFrame) -> (f32, f32) {
    let u = (particle.age / particle.life).clamp(0.0, 1.0);
    match particle.mode {
        VoiceGlowPhase::Listening => {
            let k = u * u * (3.0 - 2.0 * u);
            (
                particle.from.0 + (particle.to.0 - particle.from.0) * k,
                particle.from.1 + (particle.to.1 - particle.from.1) * k,
            )
        }
        VoiceGlowPhase::Thinking => {
            let dock = frame.dock;
            let angle = particle.phase + frame.t_ms * 0.0009;
            (
                dock.x + dock.w / 2.0 + angle.cos() * dock.w * 0.56,
                dock.y + dock.h / 2.0 + angle.sin() * dock.h * 1.4,
            )
        }
        _ => (particle.x, particle.y),
    }
}
