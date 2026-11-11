use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::{TtsPlaybackEvent, TtsPlaybackSource, VoiceWindowInsetVisual};
use crate::effects::voice_glow::scene::Rect;
use crate::effects::voice_glow::{VoiceGlowPhase, VoiceGlowPrefs};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct VoiceWindowInsetSnapshot {
    pub(crate) visual: Option<VoiceWindowInsetVisual>,
    pub(crate) corner_seam_guard_alpha: f32,
}

#[derive(Debug)]
struct QueuedPlaybackEvent {
    source: TtsPlaybackSource,
    event: TtsPlaybackEvent,
}

#[derive(Debug)]
struct SharedVoiceWindowInsetEvents {
    queue: Mutex<VecDeque<QueuedPlaybackEvent>>,
    pending_count: AtomicUsize,
    enabled: AtomicBool,
    animation_active: AtomicBool,
    snapshot: Mutex<VoiceWindowInsetSnapshot>,
    prefs: Mutex<VoiceGlowPrefs>,
    phase: AtomicU8,
    dock: Mutex<Option<Rect>>,
    wake_tx: async_channel::Sender<()>,
}

#[derive(Clone, Debug)]
pub(crate) struct VoiceWindowInsetEventBus {
    shared: Arc<SharedVoiceWindowInsetEvents>,
}

#[derive(Clone, Debug)]
pub(crate) struct VoiceWindowInsetEventSink {
    bus: VoiceWindowInsetEventBus,
    source: TtsPlaybackSource,
}

impl VoiceWindowInsetEventBus {
    pub(crate) fn new() -> (Self, async_channel::Receiver<()>) {
        let (wake_tx, wake_rx) = async_channel::bounded(1);
        (
            Self {
                shared: Arc::new(SharedVoiceWindowInsetEvents {
                    queue: Mutex::new(VecDeque::new()),
                    pending_count: AtomicUsize::new(0),
                    enabled: AtomicBool::new(true),
                    animation_active: AtomicBool::new(false),
                    snapshot: Mutex::new(VoiceWindowInsetSnapshot::default()),
                    prefs: Mutex::new(VoiceGlowPrefs::default()),
                    phase: AtomicU8::new(0),
                    dock: Mutex::new(None),
                    wake_tx,
                }),
            },
            wake_rx,
        )
    }

    pub(crate) fn sink(&self, source: TtsPlaybackSource) -> VoiceWindowInsetEventSink {
        VoiceWindowInsetEventSink {
            bus: self.clone(),
            source,
        }
    }

    pub(crate) fn push(&self, source: TtsPlaybackSource, event: TtsPlaybackEvent) {
        let mut queue = self
            .shared
            .queue
            .lock()
            .expect("Voice inset event queue lock should not be poisoned");
        queue.push_back(QueuedPlaybackEvent { source, event });
        self.shared.pending_count.fetch_add(1, Ordering::Release);
        drop(queue);
        let _ = self.shared.wake_tx.try_send(());
    }

    pub(crate) fn drain(&self) -> Vec<(TtsPlaybackSource, TtsPlaybackEvent)> {
        let mut queue = self
            .shared
            .queue
            .lock()
            .expect("Voice inset event queue lock should not be poisoned");
        let drained = queue
            .drain(..)
            .map(|queued| (queued.source, queued.event))
            .collect::<Vec<_>>();
        self.shared.pending_count.store(0, Ordering::Release);
        drop(queue);
        drained
    }

    pub(crate) fn set_enabled(&self, enabled: bool) -> bool {
        let changed = self.shared.enabled.swap(enabled, Ordering::AcqRel) != enabled;
        if changed {
            let _ = self.shared.wake_tx.try_send(());
        }
        changed
    }

    /// 偏好变化时唤醒视图；返回是否真的变了。
    pub(crate) fn set_prefs(&self, prefs: VoiceGlowPrefs) -> bool {
        let mut current = self
            .shared
            .prefs
            .lock()
            .expect("Voice glow prefs lock should not be poisoned");
        let changed = *current != prefs;
        *current = prefs;
        drop(current);
        if changed {
            let _ = self.shared.wake_tx.try_send(());
        }
        changed
    }

    pub(crate) fn prefs(&self) -> VoiceGlowPrefs {
        *self
            .shared
            .prefs
            .lock()
            .expect("Voice glow prefs lock should not be poisoned")
    }

    /// 聆听/思考由各语音源汇总后写入；回复由 TTS 事件驱动，不走这里。
    pub(crate) fn set_phase(&self, phase: VoiceGlowPhase) -> bool {
        let code = match phase {
            VoiceGlowPhase::Idle => 0,
            VoiceGlowPhase::Listening => 1,
            VoiceGlowPhase::Thinking => 2,
            VoiceGlowPhase::Speaking => 3,
        };
        let changed = self.shared.phase.swap(code, Ordering::AcqRel) != code;
        if changed {
            let _ = self.shared.wake_tx.try_send(());
        }
        changed
    }

    pub(crate) fn phase(&self) -> VoiceGlowPhase {
        match self.shared.phase.load(Ordering::Acquire) {
            1 => VoiceGlowPhase::Listening,
            2 => VoiceGlowPhase::Thinking,
            3 => VoiceGlowPhase::Speaking,
            _ => VoiceGlowPhase::Idle,
        }
    }

    /// Dock 在窗口坐标里的矩形，由 Dock 的 canvas 探针写入；相同值不重复写。
    pub(crate) fn set_dock(&self, rect: Rect) {
        let mut dock = self
            .shared
            .dock
            .lock()
            .expect("Voice glow dock lock should not be poisoned");
        if *dock != Some(rect) {
            *dock = Some(rect);
        }
    }

    pub(crate) fn dock(&self) -> Option<Rect> {
        *self
            .shared
            .dock
            .lock()
            .expect("Voice glow dock lock should not be poisoned")
    }

    pub(crate) fn enabled(&self) -> bool {
        self.shared.enabled.load(Ordering::Acquire)
    }

    pub(crate) fn set_animation_active(&self, active: bool) {
        self.shared
            .animation_active
            .store(active, Ordering::Release);
    }

    pub(crate) fn is_animation_active(&self) -> bool {
        self.shared.animation_active.load(Ordering::Acquire)
            || self.shared.pending_count.load(Ordering::Acquire) > 0
    }

    pub(crate) fn publish_snapshot(&self, snapshot: VoiceWindowInsetSnapshot) {
        *self
            .shared
            .snapshot
            .lock()
            .expect("Voice inset snapshot lock should not be poisoned") = snapshot;
    }

    pub(crate) fn snapshot(&self) -> VoiceWindowInsetSnapshot {
        *self
            .shared
            .snapshot
            .lock()
            .expect("Voice inset snapshot lock should not be poisoned")
    }
}

impl VoiceWindowInsetEventSink {
    pub(crate) fn push(&self, event: TtsPlaybackEvent) {
        self.bus.push(self.source, event);
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/voice_window_inset_channel_tests.rs"]
mod tests;
