use canvas::NodeId;
use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

const FLIGHT_DURATION: Duration = Duration::from_millis(480);
const IMPACT_DURATION: Duration = Duration::from_millis(260);
const REDUCED_PULSE_DURATION: Duration = Duration::from_millis(150);
const MAX_TICK_DELTA: Duration = Duration::from_millis(50);
const LAUNCH_STAGGER: Duration = Duration::from_millis(40);
const MAX_ACTIVE_EFFECTS: usize = 3;
const MAX_PENDING_EFFECTS: usize = 8;
const MAX_DEDUPE_KEYS: usize = 64;
pub(crate) const MAX_PARTICLES_PER_EFFECT: usize = 18;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DispatchReceiptVisualStatus {
    Submitted,
    Queued,
    Untracked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DispatchEffectPhase {
    Flight,
    Impact,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DispatchEffectSpawnOutcome {
    Spawned,
    Queued,
    Duplicate,
    DroppedCapacity,
}

#[cfg(any(test, feature = "e2e-test-support"))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DispatchEffectCounts {
    pub(crate) active: usize,
    pub(crate) pending: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DispatchEffectPresentation {
    pub(crate) target_node_id: NodeId,
    pub(crate) status: DispatchReceiptVisualStatus,
    pub(crate) phase: DispatchEffectPhase,
    pub(crate) progress: f32,
    pub(crate) reduce_motion: bool,
    pub(crate) particles: [DispatchImpactParticle; MAX_PARTICLES_PER_EFFECT],
    pub(crate) particle_count: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct DispatchImpactParticle {
    pub(crate) direction_x: f32,
    pub(crate) direction_y: f32,
    pub(crate) distance: f32,
    pub(crate) size: f32,
    pub(crate) delay: f32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct DispatchEffectDiagnostics {
    pub(super) active: usize,
    pub(super) pending: usize,
    pub(super) particles: usize,
    pub(super) duplicate_skips: u64,
    pub(super) capacity_skips: u64,
    pub(super) unavailable_skips: u64,
    pub(super) cancelled_effects: u64,
    pub(super) reduced_motion_effects: u64,
    pub(super) last_update_elapsed: Duration,
}

#[derive(Debug, Clone)]
pub(super) struct DispatchEffectStore {
    active: Vec<DispatchEffect>,
    pending: VecDeque<DispatchEffect>,
    seen_order: VecDeque<DispatchEffectKey>,
    seen: HashSet<DispatchEffectKey>,
    since_last_launch: Duration,
    metrics: DispatchEffectDiagnostics,
}

impl Default for DispatchEffectStore {
    fn default() -> Self {
        Self {
            active: Vec::with_capacity(MAX_ACTIVE_EFFECTS),
            pending: VecDeque::with_capacity(MAX_PENDING_EFFECTS),
            seen_order: VecDeque::with_capacity(MAX_DEDUPE_KEYS),
            seen: HashSet::with_capacity(MAX_DEDUPE_KEYS),
            since_last_launch: Duration::ZERO,
            metrics: DispatchEffectDiagnostics::default(),
        }
    }
}

impl DispatchEffectStore {
    pub(super) fn spawn(
        &mut self,
        task_ref: String,
        target_node_id: NodeId,
        status: DispatchReceiptVisualStatus,
        reduce_motion: bool,
    ) -> DispatchEffectSpawnOutcome {
        let key = DispatchEffectKey {
            task_ref,
            target_node_id,
        };
        if self.seen.contains(&key) {
            self.metrics.duplicate_skips = self.metrics.duplicate_skips.saturating_add(1);
            return DispatchEffectSpawnOutcome::Duplicate;
        }
        let effect = DispatchEffect::new(&key, status, reduce_motion);
        if self.active.is_empty()
            || (self.active.len() < MAX_ACTIVE_EFFECTS
                && self.pending.is_empty()
                && self.since_last_launch >= LAUNCH_STAGGER)
        {
            self.active.push(effect);
            self.since_last_launch = Duration::ZERO;
            self.remember(key);
            self.record_reduced_motion(reduce_motion);
            return DispatchEffectSpawnOutcome::Spawned;
        }
        if self.pending.len() >= MAX_PENDING_EFFECTS {
            self.remember(key);
            self.metrics.capacity_skips = self.metrics.capacity_skips.saturating_add(1);
            return DispatchEffectSpawnOutcome::DroppedCapacity;
        }
        self.pending.push_back(effect);
        self.remember(key);
        self.record_reduced_motion(reduce_motion);
        DispatchEffectSpawnOutcome::Queued
    }

    pub(super) fn tick(&mut self, delta: Duration) -> bool {
        self.metrics.last_update_elapsed = Duration::ZERO;
        if self.active.is_empty() && self.pending.is_empty() {
            return false;
        }
        let update_started_at = Instant::now();
        let delta = delta.min(MAX_TICK_DELTA);
        for effect in &mut self.active {
            effect.tick(delta);
        }
        self.active.retain(|effect| !effect.finished());
        self.since_last_launch = self
            .since_last_launch
            .saturating_add(delta)
            .min(LAUNCH_STAGGER);
        if self.active.len() < MAX_ACTIVE_EFFECTS && self.since_last_launch >= LAUNCH_STAGGER {
            if let Some(effect) = self.pending.pop_front() {
                self.active.push(effect);
                self.since_last_launch = Duration::ZERO;
            }
        }
        self.metrics.last_update_elapsed = update_started_at.elapsed();
        true
    }

    pub(super) fn is_active(&self) -> bool {
        !self.active.is_empty() || !self.pending.is_empty()
    }

    pub(super) fn target_node_ids(&self) -> Vec<NodeId> {
        self.active
            .iter()
            .chain(&self.pending)
            .map(|effect| effect.target_node_id)
            .collect()
    }

    pub(super) fn retain_targets(&mut self, mut keep: impl FnMut(NodeId) -> bool) {
        let retained_before = self.active.len() + self.pending.len();
        self.active.retain(|effect| keep(effect.target_node_id));
        self.pending.retain(|effect| keep(effect.target_node_id));
        let retained_after = self.active.len() + self.pending.len();
        self.metrics.cancelled_effects = self
            .metrics
            .cancelled_effects
            .saturating_add((retained_before - retained_after) as u64);
    }

    pub(super) fn presentations(&self) -> Vec<DispatchEffectPresentation> {
        self.active
            .iter()
            .map(DispatchEffect::presentation)
            .collect()
    }

    #[cfg(any(test, feature = "e2e-test-support"))]
    pub(super) fn counts(&self) -> DispatchEffectCounts {
        DispatchEffectCounts {
            active: self.active.len(),
            pending: self.pending.len(),
        }
    }

    pub(super) fn diagnostics(&self) -> DispatchEffectDiagnostics {
        DispatchEffectDiagnostics {
            active: self.active.len(),
            pending: self.pending.len(),
            particles: self.active.iter().map(|effect| effect.particle_count).sum(),
            ..self.metrics
        }
    }

    pub(super) fn record_unavailable_skip(&mut self) {
        self.metrics.unavailable_skips = self.metrics.unavailable_skips.saturating_add(1);
    }

    fn remember(&mut self, key: DispatchEffectKey) {
        self.seen.insert(key.clone());
        self.seen_order.push_back(key);
        if self.seen_order.len() > MAX_DEDUPE_KEYS {
            if let Some(expired) = self.seen_order.pop_front() {
                self.seen.remove(&expired);
            }
        }
    }

    fn record_reduced_motion(&mut self, reduce_motion: bool) {
        if reduce_motion {
            self.metrics.reduced_motion_effects =
                self.metrics.reduced_motion_effects.saturating_add(1);
        }
    }
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
struct DispatchEffectKey {
    task_ref: String,
    target_node_id: NodeId,
}

#[derive(Debug, Clone)]
struct DispatchEffect {
    target_node_id: NodeId,
    status: DispatchReceiptVisualStatus,
    phase: DispatchEffectPhase,
    elapsed: Duration,
    reduce_motion: bool,
    particles: [DispatchImpactParticle; MAX_PARTICLES_PER_EFFECT],
    particle_count: usize,
}

impl DispatchEffect {
    fn new(
        key: &DispatchEffectKey,
        status: DispatchReceiptVisualStatus,
        reduce_motion: bool,
    ) -> Self {
        let particle_count = if reduce_motion {
            0
        } else {
            match status {
                DispatchReceiptVisualStatus::Submitted => 18,
                DispatchReceiptVisualStatus::Queued => 12,
                DispatchReceiptVisualStatus::Untracked => 14,
            }
        };
        Self {
            target_node_id: key.target_node_id,
            status,
            phase: if reduce_motion {
                DispatchEffectPhase::Impact
            } else {
                DispatchEffectPhase::Flight
            },
            elapsed: Duration::ZERO,
            reduce_motion,
            particles: build_particles(key),
            particle_count,
        }
    }

    fn tick(&mut self, delta: Duration) {
        self.elapsed = self.elapsed.saturating_add(delta);
        if self.phase == DispatchEffectPhase::Flight && self.elapsed >= FLIGHT_DURATION {
            self.elapsed = self.elapsed.saturating_sub(FLIGHT_DURATION);
            self.phase = DispatchEffectPhase::Impact;
        }
    }

    fn finished(&self) -> bool {
        let impact_duration = if self.reduce_motion {
            REDUCED_PULSE_DURATION
        } else {
            IMPACT_DURATION
        };
        self.phase == DispatchEffectPhase::Impact && self.elapsed >= impact_duration
    }

    fn presentation(&self) -> DispatchEffectPresentation {
        let duration = match self.phase {
            DispatchEffectPhase::Flight => FLIGHT_DURATION,
            DispatchEffectPhase::Impact if self.reduce_motion => REDUCED_PULSE_DURATION,
            DispatchEffectPhase::Impact => IMPACT_DURATION,
        };
        DispatchEffectPresentation {
            target_node_id: self.target_node_id,
            status: self.status,
            phase: self.phase,
            progress: (self.elapsed.as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0),
            reduce_motion: self.reduce_motion,
            particles: self.particles,
            particle_count: self.particle_count,
        }
    }
}

fn build_particles(key: &DispatchEffectKey) -> [DispatchImpactParticle; MAX_PARTICLES_PER_EFFECT] {
    let mut seed = 0xcbf29ce484222325_u64;
    for byte in key.task_ref.bytes().chain(key.target_node_id.to_le_bytes()) {
        seed ^= u64::from(byte);
        seed = seed.wrapping_mul(0x100000001b3);
    }
    let mut particles = [DispatchImpactParticle::default(); MAX_PARTICLES_PER_EFFECT];
    for (index, particle) in particles.iter_mut().enumerate() {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let jitter = ((seed >> 32) as u32) as f32 / u32::MAX as f32;
        let angle = std::f32::consts::TAU
            * (index as f32 / MAX_PARTICLES_PER_EFFECT as f32 + jitter * 0.035);
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let distance_jitter = ((seed >> 32) as u32) as f32 / u32::MAX as f32;
        *particle = DispatchImpactParticle {
            direction_x: angle.cos(),
            direction_y: angle.sin(),
            distance: 28.0 + distance_jitter * 34.0,
            size: 2.0 + (index % 3) as f32 * 0.7,
            delay: (index % 4) as f32 * 0.035,
        };
    }
    particles
}
